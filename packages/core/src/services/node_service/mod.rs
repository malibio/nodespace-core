//! NodeService — public facade and shared infrastructure.
//!
//! This file owns:
//! - The `NodeService` struct definition and `Clone` impl
//! - Construction (`new`, `seed_*`) and accessors (`store`, `behaviors`, `with_client`,
//!   `subscribe_to_events`)
//! - Hierarchy methods (Phase 4 — kept here for now; will move to `hierarchy.rs`)
//! - `NodeAccessor` trait impl
//! - Module declarations for the focused sub-modules:
//!   `crud`, `relationship`, `schema`, `bulk`, `query`, `embedding`
//! - Shared helper functions (`compute_property_changes`, `extract_mentions`, etc.)
//! - All integration tests
//!
//! # Root Node Detection
//!
//! Root nodes are identified by `root_id IS NULL`. Never use `node_type == "topic"`.
//!
//! Examples:
//! - Root node: `root_id = NULL` (e.g., @mention pages, date nodes)
//! - Child node: `root_id = Some("parent-id")` (e.g., notes within a topic)

use crate::behaviors::NodeBehaviorRegistry;
use crate::db::events::DomainEvent;
use crate::db::{SqliteStore, StoreChange, StoreOperation};
use crate::models::{FilterOperator, Node, NodeFilter, NodeUpdate, PropertyFilter};
use crate::services::error::NodeServiceError;
use crate::services::migration_registry::MigrationRegistry;
use crate::services::NodeAccessor;
use async_trait::async_trait;
use regex::Regex;
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use tokio::sync::broadcast;

// Sub-module declarations
pub mod access_gate;
pub(crate) mod bulk;
pub(crate) mod crud;
pub(crate) mod embedding;
pub(crate) mod hierarchy;
pub(crate) mod query;
pub(crate) mod relationship;
pub(crate) mod schema;

pub use hierarchy::flatten_subtree_content;

/// Reserved ID for the DatabaseSettingsNode singleton instance.
///
/// The `database-settings` schema node stores its own id as the slug
/// `"database-settings"`, so the instance must use a distinct reserved id to
/// avoid colliding with it. Seeding and the singleton idempotency guard both
/// key off this constant so the node is deterministic and created at most once.
pub(crate) const DATABASE_SETTINGS_NODE_ID: &str = "database-settings-singleton";

/// Compute property changes between pre-mutation and post-mutation node properties
///
/// Diffs the top-level keys within each namespace. For namespaced properties
/// (e.g., `{"task": {"status": "done"}}`), diffs within each namespace object,
/// producing keys like `"task.status"`. Only handles single-level nesting
/// (namespace → property), matching NodeSpace's storage format where properties
/// are stored as `{ "node_type": { "prop": value } }`.
///
/// Returns a `Vec<PropertyChange>` describing what changed.
fn compute_property_changes(old: &Value, new: &Value) -> Vec<crate::db::events::PropertyChange> {
    use crate::db::events::PropertyChange;

    let mut changes = Vec::new();

    let old_obj = match old.as_object() {
        Some(o) => o,
        None => return changes,
    };
    let new_obj = match new.as_object() {
        Some(o) => o,
        None => return changes,
    };

    // Collect all keys from both old and new
    let mut all_keys: HashSet<&String> = old_obj.keys().collect();
    all_keys.extend(new_obj.keys());

    for key in all_keys {
        let old_val = old_obj.get(key);
        let new_val = new_obj.get(key);

        match (old_val, new_val) {
            (Some(ov), Some(nv)) => {
                // Both exist — check if the value changed
                if ov != nv {
                    // If both are objects (namespace), diff their contents
                    if let (Some(old_ns), Some(new_ns)) = (ov.as_object(), nv.as_object()) {
                        let mut ns_keys: HashSet<&String> = old_ns.keys().collect();
                        ns_keys.extend(new_ns.keys());
                        for ns_key in ns_keys {
                            let old_ns_val = old_ns.get(ns_key);
                            let new_ns_val = new_ns.get(ns_key);
                            if old_ns_val != new_ns_val {
                                changes.push(PropertyChange {
                                    key: format!("{}.{}", key, ns_key),
                                    old_value: old_ns_val.cloned(),
                                    new_value: new_ns_val.cloned(),
                                });
                            }
                        }
                    } else {
                        // Scalar change at top level
                        changes.push(PropertyChange {
                            key: key.clone(),
                            old_value: Some(ov.clone()),
                            new_value: Some(nv.clone()),
                        });
                    }
                }
            }
            (Some(ov), None) => {
                // Property removed
                changes.push(PropertyChange {
                    key: key.clone(),
                    old_value: Some(ov.clone()),
                    new_value: None,
                });
            }
            (None, Some(nv)) => {
                // Property added
                changes.push(PropertyChange {
                    key: key.clone(),
                    old_value: None,
                    new_value: Some(nv.clone()),
                });
            }
            (None, None) => unreachable!(),
        }
    }

    changes
}

#[cfg(test)]
mod property_change_tests {
    use super::*;
    use serde_json::json;

    /// Helper: find a PropertyChange by key in the result vec.
    fn find_change<'a>(
        changes: &'a [crate::db::events::PropertyChange],
        key: &str,
    ) -> Option<&'a crate::db::events::PropertyChange> {
        changes.iter().find(|c| c.key == key)
    }

    #[test]
    fn test_no_changes() {
        let old = json!({"title": "hello", "task": {"status": "todo"}});
        let new = json!({"title": "hello", "task": {"status": "todo"}});
        let changes = compute_property_changes(&old, &new);
        assert!(
            changes.is_empty(),
            "identical objects should produce no changes"
        );
    }

    #[test]
    fn test_scalar_property_changed() {
        let old = json!({"title": "old title", "color": "red"});
        let new = json!({"title": "new title", "color": "red"});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "title");
        assert_eq!(c.old_value, Some(json!("old title")));
        assert_eq!(c.new_value, Some(json!("new title")));
    }

    #[test]
    fn test_scalar_property_changed_number() {
        let old = json!({"count": 1});
        let new = json!({"count": 42});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "count");
        assert_eq!(c.old_value, Some(json!(1)));
        assert_eq!(c.new_value, Some(json!(42)));
    }

    #[test]
    fn test_namespace_inner_property_changed() {
        let old = json!({"task": {"status": "todo", "priority": "low"}});
        let new = json!({"task": {"status": "done", "priority": "low"}});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "task.status");
        assert_eq!(c.old_value, Some(json!("todo")));
        assert_eq!(c.new_value, Some(json!("done")));
    }

    #[test]
    fn test_namespace_multiple_inner_changes() {
        let old = json!({"task": {"status": "todo", "priority": "low"}});
        let new = json!({"task": {"status": "done", "priority": "high"}});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 2);
        let status = find_change(&changes, "task.status").expect("should have task.status change");
        assert_eq!(status.old_value, Some(json!("todo")));
        assert_eq!(status.new_value, Some(json!("done")));

        let priority =
            find_change(&changes, "task.priority").expect("should have task.priority change");
        assert_eq!(priority.old_value, Some(json!("low")));
        assert_eq!(priority.new_value, Some(json!("high")));
    }

    #[test]
    fn test_namespace_inner_property_added() {
        let old = json!({"task": {"status": "todo"}});
        let new = json!({"task": {"status": "todo", "due": "2026-04-01"}});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "task.due");
        assert_eq!(c.old_value, None);
        assert_eq!(c.new_value, Some(json!("2026-04-01")));
    }

    #[test]
    fn test_namespace_inner_property_removed() {
        let old = json!({"task": {"status": "todo", "due": "2026-04-01"}});
        let new = json!({"task": {"status": "todo"}});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "task.due");
        assert_eq!(c.old_value, Some(json!("2026-04-01")));
        assert_eq!(c.new_value, None);
    }

    #[test]
    fn test_property_added() {
        let old = json!({"title": "hello"});
        let new = json!({"title": "hello", "color": "blue"});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "color");
        assert_eq!(c.old_value, None);
        assert_eq!(c.new_value, Some(json!("blue")));
    }

    #[test]
    fn test_property_removed() {
        let old = json!({"title": "hello", "color": "blue"});
        let new = json!({"title": "hello"});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "color");
        assert_eq!(c.old_value, Some(json!("blue")));
        assert_eq!(c.new_value, None);
    }

    #[test]
    fn test_non_object_inputs_return_empty() {
        // old is not an object
        let changes = compute_property_changes(&json!("string"), &json!({"a": 1}));
        assert!(changes.is_empty(), "non-object old should return empty");

        // new is not an object
        let changes = compute_property_changes(&json!({"a": 1}), &json!(42));
        assert!(changes.is_empty(), "non-object new should return empty");

        // both non-objects
        let changes = compute_property_changes(&json!(null), &json!(true));
        assert!(changes.is_empty(), "both non-object should return empty");

        // null values
        let changes = compute_property_changes(&json!(null), &json!(null));
        assert!(changes.is_empty(), "null inputs should return empty");
    }

    #[test]
    fn test_both_empty_objects() {
        let changes = compute_property_changes(&json!({}), &json!({}));
        assert!(
            changes.is_empty(),
            "two empty objects should produce no changes"
        );
    }

    #[test]
    fn test_mixed_scalar_and_namespace_changes() {
        let old = json!({
            "title": "old",
            "task": {"status": "todo"}
        });
        let new = json!({
            "title": "new",
            "task": {"status": "done"}
        });
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 2);
        let title = find_change(&changes, "title").expect("should have title change");
        assert_eq!(title.old_value, Some(json!("old")));
        assert_eq!(title.new_value, Some(json!("new")));

        let status = find_change(&changes, "task.status").expect("should have task.status change");
        assert_eq!(status.old_value, Some(json!("todo")));
        assert_eq!(status.new_value, Some(json!("done")));
    }

    #[test]
    fn test_type_change_scalar_to_object() {
        // old has a scalar, new has an object for the same key — treated as scalar change
        // because old value is not an object for that key
        let old = json!({"task": "simple string"});
        let new = json!({"task": {"status": "todo"}});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "task");
        assert_eq!(c.old_value, Some(json!("simple string")));
        assert_eq!(c.new_value, Some(json!({"status": "todo"})));
    }

    #[test]
    fn test_type_change_object_to_scalar() {
        let old = json!({"task": {"status": "todo"}});
        let new = json!({"task": "collapsed"});
        let changes = compute_property_changes(&old, &new);

        assert_eq!(changes.len(), 1);
        let c = &changes[0];
        assert_eq!(c.key, "task");
        assert_eq!(c.old_value, Some(json!({"status": "todo"})));
        assert_eq!(c.new_value, Some(json!("collapsed")));
    }
}

/// Default limit for query_nodes_simple when no limit is specified.
/// Prevents accidental full table scans and improves performance.
pub const DEFAULT_QUERY_LIMIT: usize = 100;

/// Type alias for subtree data returned by `get_subtree_data`
///
/// Contains (root_node, node_map, adjacency_list) where:
/// - root_node: Option<Node> - the root node if it exists
/// - node_map: HashMap<String, Node> - all nodes indexed by ID
/// - adjacency_list: HashMap<String, Vec<String>> - children IDs indexed by parent ID (sorted by order)
pub type SubtreeData = (
    Option<Node>,
    HashMap<String, Node>,
    HashMap<String, Vec<String>>,
);

/// Parameters for creating a node
///
/// This struct is used by `NodeService::create_node_with_parent()` to encapsulate
/// all parameters needed for node creation.
///
/// # ID Generation Strategy
///
/// The `id` field supports three distinct scenarios:
///
/// 1. **Frontend-provided UUID** (Tauri commands): The frontend pre-generates UUIDs for
///    optimistic UI updates and local state tracking (`persistedNodeIds`). This ensures
///    ID consistency between client and server, preventing sync issues.
///
/// 2. **Auto-generated UUID** (MCP handlers): Server-side generation for external clients
///    like AI assistants. This prevents ID conflicts and maintains security boundaries.
///
/// 3. **Date-based ID** (special case): Date nodes use their content (YYYY-MM-DD format)
///    as the ID, enabling predictable lookups and ensuring uniqueness by date.
///
/// # Security Considerations
///
/// When accepting frontend-provided IDs:
///
/// - **UUID validation**: Non-date nodes must provide valid UUID format. Invalid UUIDs
///   are rejected with `InvalidOperation` error.
/// - **Database constraints**: The database enforces UNIQUE constraint on `nodes.id`,
///   preventing collisions at the storage layer.
/// - **Trust boundary**: Only Tauri commands (trusted in-process frontend) can provide
///   custom IDs. MCP handlers (external AI clients) always use server-side generation.
///
/// # Examples
///
/// ```no_run
/// # use nodespace_core::services::{CreateNodeParams, InsertPositionOwned};
/// # use serde_json::json;
/// // Auto-generated ID (MCP path)
/// let params = CreateNodeParams {
///     id: None,
///     node_type: "text".to_string(),
///     content: "Hello World".to_string(),
///     parent_id: Some("parent-123".to_string()),
///     position: InsertPositionOwned::Beginning,
///     properties: json!({}),
/// };
///
/// // Frontend-provided UUID (Tauri path)
/// let frontend_id = uuid::Uuid::new_v4().to_string();
/// let params_with_id = CreateNodeParams {
///     id: Some(frontend_id),
///     node_type: "text".to_string(),
///     content: "Tracked by frontend".to_string(),
///     parent_id: None,
///     position: InsertPositionOwned::Beginning,
///     properties: json!({}),
/// };
/// ```
#[derive(Debug, Clone)]
pub struct CreateNodeParams {
    /// Optional ID for the node. If None, will be auto-generated (UUID for most types, content for date nodes)
    pub id: Option<String>,
    /// Type of the node (text, task, date, etc.)
    pub node_type: String,
    /// Content of the node
    pub content: String,
    /// Optional parent node ID (container/root will be auto-derived from parent chain)
    pub parent_id: Option<String>,
    /// Where to insert the new node among the parent's children.
    /// Callers that want append-at-end should use `InsertPositionOwned::End`.
    pub position: crate::services::InsertPositionOwned,
    /// Additional node properties as JSON
    pub properties: Value,
}

/// Broadcast channel capacity for domain events — the LIVE channel `emit_event`
/// publishes on and every consumer (`subscribe_to_events`) reads, including the
/// cloud-sync push consumer.
///
/// It must comfortably buffer a bulk import's burst: relationship events are not
/// node-keyed, so they broadcast immediately (they bypass the batch guard), and
/// the push consumer replicates each `member_of` edge with its own network
/// round-trip, so it drains slower than a batch import emits. A broadcast
/// receiver only misses events if it falls MORE than this many behind, so a burst
/// larger than the buffer makes the push consumer lag and DROP edges (memberships
/// then never reach cloud, leaving those collections empty on other devices). 128
/// overflowed on a ~320-edge collection import; 4096 covers a realistic
/// multi-collection import while staying tiny in memory (each envelope is a few
/// small ids).
const DOMAIN_EVENT_CHANNEL_CAPACITY: usize = 4096;

/// Internal state shared between the store notifier closure and `BatchEmitGuard`.
///
/// `Immediate` — every event is broadcast as it arrives (default).
/// `Batching` — events accumulate in the map; last-write-wins per node_id.
pub(crate) enum BatchState {
    Immediate,
    Batching(HashMap<String, crate::db::events::EventEnvelope>),
}

/// Whether an event envelope should be forwarded to the origin-filtered push
/// channel.
///
/// Every envelope forwards except those whose source client matches the
/// configured excluded origin (when one is set). The excluded origin is
/// injected by the host layer; core assumes no particular id, so until an
/// origin is configured the push channel mirrors the main channel.
///
/// Correctness depends on the excluded writer propagating its client id onto the
/// event's `source_client_id`. Per-node writes (create/update/delete) do this;
/// `bulk_create_hierarchy` and `create_node_streaming` deliberately stamp a fixed
/// source and would NOT carry the excluded origin — a writer that must be excluded
/// here must not route through those.
fn push_forward_allowed(
    excluded: &RwLock<Option<String>>,
    envelope: &crate::db::events::EventEnvelope,
) -> bool {
    let guard = excluded.read().unwrap_or_else(|e| e.into_inner());
    match guard.as_deref() {
        Some(ex) => envelope.metadata.source_client_id.as_deref() != Some(ex),
        None => true,
    }
}

/// RAII guard returned by `NodeService::begin_batch_emit`.
///
/// While this guard is live, domain events emitted by the store notifier are
/// coalesced per node (last-write-wins) instead of broadcast individually.
/// On `Drop` the accumulated events are flushed to the broadcast channel —
/// at most one event per node.
pub struct BatchEmitGuard {
    state: Arc<Mutex<BatchState>>,
    tx: broadcast::Sender<crate::db::events::EventEnvelope>,
    /// Origin-filtered mirror of `tx`; see `NodeService::push_event_tx`.
    push_tx: broadcast::Sender<crate::db::events::EventEnvelope>,
    /// Origin excluded from the push channel; see `NodeService::push_excluded_origin`.
    push_excluded_origin: Arc<RwLock<Option<String>>>,
}

impl Drop for BatchEmitGuard {
    fn drop(&mut self) {
        let mut lock = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::mem::replace(&mut *lock, BatchState::Immediate);
        if let BatchState::Batching(buf) = prev {
            for envelope in buf.into_values() {
                // Mirror to the push channel unless this envelope's origin is
                // excluded. Clone only when forwarding to avoid an extra copy.
                if push_forward_allowed(&self.push_excluded_origin, &envelope) {
                    let _ = self.push_tx.send(envelope.clone());
                }
                let _ = self.tx.send(envelope);
            }
        }
    }
}

/// Check if a string matches date node format: YYYY-MM-DD
///
/// Valid examples: "2025-10-13", "2024-01-01"
/// Invalid examples: "abcd-ef-gh", "2025-10-1", "25-10-13", "2025-13-45" (invalid date)
///
/// This function validates both format AND semantic validity:
/// - Format: YYYY-MM-DD pattern (10 chars, correct positions for digits/dashes)
/// - Semantics: Must be a valid calendar date (no month 13, no day 45, etc.)
fn is_date_node_id(id: &str) -> bool {
    // Must be exactly 10 characters: YYYY-MM-DD
    if id.len() != 10 {
        return false;
    }

    // Check format: 4 digits, dash, 2 digits, dash, 2 digits
    let bytes = id.as_bytes();
    let format_valid = bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && bytes[7] == b'-'
        && bytes[8].is_ascii_digit()
        && bytes[9].is_ascii_digit();

    if !format_valid {
        return false;
    }

    // Semantic validation: Verify it's a valid calendar date
    // This prevents accepting strings like "2025-13-45" (invalid month/day)
    chrono::NaiveDate::parse_from_str(id, "%Y-%m-%d").is_ok()
}

/// Check if a node is a root node based on its root_id
///
/// Root nodes are identified by having a NULL root_id in the database.
/// This is the ONLY correct way to detect root nodes.
///
/// # Arguments
///
/// * `root_id` - The root_id field from a Node
///
/// # Returns
///
/// `true` if the node is a root (root_id is None), `false` otherwise
///
/// # Examples
///
/// ```
/// # use nodespace_core::services::node_service::is_root_node;
/// assert!(is_root_node(&None)); // Root node
/// assert!(!is_root_node(&Some("parent-id".to_string()))); // Child node
/// ```
pub fn is_root_node(root_id: &Option<String>) -> bool {
    root_id.is_none()
}

// Regex pattern for UUID validation (lowercase hex with standard UUID format)
const UUID_PATTERN: &str = r"^[a-f0-9]{8}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{4}-[a-f0-9]{12}$";

// Regex pattern for date validation (YYYY-MM-DD format)
const DATE_PATTERN: &str = r"^\d{4}-\d{2}-\d{2}$";

// Regex pattern for markdown-style nodespace links
// Matches: [@text](nodespace://uuid) or [text](nodespace://node/uuid?params)
// Capture group 1: the node ID (without "node/" prefix or query params)
const MARKDOWN_MENTION_PATTERN: &str =
    r"\[[^\]]+\]\(nodespace://(?:node/)?([^\s)?]+)(?:\?[^)]*)?\)";

// Regex pattern for plain nodespace URIs
// Matches: nodespace://uuid or nodespace://node/uuid
// Capture group 1: the node ID (without "node/" prefix)
const PLAIN_MENTION_PATTERN: &str = r"nodespace://(?:node/)?([^\s)?]+)";

// Regex pattern for `[[node-id]]` wikilink references — an ergonomic shorthand for
// referencing a node by its id (as opposed to the full `[text](nodespace://id)`
// markdown form). Matches: [[uuid]] or [[node/uuid]]. Capture group 1: the node ID
// (without the "node/" prefix). The id class excludes `]` and whitespace, so a
// bracketed phrase like `[[some title]]` never matches; the captured token is then
// validated by `is_valid_node_id`, so only real UUID/date ids become mentions.
const WIKILINK_MENTION_PATTERN: &str = r"\[\[(?:node/)?([^\]\s]+)\]\]";

/// Validate if a node ID is valid (UUID or date format)
///
/// Valid formats:
/// - UUID: 36-character hex string with dashes (e.g., "abc123-...")
/// - Date: YYYY-MM-DD format (e.g., "2025-10-24")
///
/// # Examples
///
/// ```
/// # use nodespace_core::services::node_service::is_valid_node_id;
/// assert!(is_valid_node_id("550e8400-e29b-41d4-a716-446655440000")); // UUID
/// assert!(is_valid_node_id("2025-10-24")); // Date
/// assert!(!is_valid_node_id("invalid")); // Invalid
/// ```
pub fn is_valid_node_id(node_id: &str) -> bool {
    // Check if it's a UUID (36 characters, hex with dashes)
    static UUID_REGEX: OnceLock<Regex> = OnceLock::new();
    let uuid_regex = UUID_REGEX.get_or_init(|| Regex::new(UUID_PATTERN).unwrap());

    if uuid_regex.is_match(node_id) {
        return true;
    }

    // Check if it's a valid date format (YYYY-MM-DD)
    static DATE_REGEX: OnceLock<Regex> = OnceLock::new();
    let date_regex = DATE_REGEX.get_or_init(|| Regex::new(DATE_PATTERN).unwrap());

    if date_regex.is_match(node_id) {
        // Validate it's an actual valid date using chrono
        if let Ok(date) = chrono::NaiveDate::parse_from_str(node_id, "%Y-%m-%d") {
            // Verify roundtrip: parsing and formatting back should give same string
            return date.format("%Y-%m-%d").to_string() == node_id;
        }
    }

    false
}

/// Derive a stable schema node ID from the schema's display name.
///
/// Schema nodes use their normalized name as ID (e.g. "Invoice" → "invoice",
/// "Customer Profile" → "customer_profile") so they can be referenced
/// predictably by type name rather than an opaque UUID.
pub(crate) fn normalize_schema_id(name: &str) -> String {
    name.to_lowercase()
        .replace([' ', '-'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

#[cfg(test)]
mod normalize_schema_id_tests {
    use super::*;

    #[test]
    fn test_normalize_schema_id_basic() {
        assert_eq!(normalize_schema_id("Invoice"), "invoice");
        assert_eq!(normalize_schema_id("Customer Profile"), "customer_profile");
        assert_eq!(normalize_schema_id("code_block"), "code_block");
        assert_eq!(normalize_schema_id("My Widget"), "my_widget");
    }

    #[test]
    fn test_normalize_schema_id_edge_cases() {
        assert_eq!(normalize_schema_id("  spaces  "), "spaces");
        assert_eq!(normalize_schema_id("already-kebab"), "already_kebab");
        assert_eq!(normalize_schema_id("UPPER CASE"), "upper_case");
    }
}

/// Extract nodespace:// mentions from content
///
/// Supports markdown links, plain URIs, and `[[node-id]]` wikilinks:
/// - Markdown: [@text](nodespace://node-id) or [text](nodespace://node-id)
/// - Plain: nodespace://node-id
/// - Wikilink: [[node-id]] (shorthand reference by id)
///
/// Accepts both UUID and date format node IDs:
/// - UUID: abc123-def456-... (36 chars)
/// - Date: 2025-10-24 (YYYY-MM-DD format)
///
/// Returns array of unique mentioned node IDs (duplicates removed).
///
/// # Performance
///
/// - **Time Complexity:** O(n × m) where n = content length, m = number of markdown links
/// - **Space Complexity:** O(k) where k = unique mentions found
/// - **Typical Performance:** ~1-5µs for content <1000 chars with <10 mentions
///
/// # Examples
///
/// ```
/// # use nodespace_core::services::node_service::extract_mentions;
/// let content = "See [@Node](nodespace://550e8400-e29b-41d4-a716-446655440000) and nodespace://2025-10-24";
/// let mentions = extract_mentions(content);
/// assert_eq!(mentions.len(), 2);
/// ```
pub fn extract_mentions(content: &str) -> Vec<String> {
    let mut mentions = HashSet::new();

    // Match markdown format using the defined pattern
    static MARKDOWN_REGEX: OnceLock<Regex> = OnceLock::new();
    let markdown_regex =
        MARKDOWN_REGEX.get_or_init(|| Regex::new(MARKDOWN_MENTION_PATTERN).unwrap());

    for cap in markdown_regex.captures_iter(content) {
        if let Some(node_id) = cap.get(1) {
            let node_id_str = node_id.as_str();
            if is_valid_node_id(node_id_str) {
                mentions.insert(node_id_str.to_string());
            }
        }
    }

    // Match plain format using the defined pattern
    // We need to avoid matching nodespace:// URIs that are already inside markdown links
    static PLAIN_REGEX: OnceLock<Regex> = OnceLock::new();
    let plain_regex = PLAIN_REGEX.get_or_init(|| Regex::new(PLAIN_MENTION_PATTERN).unwrap());

    // Collect all positions where markdown links occur to exclude them
    let mut markdown_ranges = Vec::new();
    for mat in markdown_regex.find_iter(content) {
        markdown_ranges.push((mat.start(), mat.end()));
    }

    // Find plain format matches that don't overlap with markdown matches
    for cap in plain_regex.captures_iter(content) {
        if let Some(node_id) = cap.get(1) {
            let node_id_str = node_id.as_str();

            // Check if this match is inside a markdown link
            let match_pos = cap.get(0).unwrap().start();
            let is_in_markdown = markdown_ranges
                .iter()
                .any(|(start, end)| match_pos >= *start && match_pos < *end);

            if !is_in_markdown && is_valid_node_id(node_id_str) {
                mentions.insert(node_id_str.to_string());
            }
        }
    }

    // Match `[[node-id]]` wikilink references. These never overlap the markdown or
    // plain forms (neither contains `[[`), so no range-exclusion is needed — the
    // captured id is validated the same way, so only real ids become mentions.
    static WIKILINK_REGEX: OnceLock<Regex> = OnceLock::new();
    let wikilink_regex =
        WIKILINK_REGEX.get_or_init(|| Regex::new(WIKILINK_MENTION_PATTERN).unwrap());

    for cap in wikilink_regex.captures_iter(content) {
        if let Some(node_id) = cap.get(1) {
            let node_id_str = node_id.as_str();
            if is_valid_node_id(node_id_str) {
                mentions.insert(node_id_str.to_string());
            }
        }
    }

    mentions.into_iter().collect()
}

/// Core service for node CRUD and hierarchy operations
///
/// # Examples
///
/// ```no_run
/// use nodespace_core::services::NodeService;
/// use nodespace_core::db::SqliteStore;
/// use nodespace_core::models::Node;
/// use std::path::PathBuf;
/// use std::sync::Arc;
/// use serde_json::json;
///
/// #[tokio::main]
/// async fn main() -> Result<(), Box<dyn std::error::Error>> {
///     let mut db = Arc::new(SqliteStore::new(PathBuf::from("./data/test.db")).await?);
///     let service = NodeService::new(&mut db).await?;
///
///     let node = Node::new(
///         "text".to_string(),
///         "Hello World".to_string(),
///         json!({}),
///     );
///
///     let id = service.create_node(node).await?;
///     println!("Created node: {}", id);
///     Ok(())
/// }
/// ```
pub struct NodeService {
    /// SQLite store for all persistence operations
    pub(crate) store: Arc<SqliteStore>,

    /// Behavior registry for validation
    pub(crate) behaviors: Arc<NodeBehaviorRegistry>,

    /// Migration registry for lazy schema upgrades
    pub(crate) migration_registry: Arc<MigrationRegistry>,

    /// Broadcast channel for domain events (128 subscriber capacity)
    /// Changed from DomainEvent to EventEnvelope
    pub(crate) event_tx: broadcast::Sender<crate::db::events::EventEnvelope>,

    /// Origin-filtered mirror of `event_tx`.
    ///
    /// Carries the same domain events as `event_tx` except those whose
    /// `source_client_id` matches `push_excluded_origin`. A consumer that must
    /// not be flooded by a particular origin (e.g. a bulk re-apply that tags
    /// every event with a single client id) subscribes here via
    /// `subscribe_for_push()` so that origin's burst never occupies its buffer.
    /// All other subscribers keep using `event_tx` and see every event.
    pub(crate) push_event_tx: broadcast::Sender<crate::db::events::EventEnvelope>,

    /// Source client id excluded from `push_event_tx`.
    ///
    /// Injected by the host layer via `set_push_excluded_origin`; core assumes
    /// no particular value. While `None`, `push_event_tx` mirrors `event_tx`
    /// exactly. Held behind `RwLock` so the id can be set after the store
    /// notifier closure (which reads it on every event) has been constructed.
    pub(crate) push_excluded_origin: Arc<RwLock<Option<String>>>,

    /// Shared batch state for coalescing events during bulk operations.
    ///
    /// When `BatchState::Batching`, the store notifier accumulates events instead
    /// of broadcasting immediately. `begin_batch_emit()` activates batching and
    /// returns a `BatchEmitGuard` that flushes on drop.
    pub(crate) batch_state: Arc<Mutex<BatchState>>,

    /// Optional client identifier for event source tracking
    ///
    /// When set, all emitted events will include this client_id as source_client_id
    /// in the EventEnvelope metadata.
    ///
    /// Use `with_client()` to create a new NodeService instance with client_id set.
    pub(crate) client_id: Option<String>,

    /// Optional playbook execution context for cycle detection
    ///
    /// When set, emitted events carry this context in EventEnvelope metadata.
    /// Use `scoped_for_playbook()` to create a scoped instance.
    pub(crate) execution_context: Option<crate::db::events::PlaybookExecutionContext>,

    /// Optional waker to trigger embedding processor
    ///
    /// When set, `queue_root_for_embedding()` will wake the processor after
    /// creating stale markers. This enables event-driven embedding processing
    /// without polling.
    ///
    /// Shared via `Arc` so it can be populated after `NodeService` is wrapped
    /// in `Arc` (e.g. when the embedding model loads in the background).
    #[cfg(feature = "nlp")]
    pub(crate) embedding_waker:
        std::sync::Arc<std::sync::OnceLock<crate::services::EmbeddingWaker>>,

    /// Pre-delete subtree access gate (ADR-041). Defaults to [`access_gate::AlwaysAllowGate`]
    /// so community installs keep today's unconditional-cascade behavior. A Pro daemon
    /// (`nodespaced-pro`) injects a real gate via `set_subtree_access_gate` after construction —
    /// held behind `OnceLock` so it can be set once the Pro tenant connection is established.
    pub(crate) subtree_access_gate:
        Arc<std::sync::OnceLock<Arc<dyn access_gate::SubtreeAccessGate>>>,
}

impl Clone for NodeService {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            behaviors: self.behaviors.clone(),
            migration_registry: self.migration_registry.clone(),
            event_tx: self.event_tx.clone(),
            push_event_tx: self.push_event_tx.clone(),
            push_excluded_origin: self.push_excluded_origin.clone(),
            batch_state: self.batch_state.clone(),
            client_id: self.client_id.clone(),
            execution_context: self.execution_context.clone(),
            // Share the same OnceLock so any clone can observe the waker once set.
            #[cfg(feature = "nlp")]
            embedding_waker: self.embedding_waker.clone(),
            subtree_access_gate: self.subtree_access_gate.clone(),
        }
    }
}

impl NodeService {
    /// Create a new NodeService
    ///
    /// Initializes the service with SqliteStore and creates a default
    /// NodeBehaviorRegistry with Text, Task, and Date behaviors.
    ///
    /// # Arguments
    ///
    /// * `store` - Mutable reference to Arc<SqliteStore> (allows cache updates during seeding)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let mut store = Arc::new(SqliteStore::new("./data/nodespace.db".into()).await?);
    /// let service = NodeService::new(&mut store).await?;
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// # Cache Population
    ///
    /// Takes `&mut Arc<SqliteStore>` to enable cache updates during schema seeding:
    /// - On first launch: Seeds schemas and updates caches incrementally via `Arc::get_mut()`
    /// - On subsequent launches: Caches already populated by `SqliteStore::new()`
    pub async fn new(store: &mut Arc<SqliteStore>) -> Result<Self, NodeServiceError> {
        // Create empty migration registry (no migrations registered yet - pre-deployment)
        // Infrastructure exists for future schema evolution post-deployment
        let migration_registry = MigrationRegistry::new();

        // Initialize broadcast channel for domain events (EventEnvelope)
        let (event_tx, _) = broadcast::channel(DOMAIN_EVENT_CHANNEL_CAPACITY);

        // Origin-filtered mirror of event_tx (same capacity). Mirrors every event
        // except those tagged with push_excluded_origin (unset until injected by
        // the host layer, so it mirrors event_tx exactly by default).
        let (push_event_tx, _) = broadcast::channel(DOMAIN_EVENT_CHANNEL_CAPACITY);
        let push_excluded_origin: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(None));

        // Shared batch state — Immediate by default; swapped to Batching during bulk ops.
        let batch_state: Arc<Mutex<BatchState>> = Arc::new(Mutex::new(BatchState::Immediate));

        // Register store-level notifier for automatic domain event emission
        // This callback converts StoreChange notifications to EventEnvelopes.
        // Must be set BEFORE seed_core_schemas so schema seeding also emits events.
        //
        // Events now send only node_id (not full payload) for efficiency.
        // Events wrapped in EventEnvelope with metadata.
        // Batch mode coalesces events per node during bulk operations.
        {
            let tx = event_tx.clone();
            let push_tx = push_event_tx.clone();
            let push_excluded_origin_ref = Arc::clone(&push_excluded_origin);
            let batch_state_ref = Arc::clone(&batch_state);
            let notifier = Arc::new(move |change: StoreChange| {
                use crate::db::events::{EventEnvelope, EventMetadata};

                // Compute changed properties for updates
                let changed_properties = if change.operation == StoreOperation::Updated {
                    if let Some(ref prev) = change.previous_node {
                        compute_property_changes(&prev.properties, &change.node.properties)
                    } else {
                        vec![]
                    }
                } else {
                    vec![]
                };

                // Map store operation to domain event (ID-only, no payload conversion)
                let event = match change.operation {
                    StoreOperation::Created => DomainEvent::NodeCreated {
                        node_id: change.node.id.clone(),
                        node_type: change.node.node_type.clone(),
                    },
                    StoreOperation::Updated => DomainEvent::NodeUpdated {
                        node_id: change.node.id.clone(),
                        node_type: change.node.node_type.clone(),
                        node: change.node.clone(),
                        changed_properties,
                    },
                    StoreOperation::Deleted => DomainEvent::NodeDeleted {
                        id: change.node.id.clone(),
                        node_type: change.node.node_type.clone(),
                    },
                };

                // Wrap in EventEnvelope with metadata
                let envelope = EventEnvelope {
                    event,
                    metadata: EventMetadata {
                        source_client_id: change.source,
                        playbook_context: change.playbook_context,
                    },
                };

                // In batch mode, accumulate last-write-wins per node.
                // In immediate mode (default), broadcast directly.
                let mut state = batch_state_ref.lock().unwrap_or_else(|e| e.into_inner());
                match &mut *state {
                    BatchState::Immediate => {
                        // Mirror to the push channel unless this envelope's origin
                        // is excluded. Clone only when forwarding.
                        if push_forward_allowed(&push_excluded_origin_ref, &envelope) {
                            let _ = push_tx.send(envelope.clone());
                        }
                        let _ = tx.send(envelope);
                    }
                    BatchState::Batching(buf) => {
                        // Batched events flush (and mirror) in BatchEmitGuard::drop.
                        buf.insert(change.node.id.clone(), envelope);
                    }
                }
            });

            // Get mutable reference to store to set notifier
            let store_mut = Arc::get_mut(store).ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "Cannot set notifier: SqliteStore Arc has multiple references".to_string(),
                )
            })?;
            store_mut.set_notifier(notifier);
        }

        // Seed core schemas if needed
        // This must happen BEFORE we clone the Arc into Self, so we can use Arc::get_mut()
        // to update schema caches incrementally during seeding.
        Self::seed_core_schemas_if_needed(store).await?;

        let service = Self {
            store: Arc::clone(store),
            behaviors: Arc::new(NodeBehaviorRegistry::new()),
            migration_registry: Arc::new(migration_registry),
            event_tx,
            push_event_tx,
            push_excluded_origin,
            batch_state,
            client_id: None,
            execution_context: None,
            #[cfg(feature = "nlp")]
            embedding_waker: std::sync::Arc::new(std::sync::OnceLock::new()),
            subtree_access_gate: Arc::new(std::sync::OnceLock::new()),
        };

        // Backfill description subtrees for schemas that still have
        // properties.description but no child nodes (databases created before this change).
        service.backfill_schema_description_subtrees().await?;

        // ADR-037: every install has exactly one local PersonNode (the user).
        service.seed_local_person_if_needed().await?;

        // ADR-037: seed the DatabaseSettingsNode singleton and its owner has_role
        // edge. Must run AFTER the local person seed — the owner edge attaches to it.
        service.seed_database_settings_if_needed().await?;

        Ok(service)
    }

    /// ADR-037: seed exactly one local PersonNode — the local user.
    /// Idempotent: skips when a person already exists, so an existing database
    /// is backfilled on next open too. Name/email stay absent until the user
    /// fills them in (PersonNodeBehavior allows it). On Pro upgrade this node
    /// is bound to a Supabase identity via a single
    /// `auth_identities` row — not recreated.
    ///
    /// Note: `auth_status` lives on DatabaseSettingsNode, not here.
    async fn seed_local_person_if_needed(&self) -> Result<(), NodeServiceError> {
        if !self.query_nodes_by_type("person", None).await?.is_empty() {
            return Ok(());
        }
        let person = Node::new("person".to_string(), String::new(), serde_json::json!({}));
        let id = self.create_node(person).await?;
        tracing::info!(node_id = %id, "🌱 Seeded local PersonNode (ADR-037)");
        Ok(())
    }

    /// ADR-037: seed the DatabaseSettingsNode singleton — the container for
    /// database-level configuration (sync state; tenant roles via `has_role`
    /// edges). Idempotent: skips when a database-settings node already exists, so
    /// an existing database is backfilled on next open too. Seeds `sync_enabled:
    /// false`, `auth_status: local`, and one `has_role` owner edge from the local
    /// PersonNode to this node (role `owner`, status `active`). Must run after the
    /// local person seed so the owner edge always has a person to attach to.
    async fn seed_database_settings_if_needed(&self) -> Result<(), NodeServiceError> {
        if !self
            .query_nodes_by_type("database-settings", None)
            .await?
            .is_empty()
        {
            return Ok(());
        }

        let settings = Node::new_with_id(
            DATABASE_SETTINGS_NODE_ID.to_string(),
            "database-settings".to_string(),
            String::new(),
            serde_json::json!({
                "database-settings": {
                    "sync_enabled": false,
                    "auth_status": "local"
                }
            }),
        );
        let settings_id = self.create_node(settings).await?;

        // Attach the owner role edge from the local PersonNode. Seeding order
        // guarantees exactly one local person exists at this point.
        let local_person_id = self
            .query_nodes_by_type("person", None)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "cannot seed DatabaseSettingsNode owner edge: no local PersonNode".to_string(),
                )
            })?
            .id;
        self.create_relationship(
            &local_person_id,
            "has_role",
            &settings_id,
            serde_json::json!({"role": "owner", "status": "active"}),
        )
        .await?;

        tracing::info!(
            node_id = %settings_id,
            owner = %local_person_id,
            "🌱 Seeded DatabaseSettingsNode singleton with owner has_role edge (ADR-037)"
        );
        Ok(())
    }

    /// Read the cloud tenant (schema + collection) this database is bound to, from
    /// the DatabaseSettingsNode singleton (ADR-053 per-database cloud sync). Returns
    /// `Some((schema, collection))` only when both are present and non-empty; `None`
    /// when the database is unbound — a fresh install, or one whose tenant has never
    /// been set. Callers treat `None` as "no cloud sync target yet".
    pub async fn get_bound_tenant(&self) -> Result<Option<(String, String)>, NodeServiceError> {
        let Some(node) = self
            .query_nodes_by_type("database-settings", None)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(None);
        };
        let settings = node.properties.get("database-settings");
        let schema = settings
            .and_then(|s| s.get("bound_tenant_schema"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        let collection = settings
            .and_then(|s| s.get("bound_tenant_collection"))
            .and_then(|v| v.as_str())
            .unwrap_or_default();
        if schema.is_empty() || collection.is_empty() {
            return Ok(None);
        }
        Ok(Some((schema.to_string(), collection.to_string())))
    }

    /// Bind this database to a cloud tenant (schema + collection) by writing the
    /// authoritative fields on the DatabaseSettingsNode singleton (ADR-053). Merges
    /// into the existing `database-settings` namespace so sibling fields (sync state,
    /// auth status) are preserved. Once set, `get_bound_tenant` returns these values
    /// until re-bound. The singleton is seeded on database open, so a missing one is
    /// an error rather than a silent no-op.
    pub async fn set_bound_tenant(
        &self,
        schema: &str,
        collection: &str,
    ) -> Result<(), NodeServiceError> {
        let node = self
            .query_nodes_by_type("database-settings", None)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "cannot bind tenant: DatabaseSettingsNode singleton not found".to_string(),
                )
            })?;

        let mut properties = node.properties.clone();
        let root = properties.as_object_mut().ok_or_else(|| {
            NodeServiceError::InitializationError(
                "DatabaseSettingsNode properties are not a JSON object".to_string(),
            )
        })?;
        let settings = root
            .entry("database-settings")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "DatabaseSettingsNode `database-settings` is not a JSON object".to_string(),
                )
            })?;
        settings.insert(
            "bound_tenant_schema".to_string(),
            serde_json::Value::String(schema.to_string()),
        );
        settings.insert(
            "bound_tenant_collection".to_string(),
            serde_json::Value::String(collection.to_string()),
        );

        self.update_node(
            &node.id,
            node.version,
            NodeUpdate::new().with_properties(properties),
        )
        .await?;
        Ok(())
    }

    /// Merge fields into the DatabaseSettingsNode singleton's `database-settings`
    /// namespace, preserving every sibling field (tenant binding, sync state,
    /// auth status). The singleton is seeded on database open, so a missing one
    /// is an error rather than a silent no-op.
    async fn merge_database_settings(
        &self,
        fields: &[(&str, serde_json::Value)],
    ) -> Result<(), NodeServiceError> {
        let node = self
            .query_nodes_by_type("database-settings", None)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "cannot update database settings: DatabaseSettingsNode singleton not found"
                        .to_string(),
                )
            })?;

        let mut properties = node.properties.clone();
        let root = properties.as_object_mut().ok_or_else(|| {
            NodeServiceError::InitializationError(
                "DatabaseSettingsNode properties are not a JSON object".to_string(),
            )
        })?;
        let settings = root
            .entry("database-settings")
            .or_insert_with(|| serde_json::json!({}))
            .as_object_mut()
            .ok_or_else(|| {
                NodeServiceError::InitializationError(
                    "DatabaseSettingsNode `database-settings` is not a JSON object".to_string(),
                )
            })?;
        for (key, value) in fields {
            settings.insert((*key).to_string(), value.clone());
        }

        self.update_node(
            &node.id,
            node.version,
            NodeUpdate::new().with_properties(properties),
        )
        .await?;
        Ok(())
    }

    /// Enable (or disable) per-database cloud sync by writing `sync_enabled` on
    /// the DatabaseSettingsNode singleton (ADR-053). This is the field the
    /// registry-driven Pro UI gates its collaboration surfaces on; nothing else
    /// advances it, so this is the authoritative writer the first-Pro consent
    /// flow calls once the user opts in.
    pub async fn set_sync_enabled(&self, enabled: bool) -> Result<(), NodeServiceError> {
        self.merge_database_settings(&[("sync_enabled", serde_json::Value::Bool(enabled))])
            .await
    }

    /// Set the per-database cloud auth status (`local` or `connected`) on the
    /// DatabaseSettingsNode singleton (ADR-053). The Pro daemon advances this to
    /// `connected` once identity is bound and back to `local` on sign-out, which
    /// drives the Pro UI from the sign-in variant to the connected variant.
    pub async fn set_auth_status(&self, status: &str) -> Result<(), NodeServiceError> {
        self.merge_database_settings(&[(
            "auth_status",
            serde_json::Value::String(status.to_string()),
        )])
        .await
    }

    /// Read whether per-database cloud sync is enabled, from the DatabaseSettingsNode
    /// singleton (ADR-053). Returns `false` when the field is absent or the singleton
    /// has not been seeded yet — a fresh install defaults to sync disabled until the
    /// first-Pro consent flow opts in. This is the authoritative gate the Pro daemon
    /// reads before pushing local changes to the cloud, so an un-opted-in database
    /// never leaves the device.
    pub async fn get_sync_enabled(&self) -> Result<bool, NodeServiceError> {
        let Some(node) = self
            .query_nodes_by_type("database-settings", None)
            .await?
            .into_iter()
            .next()
        else {
            return Ok(false);
        };
        Ok(node
            .properties
            .get("database-settings")
            .and_then(|s| s.get("sync_enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false))
    }

    /// Read the per-database cloud auth status (`local` or `connected`) from the
    /// DatabaseSettingsNode singleton (ADR-053). Returns `local` when the field is
    /// absent or the singleton has not been seeded yet — the pre-sign-in default the
    /// Pro UI's sign-in variant expects. Complements `set_auth_status`, which the Pro
    /// daemon uses to advance this to `connected` on bind and back to `local` on
    /// sign-out.
    pub async fn get_auth_status(&self) -> Result<String, NodeServiceError> {
        let Some(node) = self
            .query_nodes_by_type("database-settings", None)
            .await?
            .into_iter()
            .next()
        else {
            return Ok("local".to_string());
        };
        Ok(node
            .properties
            .get("database-settings")
            .and_then(|s| s.get("auth_status"))
            .and_then(|v| v.as_str())
            .unwrap_or("local")
            .to_string())
    }

    /// Seed core schema definitions, per-schema, on every start.
    ///
    /// Schema node IDs are the schema's own `id` (e.g. `"task"`,
    /// `"agent-guidance"`) per [`crate::models::SchemaNode::into_node`], so
    /// existence is checked per schema rather than gating the whole batch on
    /// one representative type. This mirrors the per-node reconciliation
    /// `seed_nodes_from_templates` uses for seeded content — a type-level
    /// skip (checking only whether `task` exists) means a schema added after
    /// a database's first run would never reach that database, exactly the
    /// gap fixed there for content nodes. Existing core schemas are left
    /// untouched; only schemas from [`crate::models::core_schemas::get_core_schemas`]
    /// missing from the database are created.
    ///
    /// This is idempotent - safe to call multiple times.
    async fn seed_core_schemas_if_needed(
        store: &mut Arc<SqliteStore>,
    ) -> Result<(), NodeServiceError> {
        use crate::models::core_schemas::get_core_schemas;

        let core_schemas = get_core_schemas();

        let mut missing_schemas = Vec::new();
        for schema in &core_schemas {
            let exists = store
                .get_node(&schema.id)
                .await
                .map_err(|e| {
                    NodeServiceError::QueryFailed(format!(
                        "Failed to check for schema '{}': {}",
                        schema.id, e
                    ))
                })?
                .is_some();
            if !exists {
                missing_schemas.push(schema.clone());
            }
        }

        if missing_schemas.is_empty() {
            tracing::info!("✅ Core schemas already seeded");
            return Ok(());
        }

        tracing::info!(
            "🌱 Seeding {} missing core schema(s)...",
            missing_schemas.len()
        );

        // Collect schema info for cache updates (before we start creating nodes)
        let schema_cache_updates: Vec<(String, bool)> = missing_schemas
            .iter()
            .map(|s| (s.id.clone(), !s.fields.is_empty()))
            .collect();

        // Universal Graph Architecture: Properties stored in node.properties.
        // Schema nodes go through the normal create path.
        {
            for schema in &missing_schemas {
                let schema_id = schema.id.clone();
                let node = schema.clone().into_node();

                store.create_node(node, None, None).await.map_err(|e| {
                    NodeServiceError::SerializationError(format!(
                        "Failed to create schema node '{}': {}",
                        schema_id, e
                    ))
                })?;
            }
        } // ← Arc clone dropped here, enabling Arc::get_mut() below

        // Update schema caches incrementally
        // We use Arc::get_mut() since we're the only owner at this point (before cloning into Self)
        let store_mut = Arc::get_mut(store).ok_or_else(|| {
            NodeServiceError::InitializationError(
                "Cannot update schema cache: store has multiple Arc references. \
                 Ensure NodeService::new() is called before cloning the store."
                    .to_string(),
            )
        })?;

        for (type_name, _has_fields) in schema_cache_updates {
            store_mut.add_to_schema_cache(type_name);
        }

        tracing::info!("✅ Core schemas seeded successfully (caches updated)");

        Ok(())
    }

    /// Seed node hierarchies from pre-expanded template node lists.
    ///
    /// Each element of `template_groups` is a flat `Vec<PreparedNode>` produced
    /// by [`crate::markdown::prepare_nodes_from_template`], which stamps the
    /// root node's properties with a `_seed` object containing `key` (the
    /// template's stable title), `version` (a content hash), and `tier`
    /// (`"system"` or `"starter"`). Nested under one object key (rather than
    /// flat top-level keys) so [`Self::normalize_flat_properties_to_namespace`]
    /// preserves it as a dormant namespace instead of hoisting its contents
    /// into `properties[node_type]` on write — the same pattern the `tool`
    /// seed template comment (`skill_pipeline.rs`) uses for the same reason.
    ///
    /// Reconciliation is per node, keyed by `_seed.key` within each `node_type`:
    ///
    /// | state                                          | action              |
    /// |-------------------------------------------------|---------------------|
    /// | absent                                           | create              |
    /// | present, hash matches                            | skip (up to date)   |
    /// | present, `system`, hash differs                  | replace             |
    /// | present, `starter`, not user-modified, hash differs | replace          |
    /// | present, `starter`, user-modified                | skip, log once      |
    ///
    /// A "replace" deletes the existing subtree and recreates it fresh — the
    /// same insert path used for a first-time create — rather than diffing
    /// individual children, since template content (including which children
    /// exist) can change between versions.
    pub async fn seed_nodes_from_templates(
        &self,
        template_groups: Vec<Vec<crate::markdown::PreparedNode>>,
    ) -> Result<(), NodeServiceError> {
        if template_groups.is_empty() {
            return Ok(());
        }

        // One query per distinct node_type covers every existing seeded node of
        // that type in a single round trip — bounded by the number of seeded
        // types (currently 3: prompt, skill, tool), not the number of nodes.
        let root_types: std::collections::HashSet<String> = template_groups
            .iter()
            .filter_map(|g| g.first())
            .map(|n| n.node_type.clone())
            .collect();

        let mut existing_by_key: HashMap<String, HashMap<String, Node>> = HashMap::new();
        for node_type in &root_types {
            let filter = crate::models::NodeFilter {
                node_type: Some(node_type.clone()),
                ..Default::default()
            };
            let mut by_key = HashMap::new();
            for node in self.query_nodes(filter).await? {
                if let Some(key) = node
                    .properties
                    .get("_seed")
                    .and_then(|s| s.get("key"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                {
                    by_key.insert(key, node);
                }
            }
            existing_by_key.insert(node_type.clone(), by_key);
        }

        let mut created_roots = 0u32;
        let mut created_children = 0u32;
        let mut replaced = 0u32;
        let mut skipped_current = 0u32;
        let mut skipped_user_modified = 0u32;

        for group in template_groups {
            let root = match group.first() {
                Some(r) => r,
                None => continue,
            };
            let seed_meta = root.properties.get("_seed");
            let seed_key = seed_meta
                .and_then(|s| s.get("key"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let seed_version = seed_meta
                .and_then(|s| s.get("version"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            let existing = existing_by_key
                .get(&root.node_type)
                .and_then(|by_key| by_key.get(seed_key));

            if let Some(existing_node) = existing {
                let existing_seed = existing_node.properties.get("_seed");
                let existing_version = existing_seed
                    .and_then(|s| s.get("version"))
                    .and_then(|v| v.as_str())
                    .unwrap_or_default();

                if existing_version == seed_version {
                    skipped_current += 1;
                    continue;
                }

                let user_modified = existing_seed
                    .and_then(|s| s.get("user_modified"))
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if user_modified {
                    tracing::info!(
                        seed_key,
                        node_type = %root.node_type,
                        "Seed content changed but node was user-modified; skipping"
                    );
                    skipped_user_modified += 1;
                    continue;
                }

                // Replace: delete the existing subtree, then fall through to the
                // same create path used for a brand-new node. The recreated root
                // gets `root.id` — a fresh UUID assigned by
                // `prepare_nodes_from_template` on every call — not the deleted
                // node's ID. Nothing references seeded prompt/skill/tool nodes by
                // stable ID today, so this is safe, but any future feature that
                // does (e.g. a `mentions` edge into a skill node) would need
                // either a stable ID carried across replace, or an explicit
                // decision that seeded-content IDs are not a stable reference
                // surface.
                self.delete_node(&existing_node.id, existing_node.version)
                    .await?;
                replaced += 1;
            }

            // Insert root node (no parent).
            self.create_node_with_parent(CreateNodeParams {
                id: Some(root.id.clone()),
                node_type: root.node_type.clone(),
                content: root.content.clone(),
                properties: root.properties.clone(),
                parent_id: None,
                position: crate::services::InsertPositionOwned::End,
            })
            .await?;
            created_roots += 1;

            // Insert children via bulk_create_hierarchy (single transaction).
            let children = &group[1..];
            if !children.is_empty() {
                let bulk_nodes: Vec<(
                    String,
                    String,
                    String,
                    Option<String>,
                    f64,
                    serde_json::Value,
                )> = children
                    .iter()
                    .map(|n| {
                        (
                            n.id.clone(),
                            n.node_type.clone(),
                            n.content.clone(),
                            n.parent_id.clone(),
                            n.order,
                            n.properties.clone(),
                        )
                    })
                    .collect();
                self.bulk_create_hierarchy(bulk_nodes).await?;
                created_children += children.len() as u32;
            }
        }

        if created_roots > 0 || replaced > 0 {
            tracing::info!(
                created_roots,
                created_children,
                replaced,
                skipped_current,
                skipped_user_modified,
                "Agent nodes reconciled from templates"
            );
        }

        Ok(())
    }

    /// Backfill description child subtrees for schemas that still have `properties.description`.
    ///
    /// Runs at every startup; idempotent because `properties.description` is removed from the
    /// schema node after a successful backfill. Schemas without that key are skipped in O(1).
    async fn backfill_schema_description_subtrees(&self) -> Result<(), NodeServiceError> {
        use crate::markdown::prepare_nodes_from_markdown;

        let schemas = self.get_all_schemas().await.map_err(|e| {
            NodeServiceError::QueryFailed(format!("Failed to fetch schemas for migration: {}", e))
        })?;

        let mut backfilled = 0usize;

        for schema in schemas {
            // Read the legacy description from properties — its presence is the migration trigger.
            // Using child count as the check would be incorrect: schemas can now legitimately
            // have non-description children, and would permanently skip backfill if any child
            // exists before the description is migrated.
            let description = self
                .store
                .get_node(&schema.id)
                .await
                .unwrap_or(None)
                .and_then(|n| {
                    n.properties
                        .get("description")
                        .and_then(|v| v.as_str())
                        .filter(|s| !s.is_empty())
                        .map(|s| s.to_string())
                });

            let description = match description {
                Some(d) => d,
                None => continue, // No legacy description to migrate (already migrated or never had one)
            };

            let prepared = match prepare_nodes_from_markdown(&description, Some(schema.id.clone()))
            {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(
                        schema_id = %schema.id,
                        error = %e,
                        "Failed to prepare description subtree during migration, skipping"
                    );
                    continue;
                }
            };

            if prepared.is_empty() {
                continue;
            }

            let bulk_nodes: Vec<(
                String,
                String,
                String,
                Option<String>,
                f64,
                serde_json::Value,
            )> = prepared
                .into_iter()
                .map(|n| {
                    (
                        n.id,
                        n.node_type,
                        n.content,
                        n.parent_id,
                        n.order,
                        n.properties,
                    )
                })
                .collect();

            if let Err(e) = self.bulk_create_hierarchy(bulk_nodes).await {
                tracing::warn!(
                    schema_id = %schema.id,
                    error = %e,
                    "Failed to backfill description subtree, skipping"
                );
                continue;
            }

            // Clear the legacy properties.description so this schema is not re-migrated
            // on the next startup (the subtree's existence is now the canonical description).
            if let Err(e) = self
                .store
                .remove_property_key(&schema.id, "description")
                .await
            {
                tracing::warn!(
                    schema_id = %schema.id,
                    error = %e,
                    "Failed to clear legacy description property after backfill"
                );
            }

            backfilled += 1;
        }

        if backfilled > 0 {
            tracing::info!(
                count = backfilled,
                "Backfilled schema description subtrees (Issue #1351)"
            );
        }

        Ok(())
    }

    /// Get access to the underlying SqliteStore
    ///
    /// Useful for advanced operations that need direct database access
    pub fn store(&self) -> &Arc<SqliteStore> {
        &self.store
    }

    /// Get a reference to the behavior registry
    pub fn behaviors(&self) -> &Arc<NodeBehaviorRegistry> {
        &self.behaviors
    }

    /// Resolve the behavior for a node type, falling back to CustomNodeBehavior.
    pub(crate) fn behavior_for(&self, node_type: &str) -> Arc<dyn crate::behaviors::NodeBehavior> {
        self.behaviors
            .get(node_type)
            .unwrap_or_else(|| Arc::new(crate::behaviors::CustomNodeBehavior::new(node_type)))
    }

    /// Check if a node type is embeddable according to its behavior
    ///
    /// Uses `NodeBehavior::get_embeddable_content()` on a probe node to determine
    /// if this node type can ever produce embeddable content. Types that unconditionally
    /// return `None` (task, date, collection, etc.) are not embeddable.
    ///
    /// For types that are conditionally embeddable (based on content), this creates
    /// a probe node with non-empty content. If the behavior still returns `None`,
    /// the type is never embeddable.
    fn is_embeddable_type(&self, node_type: &str) -> bool {
        behavior_is_embeddable(&self.behaviors, node_type)
    }

    /// Create a new NodeService with a client identifier
    ///
    /// Returns a clone of this service with the client_id set. All operations
    /// performed through the returned service will emit events with this client_id
    /// as the source_client_id.
    ///
    /// # Arguments
    ///
    /// * `client_id` - Unique identifier for the client (e.g., "tauri-window-1", "mcp-client-123")
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut store = Arc::new(SqliteStore::new("./data/nodespace.db".into()).await?);
    /// let service = NodeService::new(&mut store).await?;
    ///
    /// // Create a scoped service for a specific client
    /// let tauri_service = service.with_client("tauri-window-1");
    ///
    /// // All operations through tauri_service will include "tauri-window-1" in events
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_client(&self, client_id: impl Into<String>) -> Self {
        let mut cloned = self.clone();
        cloned.client_id = Some(client_id.into());
        cloned
    }

    /// Return a scoped `NodeService` that tags all emitted events with the given
    /// playbook execution context.
    ///
    /// Events emitted through the returned instance carry `playbook_context` in
    /// `EventEnvelope.metadata` for cycle detection in the playbook engine.
    ///
    /// **Restricted to `crate::playbook`.** Call sites outside the playbook
    /// module are a misuse — use `with_client` for general event-source tagging.
    pub(crate) fn scoped_for_playbook(
        &self,
        ctx: crate::db::events::PlaybookExecutionContext,
    ) -> Self {
        let mut cloned = self.clone();
        cloned.execution_context = Some(ctx);
        cloned
    }

    /// Subscribe to domain events (returns EventEnvelope)
    ///
    /// Returns a broadcast receiver that receives all domain events wrapped
    /// in `EventEnvelope` with metadata (source_client_id, playbook_context).
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<crate::db::events::EventEnvelope> {
        self.event_tx.subscribe()
    }

    /// Subscribe to the origin-filtered push channel.
    ///
    /// Returns a receiver that observes every domain event except those whose
    /// `source_client_id` matches the origin configured via
    /// `set_push_excluded_origin`. When no origin is configured this behaves
    /// identically to `subscribe_to_events`.
    ///
    /// Intended for a consumer that must not have its buffer flooded by a bulk
    /// re-apply that tags every event with a single origin id (e.g. the sync
    /// push consumer during a cold pull).
    pub fn subscribe_for_push(&self) -> broadcast::Receiver<crate::db::events::EventEnvelope> {
        self.push_event_tx.subscribe()
    }

    /// Configure the source client id excluded from the push channel.
    ///
    /// Events tagged with this `source_client_id` are still delivered to
    /// `subscribe_to_events` subscribers but are withheld from
    /// `subscribe_for_push` subscribers. Core assumes no particular value —
    /// the host layer injects the id of the origin whose bursts must not flood
    /// the push consumer. Shared across all clones of this service.
    pub fn set_push_excluded_origin(&mut self, origin: impl Into<String>) {
        let mut guard = self
            .push_excluded_origin
            .write()
            .unwrap_or_else(|e| e.into_inner());
        *guard = Some(origin.into());
    }

    /// Begin batched event emission for bulk operations.
    ///
    /// While the returned `BatchEmitGuard` is live, domain events from the store
    /// notifier are coalesced per node (last-write-wins) instead of being broadcast
    /// individually. When the guard drops, one event per modified node is flushed to
    /// the broadcast channel.
    ///
    /// Use this around any bulk operation that touches many nodes in a short window
    /// (e.g. embedding updates, batch imports, collection rebuilds). Single writes
    /// and ai-chat turn completions should NOT use this — they rely on immediate
    /// emission.
    ///
    pub fn begin_batch_emit(&self) -> BatchEmitGuard {
        let mut state = self.batch_state.lock().unwrap_or_else(|e| e.into_inner());
        // Nested batch guards are not supported: the outer guard's buffered events
        // would be silently discarded when the inner guard resets the state.
        debug_assert!(
            matches!(*state, BatchState::Immediate),
            "begin_batch_emit called while a batch is already active"
        );
        *state = BatchState::Batching(HashMap::new());
        drop(state);
        BatchEmitGuard {
            state: Arc::clone(&self.batch_state),
            tx: self.event_tx.clone(),
            push_tx: self.push_event_tx.clone(),
            push_excluded_origin: Arc::clone(&self.push_excluded_origin),
        }
    }

    /// Emit a domain event to all subscribers (wraps in EventEnvelope)
    ///
    /// Internal helper for emitting events after successful operations.
    /// Wraps the event in an EventEnvelope with this instance's client_id
    /// and execution_context as metadata.
    ///
    /// Routes through `batch_state`: when a `BatchEmitGuard` is active, the event
    /// is buffered (last-write-wins per node_id) instead of broadcast immediately.
    pub(crate) fn emit_event(&self, event: DomainEvent) {
        use crate::db::events::{EventEnvelope, EventMetadata};
        let envelope = EventEnvelope {
            event,
            metadata: EventMetadata {
                source_client_id: self.client_id.clone(),
                playbook_context: self.execution_context.clone(),
            },
        };
        let node_id = match &envelope.event {
            DomainEvent::NodeCreated { node_id, .. } => Some(node_id.clone()),
            DomainEvent::NodeUpdated { node_id, .. } => Some(node_id.clone()),
            DomainEvent::NodeDeleted { id, .. } => Some(id.clone()),
            // Relationship events are not node-keyed; always broadcast immediately.
            _ => None,
        };
        let mut state = self.batch_state.lock().unwrap_or_else(|e| e.into_inner());
        match (&mut *state, node_id) {
            (BatchState::Batching(buf), Some(id)) => {
                // Batched events flush (and mirror) in BatchEmitGuard::drop.
                buf.insert(id, envelope);
            }
            _ => {
                // Mirror to the push channel unless this envelope's origin is
                // excluded. Clone only when forwarding.
                if push_forward_allowed(&self.push_excluded_origin, &envelope) {
                    let _ = self.push_event_tx.send(envelope.clone());
                }
                let _ = self.event_tx.send(envelope);
            }
        }
    }
}

/// Whether `node_type`'s behavior can ever produce embeddable content (probed
/// with non-empty content). Shared by [`NodeService::is_embeddable_type`] and the
/// static spawned-task embedding path so both agree which types are embeddable
/// vs. non-embeddable containers (e.g. `date` pages).
pub(crate) fn behavior_is_embeddable(
    behaviors: &crate::behaviors::NodeBehaviorRegistry,
    node_type: &str,
) -> bool {
    let behavior: Arc<dyn crate::behaviors::NodeBehavior> = behaviors
        .get(node_type)
        .unwrap_or_else(|| Arc::new(crate::behaviors::CustomNodeBehavior::new(node_type)));
    // Probe with non-empty content to see if the behavior can ever return Some.
    let probe = Node {
        id: "probe".to_string(),
        node_type: node_type.to_string(),
        content: "probe content".to_string(),
        version: 1,
        properties: serde_json::json!({}),
        mentions: vec![],
        mentioned_in: vec![],
        created_at: chrono::Utc::now(),
        modified_at: chrono::Utc::now(),
        title: None,
        lifecycle_status: "active".to_string(),
    };
    behavior.get_embeddable_content(&probe).is_some()
}

/// Result of checking node completeness against its schema's required relationships
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletenessResult {
    /// The node ID that was checked
    pub node_id: String,
    /// Whether all required relationships are satisfied
    pub is_complete: bool,
    /// Names of required relationships that are missing
    pub missing_relationships: Vec<String>,
}

/// Recursively build a tree structure from flat node data
///
/// Converts flat node map and adjacency list into nested JSON tree.
fn build_node_tree_recursive(
    node: &Node,
    node_map: &HashMap<String, Node>,
    adjacency_list: &HashMap<String, Vec<String>>,
) -> serde_json::Value {
    // Raw node serialization (namespaced properties preserved), plus the
    // nodespace:// URI clients use for rich rendering — mirroring the
    // single-node read path's contract.
    let mut json = serde_json::to_value(node.clone())
        .unwrap_or_else(|_| serde_json::Value::Object(Default::default()));
    if let Some(obj) = json.as_object_mut() {
        obj.insert(
            "uri".to_string(),
            serde_json::Value::String(format!("nodespace://{}", node.id)),
        );
    }

    // Build children array (always present, even if empty for consistency)
    let children: Vec<serde_json::Value> = if let Some(children_ids) = adjacency_list.get(&node.id)
    {
        children_ids
            .iter()
            .filter_map(|child_id| {
                node_map.get(child_id).map(|child_node| {
                    build_node_tree_recursive(child_node, node_map, adjacency_list)
                })
            })
            .collect()
    } else {
        Vec::new()
    };

    if let Some(obj) = json.as_object_mut() {
        obj.insert("children".to_string(), serde_json::Value::Array(children));
    }

    json
}

/// NodeAccessor implementation for NodeService
///
/// Delegates to existing NodeService methods, ensuring all business rules
/// (mentions, migrations, etc.) apply when behaviors fetch related nodes.
#[async_trait]
impl NodeAccessor for NodeService {
    async fn get_node(&self, id: &str) -> Result<Option<Node>, NodeServiceError> {
        // Delegate to NodeService's existing get_node (includes mentions, migrations, etc.)
        NodeService::get_node(self, id).await
    }

    async fn get_children(&self, parent_id: &str) -> Result<Vec<Node>, NodeServiceError> {
        // Delegate to NodeService's existing get_children (edge-based, sorted by fractional order)
        NodeService::get_children(self, parent_id).await
    }

    async fn get_nodes(&self, ids: &[&str]) -> Result<Vec<Node>, NodeServiceError> {
        // Delegate to store's batch fetch, converting &str -> String for the store API
        let id_strings: Vec<String> = ids.iter().map(|s| s.to_string()).collect();
        let node_map = self
            .store
            .get_nodes_by_ids(&id_strings)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;
        Ok(node_map.into_values().collect())
    }
}

#[cfg(test)]
mod container_type_parity_tests {
    use super::behavior_is_embeddable;
    use crate::behaviors::{NodeBehaviorRegistry, NON_EMBEDDABLE_CONTAINER_TYPES};
    use std::collections::HashSet;

    /// `NON_EMBEDDABLE_CONTAINER_TYPES` is the hand-maintained SQL twin of the
    /// behavior probe: the BM25 ancestor CTE (`db/sqlite_store/embeddings.rs`) can't
    /// run `behavior_is_embeddable`, so it hardcodes this list to decide which
    /// parents to stop below. If a new built-in type is non-embeddable AND can bear
    /// children but isn't in the const, the embedding-root walk (behavior-driven)
    /// and the search-root walk (list-driven) diverge and that type's nested content
    /// becomes silently unfindable. This test fails the moment they drift.
    #[test]
    fn non_embeddable_container_types_match_behaviors() {
        let registry = NodeBehaviorRegistry::new();
        let actual: HashSet<String> = registry
            .get_all_types()
            .into_iter()
            .filter(|t| {
                !behavior_is_embeddable(&registry, t)
                    && registry
                        .get(t)
                        .map(|b| b.can_have_children())
                        .unwrap_or(false)
            })
            .collect();
        let expected: HashSet<String> = NON_EMBEDDABLE_CONTAINER_TYPES
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(
            actual, expected,
            "NON_EMBEDDABLE_CONTAINER_TYPES (behaviors/mod.rs) drifted from the \
             non-embeddable child-bearing behaviors. Update BOTH the const and the \
             BM25 CTE stop-set, or embedding-root and search-root resolution diverge."
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqliteStore;
    use crate::InsertPosition;
    use serde_json::json;
    use tempfile::TempDir;

    async fn create_test_service() -> (NodeService, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let mut store = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let service = NodeService::new(&mut store).await.unwrap();
        (service, temp_dir)
    }

    /// ADR-059 §6/§7: embedding aggregation never spans an access boundary — the
    /// boundary is made UNREACHABLE BY CONSTRUCTION (the root-only `member_of`
    /// constraint of §2 + the embeddable-type list), NOT filtered at aggregation
    /// time. This pins both load-bearing invariants so a regression (or the
    /// rejected "filter at runtime" alternative) can't silently reopen the leak:
    ///   1. `get_aggregated_content` walks `has_child` only, so content filed into a
    ///      restricted collection via `member_of` never enters an unrelated open
    ///      root's vector.
    ///   2. §2 forbids an embeddable `has_child` descendant from carrying its own
    ///      restriction (a `member_of` edge), so a boundary cannot appear inside an
    ///      embeddable root's aggregate.
    #[tokio::test]
    async fn embedding_aggregation_never_spans_an_access_boundary_adr059() {
        use crate::behaviors::{NodeBehavior, TextNodeBehavior};
        use crate::services::{CreateNodeParams, InsertPositionOwned};

        let (svc, _tmp) = create_test_service().await;

        // A RESTRICTED collection with a member ROOT holding secret content
        // (filed via member_of — the only legal way, per §2).
        svc.create_node_with_parent(CreateNodeParams {
            id: Some("11111111-1111-1111-1111-1111111111c1".into()),
            node_type: "collection".into(),
            content: "Secret Collection".into(),
            parent_id: None,
            position: InsertPositionOwned::End,
            properties: serde_json::json!({ "collection": { "restrictedToMembers": true } }),
        })
        .await
        .unwrap();
        svc.create_node_with_parent(CreateNodeParams {
            id: Some("11111111-1111-1111-1111-1111111111c2".into()),
            node_type: "text".into(),
            content: "SECRET_RESTRICTED_TEXT".into(),
            parent_id: None,
            position: InsertPositionOwned::End,
            properties: serde_json::json!({}),
        })
        .await
        .unwrap();
        svc.store()
            .add_to_collection("11111111-1111-1111-1111-1111111111c2", "11111111-1111-1111-1111-1111111111c1")
            .await
            .expect("a ROOT node may be filed into a restricted collection");

        // A separate OPEN embeddable root with a has_child child.
        svc.create_node_with_parent(CreateNodeParams {
            id: Some("11111111-1111-1111-1111-1111111111c3".into()),
            node_type: "text".into(),
            content: "OPEN_ROOT_TEXT".into(),
            parent_id: None,
            position: InsertPositionOwned::End,
            properties: serde_json::json!({}),
        })
        .await
        .unwrap();
        svc.create_node_with_parent(CreateNodeParams {
            id: Some("11111111-1111-1111-1111-1111111111c4".into()),
            node_type: "text".into(),
            content: "OPEN_CHILD_TEXT".into(),
            parent_id: Some("11111111-1111-1111-1111-1111111111c3".into()),
            position: InsertPositionOwned::End,
            properties: serde_json::json!({}),
        })
        .await
        .unwrap();

        // (1) The open root's aggregate includes its own has_child subtree, and
        // NEVER the restricted collection's member_of content.
        let root = svc.get_node("11111111-1111-1111-1111-1111111111c3").await.unwrap().unwrap();
        let aggregated = TextNodeBehavior
            .get_aggregated_content(&root, &svc)
            .await
            .unwrap_or_default();
        assert!(
            aggregated.contains("OPEN_CHILD_TEXT"),
            "aggregate must include the open has_child subtree"
        );
        assert!(
            !aggregated.contains("SECRET_RESTRICTED_TEXT"),
            "aggregate must NOT include content behind a restricted-collection access boundary (ADR-059 §7)"
        );

        // (2) §2: an embeddable has_child descendant cannot carry its own
        // restriction, so a boundary can never form inside an aggregate.
        svc.create_node_with_parent(CreateNodeParams {
            id: Some("11111111-1111-1111-1111-1111111111c5".into()),
            node_type: "text".into(),
            content: "child2".into(),
            parent_id: Some("11111111-1111-1111-1111-1111111111c3".into()),
            position: InsertPositionOwned::End,
            properties: serde_json::json!({}),
        })
        .await
        .unwrap();
        let err = svc.store().add_to_collection("11111111-1111-1111-1111-1111111111c5", "11111111-1111-1111-1111-1111111111c1").await;
        assert!(
            err.is_err(),
            "ADR-059 §2: a has_child descendant must be rejected from direct collection membership (member_of is root-only)"
        );
    }

    /// A schema added to `get_core_schemas()` after a database's first run
    /// must still reach that database on the next start — the same
    /// per-node-not-per-type reconciliation guarantee `06a94eee` established
    /// for seeded content. Simulates "pre-existing DB predates this schema" by
    /// deleting one schema node post-seed, then constructing a fresh
    /// `NodeService` against the same DB file exactly as the daemon does on
    /// every restart, and confirming only the missing schema is recreated —
    /// an unrelated, already-present schema is left untouched.
    #[tokio::test]
    async fn seed_core_schemas_reconciles_missing_schema_on_next_service_new() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let task_before = {
            let mut store = Arc::new(SqliteStore::new(db_path.clone()).await.unwrap());
            let service = NodeService::new(&mut store).await.unwrap();

            // Sanity: agent-guidance is present after the normal initial seed.
            assert!(
                service.get_node("agent-guidance").await.unwrap().is_some(),
                "agent-guidance schema must exist after initial seed"
            );
            let task_before = service
                .get_node("task")
                .await
                .unwrap()
                .expect("task schema must exist after initial seed");

            // Simulate a pre-existing database that predates the agent-guidance
            // schema: remove it directly at the store level (schema nodes
            // aren't deletable through the validated NodeService::delete_node
            // path).
            service
                .store
                .delete_node("agent-guidance", None)
                .await
                .expect("store-level delete succeeds");
            assert!(
                service.get_node("agent-guidance").await.unwrap().is_none(),
                "agent-guidance must be gone after the simulated pre-existing-DB delete"
            );

            task_before
            // service's Arc<SqliteStore> is dropped here, leaving no other
            // owner — required by seed_core_schemas_if_needed's Arc::get_mut.
        };

        // Reopen against the same DB file, exactly as the daemon does on restart.
        let mut store = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let service = NodeService::new(&mut store)
            .await
            .expect("reconciliation on restart succeeds");

        assert!(
            service.get_node("agent-guidance").await.unwrap().is_some(),
            "agent-guidance must be recreated by reconciliation, not left orphaned"
        );
        let task_after = service.get_node("task").await.unwrap().unwrap();
        assert_eq!(
            task_after.version, task_before.version,
            "an unrelated, already-present schema (task) must not be touched by reconciliation"
        );
    }

    /// Adding existing nodes as children one at a time appends each after the
    /// current last sibling, producing strictly increasing order keys — the
    /// simple single-threaded case for the add-existing-child path.
    #[tokio::test]
    async fn add_existing_child_appends_in_order() {
        let (service, _temp) = create_test_service().await;
        for id in ["p", "a", "b", "c"] {
            service
                .create_node(Node::new_with_id(
                    id.to_string(),
                    "text".to_string(),
                    id.to_string(),
                    json!({}),
                ))
                .await
                .unwrap();
        }

        for child in ["a", "b", "c"] {
            service
                .create_relationship("p", "has_child", child, json!({}))
                .await
                .unwrap();
        }

        // get_children sorts by the has_child edge order ASC, so insertion order
        // is preserved: each append landed after the previous max.
        let kids: Vec<String> = service
            .get_children("p")
            .await
            .unwrap()
            .into_iter()
            .map(|k| k.id)
            .collect();
        assert_eq!(kids, vec!["a", "b", "c"], "appends preserve sibling order");

        let mut orders: Vec<f64> = service
            .store()
            .get_relationship_orders("p", "has_child", "in_node")
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.unwrap_or(0.0))
            .collect();
        orders.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(orders, vec![1.0, 2.0, 3.0], "strictly appended order keys");
    }

    /// Regression for the ops-layer add-existing-child order race: many
    /// `create_relationship("has_child")` appends onto the SAME parent running
    /// at once — alongside a concurrent reorder of an existing child — must leave
    /// every sibling with a DISTINCT fractional-order key. The pre-fix code read
    /// the next child order and wrote the edge as two separate un-serialized
    /// steps, so concurrent appends computed the same key against a stale max and
    /// collided. `append_child_edge` does the read → compute → write atomically
    /// under `reorder_lock`, matching `move_node`'s discipline.
    ///
    /// Requires a multi-thread runtime so the appends' read/compute/write can
    /// actually interleave (mirrors the store-level reorder-race test).
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_add_existing_child_keeps_distinct_order() {
        let (service, _temp) = create_test_service().await;
        let service = Arc::new(service);

        // Parent seeded with two children: a non-trivial starting max order and a
        // reorder target for the concurrent move.
        service
            .create_node(Node::new_with_id(
                "p".to_string(),
                "text".to_string(),
                "p".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        for seed in ["seed0", "seed1"] {
            service
                .create_node(Node::new_with_id(
                    seed.to_string(),
                    "text".to_string(),
                    seed.to_string(),
                    json!({}),
                ))
                .await
                .unwrap();
            service
                .create_relationship("p", "has_child", seed, json!({}))
                .await
                .unwrap();
        }

        // Orphan nodes to attach concurrently.
        const N: usize = 16;
        for i in 0..N {
            service
                .create_node(Node::new_with_id(
                    format!("n{i}"),
                    "text".to_string(),
                    format!("n{i}"),
                    json!({}),
                ))
                .await
                .unwrap();
        }

        let mut handles = Vec::new();
        // N concurrent add-existing-child appends onto the same parent.
        for i in 0..N {
            let service = service.clone();
            handles.push(tokio::spawn(async move {
                service
                    .create_relationship("p", "has_child", &format!("n{i}"), json!({}))
                    .await
            }));
        }
        // A concurrent reorder contending on the same parent's sibling order.
        {
            let service = service.clone();
            handles.push(tokio::spawn(async move {
                service
                    .reorder_child("seed1", crate::services::InsertPosition::Beginning)
                    .await
            }));
        }
        for h in handles {
            h.await
                .expect("task panicked")
                .expect("add-existing-child / reorder returned an error");
        }

        // All 2 seed + N appended children are present (none lost/duplicated).
        let children = service.get_children("p").await.unwrap();
        assert_eq!(children.len(), N + 2, "lost or duplicated a child");

        // Every has_child edge under the parent carries a DISTINCT order key.
        let mut orders: Vec<f64> = service
            .store()
            .get_relationship_orders("p", "has_child", "in_node")
            .await
            .unwrap()
            .into_iter()
            .map(|o| o.unwrap_or(0.0))
            .collect();
        assert_eq!(orders.len(), N + 2, "expected one edge per child");
        orders.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pair in orders.windows(2) {
            assert!(
                (pair[1] - pair[0]).abs() > f64::EPSILON,
                "colliding sibling order keys after concurrent add-existing-child: {orders:?}"
            );
        }
    }

    /// The batched edge-sweep primitive (#345) reproduces the sender's sibling order
    /// and is idempotent — it never re-parents a child that already has a parent.
    #[tokio::test]
    async fn bulk_create_has_child_edges_reproduces_order_and_is_idempotent() {
        let (service, _temp) = create_test_service().await;
        for id in ["p", "a", "b", "c"] {
            service
                .create_node(Node::new_with_id(
                    id.to_string(),
                    "text".to_string(),
                    id.to_string(),
                    json!({}),
                ))
                .await
                .unwrap();
        }
        // Attach out of insertion order, with sibling orders b=1, a=2, c=3.
        let n = service
            .bulk_create_has_child_edges(&[
                ("p".to_string(), "a".to_string(), 2.0),
                ("p".to_string(), "b".to_string(), 1.0),
                ("p".to_string(), "c".to_string(), 3.0),
            ])
            .await
            .unwrap();
        assert_eq!(n, 3);
        let kids: Vec<String> = service
            .get_children("p")
            .await
            .unwrap()
            .into_iter()
            .map(|k| k.id)
            .collect();
        assert_eq!(
            kids,
            vec!["b", "a", "c"],
            "children sorted by the given sibling order"
        );

        // Idempotent: a re-run skips already-parented children (no dup, no re-parent).
        let n2 = service
            .bulk_create_has_child_edges(&[
                ("p".to_string(), "a".to_string(), 9.0),
                ("p".to_string(), "b".to_string(), 9.0),
            ])
            .await
            .unwrap();
        assert_eq!(n2, 0, "already-parented children are skipped");
        let kids2: Vec<String> = service
            .get_children("p")
            .await
            .unwrap()
            .into_iter()
            .map(|k| k.id)
            .collect();
        assert_eq!(
            kids2,
            vec!["b", "a", "c"],
            "order unchanged on idempotent re-run"
        );
    }

    /// Within a SINGLE batch, a child listed under two parents attaches to the FIRST
    /// only — the store's read-your-writes skip-check prevents a second parent edge
    /// (a node has at most one parent).
    #[tokio::test]
    async fn bulk_create_has_child_edges_no_second_parent_within_one_batch() {
        let (service, _temp) = create_test_service().await;
        for id in ["p", "q", "x"] {
            service
                .create_node(Node::new_with_id(
                    id.to_string(),
                    "text".to_string(),
                    id.to_string(),
                    json!({}),
                ))
                .await
                .unwrap();
        }
        // Same child x under p then q in one batch.
        let n = service
            .bulk_create_has_child_edges(&[
                ("p".to_string(), "x".to_string(), 1.0),
                ("q".to_string(), "x".to_string(), 2.0),
            ])
            .await
            .unwrap();
        assert_eq!(n, 1, "only the first parent edge is created");
        let p_kids: Vec<String> = service
            .get_children("p")
            .await
            .unwrap()
            .into_iter()
            .map(|k| k.id)
            .collect();
        assert_eq!(p_kids, vec!["x"], "x attaches to the first parent p");
        assert!(
            service.get_children("q").await.unwrap().is_empty(),
            "x is NOT also a child of q"
        );
    }

    #[tokio::test]
    async fn test_create_text_node() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Hello World".to_string(), json!({}));

        let id = service.create_node(node.clone()).await.unwrap();
        assert_eq!(id, node.id);

        let retrieved = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "Hello World");
        assert_eq!(retrieved.node_type, "text");
    }

    #[tokio::test]
    async fn test_create_task_node() {
        let (service, _temp) = create_test_service().await;

        // Client sends flat properties, backend normalizes to namespaced storage
        let node = Node::new(
            "task".to_string(),
            "Implement NodeService".to_string(),
            json!({"status": "in_progress", "priority": "high"}),
        );

        let id = service.create_node(node).await.unwrap();
        let retrieved = service.get_node(&id).await.unwrap().unwrap();

        assert_eq!(retrieved.node_type, "task");
        // Internal API returns namespaced properties (client-facing API flattens)
        assert_eq!(retrieved.properties["task"]["status"], "in_progress");
        assert_eq!(retrieved.properties["task"]["priority"], "high");
    }

    #[tokio::test]
    async fn test_create_date_node() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new_with_id(
            "2025-01-03".to_string(),
            "text".to_string(),
            "2025-01-03".to_string(),
            json!({}),
        );

        let id = service.create_node(node).await.unwrap();
        assert_eq!(id, "2025-01-03");

        let retrieved = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.node_type, "date");
        assert_eq!(retrieved.id, "2025-01-03");
    }

    #[tokio::test]
    async fn test_embedding_write_wrappers_upsert_read_delete() {
        // Pull-apply path: the non-nlp NodeService write wrappers must
        // round-trip a received vector into the local store and clear it.
        let (service, _temp) = create_test_service().await;
        let id = service
            .create_node(Node::new(
                "text".to_string(),
                "vec node".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        let mut vector = vec![0.0f32; 768];
        vector[3] = 1.0;
        let emb = crate::models::NewEmbedding::single_chunk(id.clone(), vector, "hash-3", 10, 4);
        service.upsert_embeddings(&id, vec![emb]).await.unwrap();

        let got = service.get_embeddings(&id).await.unwrap();
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].vector[3], 1.0, "applied vector round-trips");

        service.delete_embeddings(&id).await.unwrap();
        assert!(service.get_embeddings(&id).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_get_virtual_date_node_as_parent() {
        let (service, _temp) = create_test_service().await;

        // Verify the date node is returned as virtual (not persisted yet)
        let date_before = service.get_node("2025-10-13").await.unwrap().unwrap();
        assert_eq!(date_before.node_type, "date");
        assert_eq!(date_before.content, "2025-10-13"); // Virtual dates have correct content

        // Verify it's NOT persisted in database yet
        let filter = NodeFilter::new()
            .with_node_type("date".to_string())
            .with_ids(vec!["2025-10-13".to_string()]);
        let results = service.query_nodes(filter).await.unwrap();
        assert_eq!(results.len(), 0); // Not persisted yet - virtual only

        // For actual persistence when children are added, use NodeOperations
        // (NodeService is low-level, NodeOperations handles business logic like auto-creating dates)
    }

    #[tokio::test]
    async fn test_get_virtual_date_node() {
        let (service, _temp) = create_test_service().await;

        // Get a date node that doesn't exist in database
        // Should return virtual date node with correct properties
        let node = service.get_node("2025-10-13").await.unwrap();
        assert!(node.is_some());

        let date_node = node.unwrap();
        assert_eq!(date_node.id, "2025-10-13");
        assert_eq!(date_node.node_type, "date");
        assert_eq!(date_node.content, "2025-10-13"); // Virtual date nodes default content to the date ID
                                                     // Note: Sibling ordering is now on has_child relationship order field, not node.before_sibling_id
    }

    #[tokio::test]
    async fn test_get_virtual_date_node_not_persisted() {
        let (service, _temp) = create_test_service().await;

        // Get virtual date node
        let _virtual_node = service.get_node("2025-10-13").await.unwrap().unwrap();

        // Verify it's NOT in the database (virtual only)
        // Try to query it by filtering for date nodes specifically
        let filter = NodeFilter::new()
            .with_node_type("date".to_string())
            .with_ids(vec!["2025-10-13".to_string()]);
        let results = service.query_nodes(filter).await.unwrap();
        assert_eq!(results.len(), 0); // Not persisted yet - virtual only
    }

    #[tokio::test]
    async fn test_virtual_date_persists_when_child_created() {
        let (service, _temp) = create_test_service().await;

        // This test demonstrates that NodeOperations (not NodeService directly)
        // handles auto-persistence of date nodes when children are created.
        // NodeService is low-level storage, NodeOperations has business logic.

        // Verify virtual date exists
        let virtual_date = service.get_node("2025-10-13").await.unwrap().unwrap();
        assert_eq!(virtual_date.content, "2025-10-13");

        // Auto-persistence happens in NodeOperations.create_node, not NodeService
        // (see operations module tests for that behavior)
    }

    #[tokio::test]
    async fn test_get_node_returns_none_for_invalid_date() {
        let (service, _temp) = create_test_service().await;

        // Invalid date formats should return None
        let invalid1 = service.get_node("not-a-date").await.unwrap();
        assert!(invalid1.is_none());

        // Invalid dates (wrong format) should return None
        let invalid2 = service.get_node("25-10-13").await.unwrap(); // Wrong format
        assert!(invalid2.is_none());

        // Semantically invalid dates should return None
        let invalid3 = service.get_node("2025-13-45").await.unwrap(); // Invalid month/day
        assert!(invalid3.is_none());
    }

    #[tokio::test]
    async fn test_persisted_date_takes_precedence_over_virtual() {
        let (service, _temp) = create_test_service().await;

        // Create and persist a date node with custom content
        let date_node = Node::new_with_id(
            "2025-10-13".to_string(),
            "date".to_string(),
            "Custom Date Content".to_string(),
            json!({}), // No properties - date nodes use content only
        );

        service.create_node(date_node).await.unwrap();

        // Get the node - should return persisted version with custom content
        let retrieved = service.get_node("2025-10-13").await.unwrap().unwrap();
        assert_eq!(retrieved.content, "Custom Date Content");
        assert_eq!(retrieved.node_type, "date");
    }

    #[tokio::test]
    async fn test_update_node() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Original".to_string(), json!({}));

        let id = service.create_node(node).await.unwrap();

        let update = NodeUpdate::new().with_content("Updated".to_string());
        service.update_node_unchecked(&id, update).await.unwrap();

        let retrieved = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(retrieved.content, "Updated");
    }

    #[tokio::test]
    async fn test_delete_node() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "To be deleted".to_string(), json!({}));

        let id = service.create_node(node).await.unwrap();
        service.delete_node_unchecked(&id).await.unwrap();

        let retrieved = service.get_node(&id).await.unwrap();
        assert!(retrieved.is_none());
    }

    #[tokio::test]
    async fn test_query_nodes_by_type() {
        let (service, _temp) = create_test_service().await;

        service
            .create_node(Node::new(
                "text".to_string(),
                "Text 1".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        service
            .create_node(Node::new(
                "task".to_string(),
                "Task 1".to_string(),
                json!({"status": "open"}),
            ))
            .await
            .unwrap();
        service
            .create_node(Node::new(
                "text".to_string(),
                "Text 2".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        let filter = NodeFilter::new().with_node_type("text".to_string());
        let results = service.query_nodes(filter).await.unwrap();

        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|n| n.node_type == "text"));
    }

    #[tokio::test]
    async fn test_bulk_create() {
        let (service, _temp) = create_test_service().await;

        let nodes = vec![
            Node::new("text".to_string(), "Bulk 1".to_string(), json!({})),
            Node::new("text".to_string(), "Bulk 2".to_string(), json!({})),
            Node::new(
                "task".to_string(),
                "Bulk Task".to_string(),
                json!({"status": "open"}),
            ),
        ];

        let ids = service.bulk_create(nodes.clone()).await.unwrap();
        assert_eq!(ids.len(), 3);

        for (i, id) in ids.iter().enumerate() {
            let node = service.get_node(id).await.unwrap().unwrap();
            assert_eq!(node.content, nodes[i].content);
        }
    }

    #[tokio::test]
    async fn test_bulk_update() {
        let (service, _temp) = create_test_service().await;

        let node1 = Node::new("text".to_string(), "Original 1".to_string(), json!({}));
        let node2 = Node::new("text".to_string(), "Original 2".to_string(), json!({}));

        let id1 = service.create_node(node1).await.unwrap();
        let id2 = service.create_node(node2).await.unwrap();

        let updates = vec![
            (
                id1.clone(),
                NodeUpdate::new().with_content("Updated 1".to_string()),
            ),
            (
                id2.clone(),
                NodeUpdate::new().with_content("Updated 2".to_string()),
            ),
        ];

        service.bulk_update(updates).await.unwrap();

        let retrieved1 = service.get_node(&id1).await.unwrap().unwrap();
        let retrieved2 = service.get_node(&id2).await.unwrap().unwrap();

        assert_eq!(retrieved1.content, "Updated 1");
        assert_eq!(retrieved2.content, "Updated 2");
    }

    #[tokio::test]
    async fn test_bulk_update_partial_fields_preserves_unspecified() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new(
            "text".to_string(),
            "Original content".to_string(),
            json!({}),
        );
        let id = service.create_node(node).await.unwrap();

        // Update only content — node_type must be preserved
        service
            .bulk_update(vec![(
                id.clone(),
                NodeUpdate::new().with_content("New content".to_string()),
            )])
            .await
            .unwrap();
        let after_content_update = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(after_content_update.content, "New content");
        assert_eq!(after_content_update.node_type, "text");

        // Update only node_type — content must be preserved
        service
            .bulk_update(vec![(
                id.clone(),
                NodeUpdate::new().with_node_type("text".to_string()),
            )])
            .await
            .unwrap();
        let after_type_update = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(after_type_update.content, "New content");
        assert_eq!(after_type_update.node_type, "text");

        // Update only properties — content and node_type must be preserved.
        // bulk_update now NORMALIZES + deep-MERGES properties exactly like the
        // single-node update_node path (it previously wholesale-replaced with the raw
        // client value, diverging from single-update and skipping normalization).
        service
            .bulk_update(vec![(
                id.clone(),
                NodeUpdate::new().with_properties(json!({"key": "value"})),
            )])
            .await
            .unwrap();
        let after_props_update = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(after_props_update.content, "New content");
        assert_eq!(after_props_update.node_type, "text");
        let props = after_props_update.properties.to_string();
        assert!(
            props.contains("key") && props.contains("value"),
            "property must be merged + normalized into the node: {props}"
        );
    }

    #[tokio::test]
    async fn test_bulk_update_bad_id_rolls_back() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Will not change".to_string(), json!({}));
        let good_id = service.create_node(node).await.unwrap();

        let result = service
            .bulk_update(vec![
                (
                    good_id.clone(),
                    NodeUpdate::new().with_content("Changed".to_string()),
                ),
                (
                    "nonexistent-id".to_string(),
                    NodeUpdate::new().with_content("Also changed".to_string()),
                ),
            ])
            .await;

        assert!(result.is_err(), "bulk_update with bad id must fail");
        // The good node must be unchanged — transaction rolled back
        let node = service.get_node(&good_id).await.unwrap().unwrap();
        assert_eq!(node.content, "Will not change");
    }

    #[tokio::test]
    async fn test_bulk_delete() {
        let (service, _temp) = create_test_service().await;

        let node1 = Node::new("text".to_string(), "Delete 1".to_string(), json!({}));
        let node2 = Node::new("text".to_string(), "Delete 2".to_string(), json!({}));

        let id1 = service.create_node(node1).await.unwrap();
        let id2 = service.create_node(node2).await.unwrap();

        service
            .bulk_delete(vec![id1.clone(), id2.clone()])
            .await
            .unwrap();

        assert!(service.get_node(&id1).await.unwrap().is_none());
        assert!(service.get_node(&id2).await.unwrap().is_none());
    }

    // NOTE: The remaining tests from the original node_service.rs are included below.
    // They are preserved verbatim from lines 7083 onwards of the original file.

    #[tokio::test]
    async fn test_schema_validation() {
        let (service, _temp) = create_test_service().await;

        // Valid task with all required properties
        let task = Node::new(
            "task".to_string(),
            "Valid Task".to_string(),
            json!({"status": "open", "priority": "medium"}),
        );
        service.create_node(task).await.unwrap();
    }

    #[tokio::test]
    async fn test_hierarchy_operations() {
        let (service, _temp) = create_test_service().await;

        // Create parent
        let parent = Node::new("text".to_string(), "Parent".to_string(), json!({}));
        let parent_id = service.create_node(parent).await.unwrap();

        // Create child using create_node_with_parent
        let child_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child".to_string(),
                parent_id: Some(parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        // Verify hierarchy
        let children = service.get_children(&parent_id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].id, child_id);

        let parent_node = service.get_parent(&child_id).await.unwrap();
        assert!(parent_node.is_some());
        assert_eq!(parent_node.unwrap().id, parent_id);
    }

    #[tokio::test]
    async fn test_date_auto_creation() {
        let (service, _temp) = create_test_service().await;

        // Create a node with date as parent - date should auto-create
        let child_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Note under date".to_string(),
                parent_id: Some("2025-06-15".to_string()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        // Date node should now exist
        let date_node = service.get_node("2025-06-15").await.unwrap();
        assert!(date_node.is_some());
        assert_eq!(date_node.unwrap().node_type, "date");

        // Child should be under date node
        let children = service.get_children("2025-06-15").await.unwrap();
        assert!(children.iter().any(|c| c.id == child_id));
    }

    #[tokio::test]
    async fn test_mention_operations() {
        let (service, _temp) = create_test_service().await;

        let node1 = Node::new("text".to_string(), "Node 1".to_string(), json!({}));
        let node2 = Node::new("text".to_string(), "Node 2".to_string(), json!({}));

        let id1 = service.create_node(node1).await.unwrap();
        let id2 = service.create_node(node2).await.unwrap();

        // Create mention
        service.create_mention(&id1, &id2).await.unwrap();

        // Get mentions
        let mentions = service.get_mentions(&id1).await.unwrap();
        assert!(mentions.contains(&id2));

        // Get backlinks
        let backlinks = service.get_mentioned_by(&id2).await.unwrap();
        assert!(backlinks.contains(&id1));

        // Delete mention
        service.delete_mention(&id1, &id2).await.unwrap();

        let mentions_after = service.get_mentions(&id1).await.unwrap();
        assert!(!mentions_after.contains(&id2));
    }

    #[tokio::test]
    async fn test_version_conflict_detection() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Original".to_string(), json!({}));
        let id = service.create_node(node).await.unwrap();

        // First update succeeds
        let update1 = NodeUpdate::new().with_content("First update".to_string());
        let updated = service.update_node(&id, 1, update1).await.unwrap();
        assert_eq!(updated.version, 2);

        // Second update with old version fails
        let update2 = NodeUpdate::new().with_content("Stale update".to_string());
        let result = service.update_node(&id, 1, update2).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_rejects_unsupported_lifecycle_status() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Original".to_string(), json!({}));
        let id = service.create_node(node).await.unwrap();

        // Writing an unsupported lifecycle_status ("deleted") must be rejected —
        // there is no soft-delete state, only "active"/"archived".
        let bad = NodeUpdate::new().with_lifecycle_status("deleted".to_string());
        let err = service
            .update_node(&id, 1, bad)
            .await
            .expect_err("update with lifecycle_status 'deleted' must be rejected");
        assert!(
            err.to_string().contains("lifecycle_status"),
            "error should name the offending field, got: {err}"
        );

        // The rejected write must leave the persisted row untouched (still active, v1).
        let unchanged = service.get_node(&id).await.unwrap().unwrap();
        assert_eq!(unchanged.lifecycle_status, "active");
        assert_eq!(unchanged.version, 1);
    }

    #[tokio::test]
    async fn test_active_archived_lifecycle_transitions_succeed() {
        let (service, _temp) = create_test_service().await;

        let node = Node::new("text".to_string(), "Original".to_string(), json!({}));
        let id = service.create_node(node).await.unwrap();

        // active -> archived
        let archived = service
            .update_node(
                &id,
                1,
                NodeUpdate::new().with_lifecycle_status("archived".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(archived.lifecycle_status, "archived");
        assert_eq!(archived.version, 2);

        // archived -> active
        let reactivated = service
            .update_node(
                &id,
                2,
                NodeUpdate::new().with_lifecycle_status("active".to_string()),
            )
            .await
            .unwrap();
        assert_eq!(reactivated.lifecycle_status, "active");
        assert_eq!(reactivated.version, 3);
    }

    #[tokio::test]
    async fn test_all_store_write_paths_reject_unsupported_lifecycle_status() {
        let (service, _temp) = create_test_service().await;
        let node = Node::new("text".to_string(), "Original".to_string(), json!({}));
        let id = service.create_node(node).await.unwrap();
        let store = service.store();

        // Direct single-column write path.
        assert!(
            store.update_lifecycle_status(&id, "deleted").await.is_err(),
            "update_lifecycle_status must reject 'deleted'"
        );
        // A supported value on the same path still succeeds.
        store
            .update_lifecycle_status(&id, "archived")
            .await
            .expect("update_lifecycle_status must accept 'archived'");

        // Batch write path.
        assert!(
            store
                .bulk_update(vec![(
                    id.clone(),
                    NodeUpdate::new().with_lifecycle_status("deleted".to_string()),
                )])
                .await
                .is_err(),
            "bulk_update must reject 'deleted'"
        );

        // Insert write path.
        let mut bad_new = Node::new("text".to_string(), "Bad".to_string(), json!({}));
        bad_new.lifecycle_status = "deleted".to_string();
        assert!(
            store.create_node(bad_new, None, None).await.is_err(),
            "create_node must reject 'deleted'"
        );
    }

    #[tokio::test]
    async fn test_delete_with_cascade() {
        let (service, _temp) = create_test_service().await;

        // Create parent and children
        let parent = Node::new("text".to_string(), "Parent".to_string(), json!({}));
        let parent_id = service.create_node(parent).await.unwrap();

        let child_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child".to_string(),
                parent_id: Some(parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        // Delete parent - should cascade to child
        let parent_version = service.get_node(&parent_id).await.unwrap().unwrap().version;
        service
            .delete_node(&parent_id, parent_version)
            .await
            .unwrap();

        // Both should be gone
        assert!(service.get_node(&parent_id).await.unwrap().is_none());
        assert!(service.get_node(&child_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_query_with_property_filters() {
        let (service, _temp) = create_test_service().await;

        // Create tasks with different statuses
        service
            .create_node(Node::new(
                "task".to_string(),
                "Open task".to_string(),
                json!({"status": "open"}),
            ))
            .await
            .unwrap();
        service
            .create_node(Node::new(
                "task".to_string(),
                "Done task".to_string(),
                json!({"status": "done"}),
            ))
            .await
            .unwrap();

        // Filter by status
        let filter = NodeFilter::new()
            .with_node_type("task".to_string())
            .with_property_filter(
                crate::models::PropertyFilter::new(
                    "$.status".to_string(),
                    FilterOperator::Equals,
                    json!("open"),
                )
                .unwrap(),
            );

        let results = service.query_nodes(filter).await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].content, "Open task");
    }

    #[tokio::test]
    async fn test_schema_node_operations() {
        let (service, _temp) = create_test_service().await;

        // Get the task schema (seeded at startup)
        let schema = service.get_schema_node("task").await.unwrap();
        assert!(schema.is_some());
        let schema = schema.unwrap();
        assert!(!schema.fields.is_empty());
    }

    #[tokio::test]
    async fn test_get_all_schemas() {
        let (service, _temp) = create_test_service().await;

        let schemas = service.get_all_schemas().await.unwrap();
        assert!(!schemas.is_empty());

        // Should include core schemas
        let task_schema = schemas.iter().find(|s| s.id == "task");
        assert!(task_schema.is_some());
    }

    #[tokio::test]
    async fn test_extract_mentions_simple() {
        let content = "See [@Node](nodespace://550e8400-e29b-41d4-a716-446655440000) for details";
        let mentions = extract_mentions(content);
        assert_eq!(mentions.len(), 1);
        assert!(mentions.contains(&"550e8400-e29b-41d4-a716-446655440000".to_string()));
    }

    #[tokio::test]
    async fn test_extract_mentions_date() {
        let content = "Today is nodespace://2025-10-24";
        let mentions = extract_mentions(content);
        assert_eq!(mentions.len(), 1);
        assert!(mentions.contains(&"2025-10-24".to_string()));
    }

    #[test]
    fn test_extract_mentions_wikilink_uuid_and_date() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let content = format!("See [[{uuid}]] and the log for [[2025-10-24]].");
        let mentions = extract_mentions(&content);
        assert_eq!(mentions.len(), 2);
        assert!(mentions.contains(&uuid.to_string()));
        assert!(mentions.contains(&"2025-10-24".to_string()));
    }

    #[test]
    fn test_extract_mentions_wikilink_strips_node_prefix() {
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let content = format!("Ref [[node/{uuid}]]");
        let mentions = extract_mentions(&content);
        assert_eq!(mentions, vec![uuid.to_string()]);
    }

    #[test]
    fn test_extract_mentions_wikilink_ignores_non_ids() {
        // A bracketed phrase (has whitespace / isn't a valid id) is not a mention;
        // a token that isn't a UUID or date is filtered by is_valid_node_id.
        let content = "[[some page title]] and [[not-a-real-id]] and [[]]";
        let mentions = extract_mentions(content);
        assert!(mentions.is_empty());
    }

    #[test]
    fn test_extract_mentions_wikilink_nested_brackets_not_a_mention() {
        // Malformed nesting corrupts the captured token with a stray bracket, which
        // is_valid_node_id rejects — a triple-bracketed id is not a mention.
        let uuid = "550e8400-e29b-41d4-a716-446655440000";
        let content = format!("[[[{uuid}]]]");
        assert!(extract_mentions(&content).is_empty());
    }

    #[test]
    fn test_extract_mentions_wikilink_dedups_with_markdown_form() {
        // The same id referenced once as a wikilink and once as a markdown link is a
        // single mention; a second distinct id adds one more.
        let a = "550e8400-e29b-41d4-a716-446655440000";
        let b = "6ba7b810-9dad-11d1-80b4-00c04fd430c8";
        let content = format!("[[{a}]] then [@X](nodespace://{a}) and [[{b}]]");
        let mut mentions = extract_mentions(&content);
        mentions.sort();
        assert_eq!(mentions, vec![a.to_string(), b.to_string()]);
    }

    #[tokio::test]
    async fn test_is_valid_node_id_uuid() {
        assert!(is_valid_node_id("550e8400-e29b-41d4-a716-446655440000"));
        assert!(!is_valid_node_id("invalid-id"));
        assert!(!is_valid_node_id(""));
    }

    #[tokio::test]
    async fn test_is_valid_node_id_date() {
        assert!(is_valid_node_id("2025-10-24"));
        assert!(!is_valid_node_id("2025-13-24")); // Invalid month
        assert!(!is_valid_node_id("25-10-24")); // Wrong year format
    }

    // C3b: container rule enforcement tests
    #[tokio::test]
    async fn test_move_node_rejects_non_container_parent() {
        let (service, _temp) = create_test_service().await;

        // query nodes cannot have children
        let leaf = Node::new("query".to_string(), "my query".to_string(), json!({}));
        let leaf_id = service.create_node(leaf).await.unwrap();

        let child = Node::new("text".to_string(), "child".to_string(), json!({}));
        let child_id = service.create_node(child).await.unwrap();
        let child_node = service.get_node(&child_id).await.unwrap().unwrap();

        let result = service
            .move_node(
                &child_id,
                child_node.version,
                Some(&leaf_id),
                crate::services::InsertPosition::End,
            )
            .await;

        assert!(
            matches!(result, Err(NodeServiceError::NotAContainer { .. })),
            "move_node should reject a non-container parent; got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_move_node_accepts_container_parent() {
        let (service, _temp) = create_test_service().await;

        let container = Node::new("text".to_string(), "container".to_string(), json!({}));
        let container_id = service.create_node(container).await.unwrap();

        let child = Node::new("text".to_string(), "child".to_string(), json!({}));
        let child_id = service.create_node(child).await.unwrap();
        let child_node = service.get_node(&child_id).await.unwrap().unwrap();

        let result = service
            .move_node(
                &child_id,
                child_node.version,
                Some(&container_id),
                crate::services::InsertPosition::End,
            )
            .await;

        assert!(result.is_ok(), "move_node should accept a container parent");
    }

    #[tokio::test]
    async fn test_reparenting_a_collection_member_is_rejected() {
        // ADR-059 §2 (reparent side): a content node that holds a `member_of` edge
        // is a root member; moving it under a parent would make it a forbidden
        // interior member. The store-level guard rejects it on every reparent path
        // — service `move_node` AND `upsert_node_with_parent` (the gRPC /
        // save_node_with_parent path) — and must NOT silently drop the membership.
        // A move to root stays allowed.
        let (service, _temp) = create_test_service().await;

        let coll = Node::new("collection".to_string(), "Coll".to_string(), json!({}));
        let coll_id = service.create_node(coll).await.unwrap();

        let root = Node::new("text".to_string(), "root doc".to_string(), json!({}));
        let root_id = service.create_node(root).await.unwrap();
        service
            .create_relationship(&root_id, "member_of", &coll_id, json!({}))
            .await
            .unwrap();

        let parent = Node::new("text".to_string(), "a parent".to_string(), json!({}));
        let parent_id = service.create_node(parent).await.unwrap();

        // Path 1 — service move_node reparent → rejected, naming the collection.
        let root_node = service.get_node(&root_id).await.unwrap().unwrap();
        let err = service
            .move_node(
                &root_id,
                root_node.version,
                Some(&parent_id),
                crate::services::InsertPosition::End,
            )
            .await
            .expect_err("reparenting a collection member via move_node must be rejected");
        assert!(
            err.to_string().contains("member_of_not_root") && err.to_string().contains(&coll_id),
            "rejection must state the reason and name the collection; got: {err}"
        );

        // The rejected move did not drop the membership: a second attempt rejects.
        let root_node = service.get_node(&root_id).await.unwrap().unwrap();
        let again = service
            .move_node(
                &root_id,
                root_node.version,
                Some(&parent_id),
                crate::services::InsertPosition::End,
            )
            .await;
        assert!(
            again.is_err(),
            "membership must be intact after a rejected move; got: {:?}",
            again
        );

        // Path 2 — upsert_node_with_parent is a DISTINCT reparent path (gRPC
        // upsert / the save_node_with_parent Tauri command); it must be gated too.
        let err2 = service
            .upsert_node_with_parent(&root_id, "root doc", "text", &parent_id, &root_id, None)
            .await
            .expect_err("reparenting via upsert_node_with_parent must be rejected");
        assert!(
            err2.to_string().contains("member_of_not_root"),
            "upsert_node_with_parent reparent must be gated; got: {err2}"
        );

        // Moving a member to root (new_parent = None) is still allowed.
        let root_node = service.get_node(&root_id).await.unwrap().unwrap();
        let to_root = service
            .move_node(
                &root_id,
                root_node.version,
                None,
                crate::services::InsertPosition::End,
            )
            .await;
        assert!(
            to_root.is_ok(),
            "moving a collection member to root must remain allowed; got: {:?}",
            to_root
        );
    }

    #[tokio::test]
    async fn test_create_node_with_parent_rejects_non_container() {
        let (service, _temp) = create_test_service().await;

        let leaf = Node::new("query".to_string(), "leaf query".to_string(), json!({}));
        let leaf_id = service.create_node(leaf).await.unwrap();

        let result = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "child".to_string(),
                parent_id: Some(leaf_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await;

        assert!(
            matches!(result, Err(NodeServiceError::NotAContainer { .. })),
            "create_node_with_parent should reject a non-container parent; got: {:?}",
            result
        );
    }

    #[tokio::test]
    async fn test_create_node_with_parent_accepts_container() {
        let (service, _temp) = create_test_service().await;

        let container = Node::new("text".to_string(), "container".to_string(), json!({}));
        let container_id = service.create_node(container).await.unwrap();

        let result = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "child".to_string(),
                parent_id: Some(container_id),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await;

        assert!(
            result.is_ok(),
            "create_node_with_parent should accept a container parent"
        );
    }

    // C3c: atomic child-transfer tests
    #[tokio::test]
    async fn test_move_children_to_parent_success() {
        let (service, _temp) = create_test_service().await;

        let parent = Node::new("text".to_string(), "Original Parent".to_string(), json!({}));
        let parent_id = service.create_node(parent).await.unwrap();

        let new_parent = Node::new("text".to_string(), "New Parent".to_string(), json!({}));
        let new_parent_id = service.create_node(new_parent).await.unwrap();

        // Create two children under the original parent
        let child1_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child 1".to_string(),
                parent_id: Some(parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        let child2_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child 2".to_string(),
                parent_id: Some(parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        let child1 = service.get_node(&child1_id).await.unwrap().unwrap();
        let child2 = service.get_node(&child2_id).await.unwrap().unwrap();

        let children = vec![
            (child1_id.clone(), child1.version),
            (child2_id.clone(), child2.version),
        ];

        let updated = service
            .move_children_to_parent(&new_parent_id, &children)
            .await
            .unwrap();

        assert_eq!(updated.len(), 2);

        // Children now live under new_parent — version was bumped
        let updated1 = service.get_node(&child1_id).await.unwrap().unwrap();
        let updated2 = service.get_node(&child2_id).await.unwrap().unwrap();
        assert!(updated1.version > child1.version);
        assert!(updated2.version > child2.version);

        // Verify actual parent relationships via get_children
        let new_parent_children = service.get_children(&new_parent_id).await.unwrap();
        let new_child_ids: Vec<&str> = new_parent_children.iter().map(|n| n.id.as_str()).collect();
        assert!(new_child_ids.contains(&child1_id.as_str()));
        assert!(new_child_ids.contains(&child2_id.as_str()));

        // Original parent now has no children
        let original_children = service.get_children(&parent_id).await.unwrap();
        assert!(original_children.is_empty());
    }

    #[tokio::test]
    async fn test_move_children_to_parent_all_or_nothing_on_stale_version() {
        let (service, _temp) = create_test_service().await;

        let new_parent = Node::new("text".to_string(), "New Parent".to_string(), json!({}));
        let new_parent_id = service.create_node(new_parent).await.unwrap();

        let old_parent = Node::new("text".to_string(), "Old Parent".to_string(), json!({}));
        let old_parent_id = service.create_node(old_parent).await.unwrap();

        let child1_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child 1".to_string(),
                parent_id: Some(old_parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        let child2_id = service
            .create_node_with_parent(CreateNodeParams {
                id: None,
                node_type: "text".to_string(),
                content: "Child 2".to_string(),
                parent_id: Some(old_parent_id.clone()),
                position: crate::services::InsertPositionOwned::End,
                properties: json!({}),
            })
            .await
            .unwrap();

        let child1 = service.get_node(&child1_id).await.unwrap().unwrap();
        // child2 uses stale version 0 — should trigger OCC failure
        let stale_version = 0;

        let children = vec![
            (child1_id.clone(), child1.version),
            (child2_id.clone(), stale_version),
        ];

        let result = service
            .move_children_to_parent(&new_parent_id, &children)
            .await;

        assert!(result.is_err(), "stale version should cause failure");
        let err = result.unwrap_err();
        assert!(
            matches!(err, NodeServiceError::VersionConflict { .. }),
            "expected VersionConflict, got {:?}",
            err
        );

        // ALL-OR-NOTHING: child1 must still be under old_parent, not new_parent
        let old_children = service.get_children(&old_parent_id).await.unwrap();
        let old_ids: Vec<&str> = old_children.iter().map(|n| n.id.as_str()).collect();
        assert!(
            old_ids.contains(&child1_id.as_str()),
            "child1 should still be under old_parent after rollback"
        );
        assert!(
            old_ids.contains(&child2_id.as_str()),
            "child2 should still be under old_parent after rollback"
        );

        let new_children = service.get_children(&new_parent_id).await.unwrap();
        assert!(
            new_children.is_empty(),
            "new_parent should have no children after rollback"
        );
    }

    #[tokio::test]
    async fn test_move_children_to_parent_rejects_non_container_parent() {
        let (service, _temp) = create_test_service().await;

        // query nodes cannot have children
        let leaf = Node::new("query".to_string(), "my query".to_string(), json!({}));
        let leaf_id = service.create_node(leaf).await.unwrap();

        let child = Node::new("text".to_string(), "child".to_string(), json!({}));
        let child_id = service.create_node(child).await.unwrap();
        let child_node = service.get_node(&child_id).await.unwrap().unwrap();

        let result = service
            .move_children_to_parent(&leaf_id, &[(child_id.clone(), child_node.version)])
            .await;

        assert!(result.is_err());
        assert!(
            matches!(result.unwrap_err(), NodeServiceError::NotAContainer { .. }),
            "expected NotAContainer error"
        );
    }

    #[tokio::test]
    async fn test_move_children_to_parent_preserves_sibling_order() {
        let (service, _temp) = create_test_service().await;

        let new_parent = Node::new("text".to_string(), "New Parent".to_string(), json!({}));
        let new_parent_id = service.create_node(new_parent).await.unwrap();

        let old_parent = Node::new("text".to_string(), "Old Parent".to_string(), json!({}));
        let old_parent_id = service.create_node(old_parent).await.unwrap();

        let mut child_ids = Vec::new();
        for i in 0..3 {
            let id = service
                .create_node_with_parent(CreateNodeParams {
                    id: None,
                    node_type: "text".to_string(),
                    content: format!("Child {}", i),
                    parent_id: Some(old_parent_id.clone()),
                    position: crate::services::InsertPositionOwned::End,
                    properties: json!({}),
                })
                .await
                .unwrap();
            child_ids.push(id);
        }

        let children: Vec<(String, i64)> = {
            let mut v = Vec::new();
            for id in &child_ids {
                let n = service.get_node(id).await.unwrap().unwrap();
                v.push((id.clone(), n.version));
            }
            v
        };

        service
            .move_children_to_parent(&new_parent_id, &children)
            .await
            .unwrap();

        // get_children returns in order — confirm the sequence is preserved
        let new_children = service.get_children(&new_parent_id).await.unwrap();
        assert_eq!(new_children.len(), 3);
        let returned_ids: Vec<&str> = new_children.iter().map(|n| n.id.as_str()).collect();
        assert_eq!(
            returned_ids,
            child_ids.iter().map(|s| s.as_str()).collect::<Vec<_>>()
        );
    }

    mod node_accessor_tests {
        use super::*;

        #[tokio::test]
        async fn test_node_accessor_get_node() {
            let (service, _temp) = create_test_service().await;

            let node = Node::new("text".to_string(), "Accessor Test".to_string(), json!({}));
            let node_id = node.id.clone();
            service.create_node(node).await.unwrap();

            let accessor: &dyn NodeAccessor = &service;
            let result = accessor.get_node(&node_id).await.unwrap();
            assert!(result.is_some());
            assert_eq!(result.unwrap().content, "Accessor Test");
        }

        #[tokio::test]
        async fn test_node_accessor_get_children() {
            let (service, _temp) = create_test_service().await;

            let parent = Node::new("text".to_string(), "Parent".to_string(), json!({}));
            let parent_id = parent.id.clone();
            service.create_node(parent).await.unwrap();

            let child1_id = service
                .create_node_with_parent(CreateNodeParams {
                    id: None,
                    node_type: "text".to_string(),
                    content: "Child 1".to_string(),
                    parent_id: Some(parent_id.clone()),
                    position: crate::services::InsertPositionOwned::End,
                    properties: json!({}),
                })
                .await
                .unwrap();

            let _child2_id = service
                .create_node_with_parent(CreateNodeParams {
                    id: None,
                    node_type: "text".to_string(),
                    content: "Child 2".to_string(),
                    parent_id: Some(parent_id.clone()),
                    position: crate::services::InsertPositionOwned::End,
                    properties: json!({}),
                })
                .await
                .unwrap();

            let accessor: &dyn NodeAccessor = &service;
            let children = accessor.get_children(&parent_id).await.unwrap();
            assert_eq!(
                children.len(),
                2,
                "NodeAccessor::get_children should return 2 children"
            );

            // Node with no children returns empty vec
            let empty = accessor.get_children(&child1_id).await.unwrap();
            assert!(
                empty.is_empty(),
                "NodeAccessor::get_children for leaf node should be empty"
            );
        }

        #[tokio::test]
        async fn test_node_accessor_get_nodes_batch() {
            let (service, _temp) = create_test_service().await;

            let n1 = Node::new("text".to_string(), "Batch 1".to_string(), json!({}));
            let n2 = Node::new("text".to_string(), "Batch 2".to_string(), json!({}));
            let n3 = Node::new("text".to_string(), "Batch 3".to_string(), json!({}));
            let id1 = n1.id.clone();
            let id2 = n2.id.clone();
            let id3 = n3.id.clone();
            service.create_node(n1).await.unwrap();
            service.create_node(n2).await.unwrap();
            service.create_node(n3).await.unwrap();

            let accessor: &dyn NodeAccessor = &service;
            let batch = accessor
                .get_nodes(&[&id1, &id2, &id3, "nonexistent"])
                .await
                .unwrap();
            assert_eq!(
                batch.len(),
                3,
                "NodeAccessor::get_nodes should return only existing nodes"
            );

            let contents: HashSet<String> = batch.into_iter().map(|n| n.content).collect();
            assert!(contents.contains("Batch 1"));
            assert!(contents.contains("Batch 2"));
            assert!(contents.contains("Batch 3"));
        }
    }

    // ---------------------------------------------------------------------------
    // Atomic subtree cascade delete tests
    // ---------------------------------------------------------------------------

    #[tokio::test]
    async fn test_delete_node_cascades_subtree_atomically() {
        let (service, _temp) = create_test_service().await;

        // Build: root → child1 → grandchild
        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = service
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let grandchild_id = service
            .create_node(Node::new(
                "text".to_string(),
                "grandchild".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        service
            .create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();
        service
            .create_parent_edge(&grandchild_id, &child_id, InsertPosition::End)
            .await
            .unwrap();

        // Delete root → all three nodes must disappear
        let root = service.get_node(&root_id).await.unwrap().unwrap();
        let result = service.delete_node(&root_id, root.version).await.unwrap();

        assert!(result.existed);
        assert_eq!(result.deleted_count, 3, "root + child + grandchild = 3");

        assert!(service.get_node(&root_id).await.unwrap().is_none());
        assert!(service.get_node(&child_id).await.unwrap().is_none());
        assert!(service.get_node(&grandchild_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_node_version_conflict_leaves_subtree_intact() {
        let (service, _temp) = create_test_service().await;

        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = service
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        service
            .create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();

        // Delete with stale version → VersionConflict, nothing deleted
        let result = service.delete_node(&root_id, 999).await;
        assert!(
            matches!(result, Err(NodeServiceError::VersionConflict { .. })),
            "expected VersionConflict, got {:?}",
            result
        );

        // Subtree intact
        assert!(service.get_node(&root_id).await.unwrap().is_some());
        assert!(service.get_node(&child_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_node_occ_guards_only_target_not_descendants() {
        let (service, _temp) = create_test_service().await;

        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = service
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        service
            .create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();

        // Update the child (bumping its version) — should NOT cause the cascade to fail
        let child_update = NodeUpdate::new().with_content("updated child".to_string());
        service
            .update_node_unchecked(&child_id, child_update)
            .await
            .unwrap();

        // Delete root with its correct version — cascade should succeed despite child version change
        let root = service.get_node(&root_id).await.unwrap().unwrap();
        let result = service.delete_node(&root_id, root.version).await.unwrap();

        assert!(result.existed);
        assert_eq!(result.deleted_count, 2);

        assert!(service.get_node(&root_id).await.unwrap().is_none());
        assert!(service.get_node(&child_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_node_idempotent_for_missing_node() {
        let (service, _temp) = create_test_service().await;

        let result = service.delete_node("nonexistent-id", 1).await.unwrap();
        assert!(!result.existed);
        assert_eq!(result.deleted_count, 0);
    }

    // ---------------------------------------------------------------------------
    // Access-gated cascade delete (ADR-041 "CASCADE requires read access across
    // the whole subtree"). Community installs always see AlwaysAllowGate (the
    // default) — these tests inject a stub gate to simulate a synced Pro tenant
    // denying access to part of a subtree, since nodespace-core itself has no
    // access-control concept to exercise otherwise.
    // ---------------------------------------------------------------------------

    /// Denies whenever the checked set contains a designated "restricted" node id —
    /// stands in for a Pro tenant gate reporting a restricted descendant the actor
    /// isn't a member of.
    struct DenyIfPresentGate {
        restricted_id: String,
    }

    #[async_trait::async_trait]
    impl crate::services::node_service::access_gate::SubtreeAccessGate for DenyIfPresentGate {
        async fn check_subtree_access(
            &self,
            node_ids: &[String],
        ) -> crate::services::node_service::access_gate::SubtreeAccessDecision {
            let inaccessible_count = node_ids
                .iter()
                .filter(|id| **id == self.restricted_id)
                .count() as u64;
            if inaccessible_count > 0 {
                crate::services::node_service::access_gate::SubtreeAccessDecision::Denied {
                    inaccessible_count,
                }
            } else {
                crate::services::node_service::access_gate::SubtreeAccessDecision::Allowed
            }
        }
    }

    #[tokio::test]
    async fn test_delete_node_refused_when_subtree_has_restricted_descendant() {
        let (service, _temp) = create_test_service().await;

        // Project (open) -> Task (restricted, actor is not a member)
        let project_id = service
            .create_node(Node::new(
                "text".to_string(),
                "Platform Migration".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let task_id = service
            .create_node(Node::new(
                "text".to_string(),
                "Contractor rate renegotiation".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        service
            .create_parent_edge(&task_id, &project_id, InsertPosition::End)
            .await
            .unwrap();

        service.set_subtree_access_gate(std::sync::Arc::new(DenyIfPresentGate {
            restricted_id: task_id.clone(),
        }));

        let project = service.get_node(&project_id).await.unwrap().unwrap();
        let result = service.delete_node(&project_id, project.version).await;

        assert!(
            matches!(result, Err(NodeServiceError::HierarchyViolation(_))),
            "expected HierarchyViolation, got {:?}",
            result
        );

        // Neither the open parent nor the restricted descendant was deleted — the
        // whole delete aborted, no partial removal.
        assert!(
            service.get_node(&project_id).await.unwrap().is_some(),
            "open parent must survive a refused delete"
        );
        assert!(
            service.get_node(&task_id).await.unwrap().is_some(),
            "restricted descendant must survive a refused delete"
        );
    }

    #[tokio::test]
    async fn test_delete_node_succeeds_when_subtree_fully_readable() {
        let (service, _temp) = create_test_service().await;

        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = service
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        service
            .create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();

        // Gate is configured to deny a node NOT in this subtree — every node here
        // is readable, so the delete must succeed unchanged.
        service.set_subtree_access_gate(std::sync::Arc::new(DenyIfPresentGate {
            restricted_id: "some-other-restricted-node".to_string(),
        }));

        let root = service.get_node(&root_id).await.unwrap().unwrap();
        let result = service.delete_node(&root_id, root.version).await.unwrap();

        assert!(result.existed);
        assert_eq!(result.deleted_count, 2);
        assert!(service.get_node(&root_id).await.unwrap().is_none());
        assert!(service.get_node(&child_id).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn test_delete_node_version_conflict_still_wins_over_access_gate() {
        let (service, _temp) = create_test_service().await;

        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();

        // A gate that would allow this subtree — proves the version conflict is
        // reached and reported on its own terms, not masked by the gate.
        service.set_subtree_access_gate(std::sync::Arc::new(DenyIfPresentGate {
            restricted_id: "unrelated".to_string(),
        }));

        let result = service.delete_node(&root_id, 999).await;
        assert!(
            matches!(result, Err(NodeServiceError::VersionConflict { .. })),
            "expected VersionConflict, got {:?}",
            result
        );
        assert!(service.get_node(&root_id).await.unwrap().is_some());
    }

    #[tokio::test]
    async fn test_delete_node_occ_on_readable_descendant_unaffected_by_gate() {
        let (service, _temp) = create_test_service().await;

        let root_id = service
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = service
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        service
            .create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();

        // Gate allows everything (readable subtree) — pins that a concurrent edit to
        // a readable descendant still doesn't abort the cascade with the gate active,
        // matching test_delete_node_occ_guards_only_target_not_descendants.
        service.set_subtree_access_gate(std::sync::Arc::new(DenyIfPresentGate {
            restricted_id: "unrelated".to_string(),
        }));

        let child_update = NodeUpdate::new().with_content("updated child".to_string());
        service
            .update_node_unchecked(&child_id, child_update)
            .await
            .unwrap();

        let root = service.get_node(&root_id).await.unwrap().unwrap();
        let result = service.delete_node(&root_id, root.version).await.unwrap();

        assert!(result.existed);
        assert_eq!(result.deleted_count, 2);
    }

    // =========================================================================
    // BatchEmitGuard — batched event emission tests
    // =========================================================================

    /// Single `update_node` (non-bulk) must still emit immediately — no guard involved.
    #[tokio::test]
    async fn batch_emit_single_update_is_immediate() {
        let (service, _temp) = create_test_service().await;
        let mut rx = service.subscribe_to_events();

        let id = service
            .create_node(Node::new(
                "text".to_string(),
                "hello".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        // Drain create event
        let _ = rx.try_recv();

        let update = crate::models::NodeUpdate::new().with_content("updated".to_string());
        service.update_node_unchecked(&id, update).await.unwrap();

        // Event must arrive immediately (no guard active)
        assert!(
            rx.try_recv().is_ok(),
            "single update_node must emit an event immediately"
        );
    }

    /// `bulk_update` via `BatchEmitGuard` must deliver exactly one event per node,
    /// not one per write.
    #[tokio::test]
    async fn batch_emit_bulk_update_coalesces_per_node() {
        let (service, _temp) = create_test_service().await;

        // Create two nodes
        let id1 = service
            .create_node(Node::new(
                "text".to_string(),
                "node 1".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let id2 = service
            .create_node(Node::new(
                "text".to_string(),
                "node 2".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        // Subscribe after creates so we only see update events
        let mut rx = service.subscribe_to_events();

        let updates = vec![
            (
                id1.clone(),
                crate::models::NodeUpdate::new().with_content("updated 1".to_string()),
            ),
            (
                id2.clone(),
                crate::models::NodeUpdate::new().with_content("updated 2".to_string()),
            ),
        ];

        service.bulk_update(updates).await.unwrap();

        // Collect all events that arrived
        let mut events = Vec::new();
        while let Ok(env) = rx.try_recv() {
            events.push(env);
        }

        // Exactly two events — one per node, not one per write
        assert_eq!(
            events.len(),
            2,
            "bulk_update must emit exactly one event per updated node, got {}",
            events.len()
        );

        let node_ids: Vec<_> = events
            .iter()
            .map(|e| match &e.event {
                DomainEvent::NodeUpdated { node_id, .. } => node_id.clone(),
                other => panic!("unexpected event: {:?}", other),
            })
            .collect();

        assert!(node_ids.contains(&id1));
        assert!(node_ids.contains(&id2));
    }

    /// Guard drops → batch mode resets to Immediate; subsequent single writes broadcast
    /// without buffering.
    #[tokio::test]
    async fn batch_emit_guard_drop_restores_immediate_mode() {
        let (service, _temp) = create_test_service().await;

        let id = service
            .create_node(Node::new("text".to_string(), "node".to_string(), json!({})))
            .await
            .unwrap();

        // Activate and immediately drop the guard
        {
            let _guard = service.begin_batch_emit();
        }

        // Subscribe after guard is dropped
        let mut rx = service.subscribe_to_events();

        let update = crate::models::NodeUpdate::new().with_content("after guard".to_string());
        service.update_node_unchecked(&id, update).await.unwrap();

        assert!(
            rx.try_recv().is_ok(),
            "after guard drop, updates must emit immediately again"
        );
    }

    /// Events buffered while the guard is live must NOT be visible to subscribers
    /// until the guard drops.
    #[tokio::test]
    async fn batch_emit_events_not_visible_until_flush() {
        let (service, _temp) = create_test_service().await;

        let id = service
            .create_node(Node::new("text".to_string(), "node".to_string(), json!({})))
            .await
            .unwrap();

        let mut rx = service.subscribe_to_events();

        let guard = service.begin_batch_emit();

        let update = crate::models::NodeUpdate::new().with_content("buffered".to_string());
        service.update_node_unchecked(&id, update).await.unwrap();

        // No event visible yet
        assert!(
            rx.try_recv().is_err(),
            "event must not be visible before guard drops"
        );

        // Drop guard → flush
        drop(guard);

        assert!(
            rx.try_recv().is_ok(),
            "event must be visible after guard drops"
        );
    }

    // =========================================================================
    // Origin-filtered push channel (subscribe_for_push / set_push_excluded_origin)
    // =========================================================================

    /// Drain a receiver and return the ids of every node-keyed event seen.
    fn drain_node_ids(
        rx: &mut broadcast::Receiver<crate::db::events::EventEnvelope>,
    ) -> Vec<String> {
        let mut ids = Vec::new();
        while let Ok(env) = rx.try_recv() {
            match &env.event {
                DomainEvent::NodeCreated { node_id, .. } => ids.push(node_id.clone()),
                DomainEvent::NodeUpdated { node_id, .. } => ids.push(node_id.clone()),
                DomainEvent::NodeDeleted { id, .. } => ids.push(id.clone()),
                _ => {}
            }
        }
        ids
    }

    /// With no excluded origin configured, the push channel mirrors the main
    /// channel exactly — every event reaches both, regardless of origin tag.
    #[tokio::test]
    async fn push_channel_mirrors_main_when_no_origin_excluded() {
        let (service, _temp) = create_test_service().await;
        let mut ui_rx = service.subscribe_to_events();
        let mut push_rx = service.subscribe_for_push();

        let tagged = service.with_client("sync-service");
        let sync_id = tagged
            .create_node(Node::new(
                "text".to_string(),
                "from sync".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let local_id = service
            .create_node(Node::new(
                "text".to_string(),
                "local".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        let ui_ids = drain_node_ids(&mut ui_rx);
        let push_ids = drain_node_ids(&mut push_rx);

        assert!(ui_ids.contains(&sync_id) && ui_ids.contains(&local_id));
        assert!(
            push_ids.contains(&sync_id) && push_ids.contains(&local_id),
            "with no excluded origin, push channel must see every event"
        );
    }

    /// Node events (store-notifier path): once an origin is excluded, its
    /// writes reach the UI channel but not the push channel, while normal
    /// writes reach both.
    #[tokio::test]
    async fn push_channel_excludes_configured_origin_for_node_events() {
        let (mut service, _temp) = create_test_service().await;
        service.set_push_excluded_origin("sync-service");

        let mut ui_rx = service.subscribe_to_events();
        let mut push_rx = service.subscribe_for_push();

        let sync_id = service
            .with_client("sync-service")
            .create_node(Node::new(
                "text".to_string(),
                "from sync".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let local_id = service
            .create_node(Node::new(
                "text".to_string(),
                "local".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        let ui_ids = drain_node_ids(&mut ui_rx);
        let push_ids = drain_node_ids(&mut push_rx);

        assert!(
            ui_ids.contains(&sync_id) && ui_ids.contains(&local_id),
            "UI channel must observe both the excluded-origin and the normal write"
        );
        assert!(
            push_ids.contains(&local_id),
            "push channel must observe the normal write"
        );
        assert!(
            !push_ids.contains(&sync_id),
            "push channel must NOT observe the excluded-origin write"
        );
    }

    /// Relationship events (emit_event path) tagged with the excluded origin are
    /// likewise withheld from the push channel but still delivered to the UI.
    #[tokio::test]
    async fn push_channel_excludes_configured_origin_for_relationship_events() {
        let (mut service, _temp) = create_test_service().await;
        service.set_push_excluded_origin("sync-service");

        let sync = service.with_client("sync-service");
        let root_id = sync
            .create_node(Node::new("text".to_string(), "root".to_string(), json!({})))
            .await
            .unwrap();
        let child_id = sync
            .create_node(Node::new(
                "text".to_string(),
                "child".to_string(),
                json!({}),
            ))
            .await
            .unwrap();

        // Subscribe after node creation so only the relationship event is captured.
        let mut ui_rx = service.subscribe_to_events();
        let mut push_rx = service.subscribe_for_push();

        // create_parent_edge emits a RelationshipCreated via emit_event, tagged
        // with the sync-service origin.
        sync.create_parent_edge(&child_id, &root_id, InsertPosition::End)
            .await
            .unwrap();

        let count_rel = |rx: &mut broadcast::Receiver<crate::db::events::EventEnvelope>| {
            let mut n = 0;
            while let Ok(env) = rx.try_recv() {
                if matches!(env.event, DomainEvent::RelationshipCreated { .. }) {
                    n += 1;
                }
            }
            n
        };

        assert!(
            count_rel(&mut ui_rx) >= 1,
            "UI channel must observe the excluded-origin relationship event"
        );
        assert_eq!(
            count_rel(&mut push_rx),
            0,
            "push channel must NOT observe the excluded-origin relationship event"
        );
    }

    /// Batched flush (BatchEmitGuard::drop) honours the origin filter: an
    /// excluded-origin bulk flush reaches the UI channel but not the push
    /// channel.
    #[tokio::test]
    async fn push_channel_excludes_configured_origin_for_batched_flush() {
        let (mut service, _temp) = create_test_service().await;
        service.set_push_excluded_origin("sync-service");

        let sync = service.with_client("sync-service");
        let id = sync
            .create_node(Node::new("text".to_string(), "node".to_string(), json!({})))
            .await
            .unwrap();

        let mut ui_rx = service.subscribe_to_events();
        let mut push_rx = service.subscribe_for_push();

        {
            let _guard = sync.begin_batch_emit();
            let update = crate::models::NodeUpdate::new().with_content("batched".to_string());
            sync.update_node_unchecked(&id, update).await.unwrap();
            // Nothing flushes until the guard drops here.
        }

        let ui_ids = drain_node_ids(&mut ui_rx);
        let push_ids = drain_node_ids(&mut push_rx);

        assert!(
            ui_ids.contains(&id),
            "UI channel must observe the batched excluded-origin flush"
        );
        assert!(
            !push_ids.contains(&id),
            "push channel must NOT observe the batched excluded-origin flush"
        );
    }

    /// `bulk_create_hierarchy_trusted` must emit exactly one Created event per inserted
    /// node with no duplicates, delivered in a single flush rather than one-at-a-time.
    /// The batch guard coalesces last-write-wins per node_id on drop.
    #[tokio::test]
    async fn bulk_create_hierarchy_trusted_coalesces_events_per_root() {
        let (service, _temp) = create_test_service().await;

        // Subscribe before the import so we capture all Created events.
        let mut rx = service.subscribe_to_events();

        // Build a two-level hierarchy: one root + two children.
        let root_id = uuid::Uuid::new_v4().to_string();
        let child1_id = uuid::Uuid::new_v4().to_string();
        let child2_id = uuid::Uuid::new_v4().to_string();

        let nodes = vec![
            (
                root_id.clone(),
                "text".to_string(),
                "root node".to_string(),
                None,
                1.0,
                serde_json::json!({}),
            ),
            (
                child1_id.clone(),
                "text".to_string(),
                "child one".to_string(),
                Some(root_id.clone()),
                1.0,
                serde_json::json!({}),
            ),
            (
                child2_id.clone(),
                "text".to_string(),
                "child two".to_string(),
                Some(root_id.clone()),
                2.0,
                serde_json::json!({}),
            ),
        ];

        service.bulk_create_hierarchy_trusted(nodes).await.unwrap();

        // Drain all events that arrived after the call.
        let mut events = Vec::new();
        while let Ok(env) = rx.try_recv() {
            events.push(env);
        }

        // One event per node, no duplicates — all arrive in a single flush.
        assert_eq!(
            events.len(),
            3,
            "expected exactly one Created event per inserted node, got {}",
            events.len()
        );

        let node_ids: Vec<_> = events
            .iter()
            .map(|e| match &e.event {
                DomainEvent::NodeCreated { node_id, .. } => node_id.clone(),
                other => panic!("unexpected event type: {:?}", other),
            })
            .collect();

        assert!(node_ids.contains(&root_id), "root event missing");
        assert!(node_ids.contains(&child1_id), "child1 event missing");
        assert!(node_ids.contains(&child2_id), "child2 event missing");
    }

    /// The batch importer assigns collection membership via
    /// `bulk_add_to_collections_notify`, and each newly created `member_of` edge
    /// MUST emit a RelationshipCreated event so the cloud-sync push replicates it.
    /// The raw store insert emits nothing — that gap is why imported collections
    /// showed up populated only on the importing device and empty on a fresh pull.
    #[tokio::test]
    async fn bulk_add_to_collections_notify_emits_push_events() {
        let (service, _temp) = create_test_service().await;

        // A stand-in collection plus two content nodes. The store bulk-insert does
        // not type-check the target, so plain nodes suffice to exercise emission.
        let coll = service
            .create_node(Node::new(
                "text".to_string(),
                "Docs collection".to_string(),
                json!({}),
            ))
            .await
            .unwrap();
        let a = service
            .create_node(Node::new("text".to_string(), "A".to_string(), json!({})))
            .await
            .unwrap();
        let b = service
            .create_node(Node::new("text".to_string(), "B".to_string(), json!({})))
            .await
            .unwrap();

        // Subscribe after node creation so only the membership events are captured.
        let mut rx = service.subscribe_to_events();

        let created = service
            .bulk_add_to_collections_notify(&[(a.clone(), coll.clone()), (b.clone(), coll.clone())])
            .await
            .unwrap();
        assert_eq!(created, 2, "two new memberships created");

        let mut member_of_events = 0;
        while let Ok(env) = rx.try_recv() {
            if let DomainEvent::RelationshipCreated { relationship } = &env.event {
                if relationship.relationship_type == "member_of" {
                    member_of_events += 1;
                }
            }
        }
        assert_eq!(
            member_of_events, 2,
            "each new member_of edge must emit a RelationshipCreated event so it pushes to cloud",
        );

        // Idempotent re-assert: an already-present edge creates nothing and, so,
        // emits nothing (no spurious re-push).
        let mut rx2 = service.subscribe_to_events();
        let again = service
            .bulk_add_to_collections_notify(&[(a.clone(), coll.clone())])
            .await
            .unwrap();
        assert_eq!(again, 0, "existing membership is not recreated");
        assert!(
            rx2.try_recv().is_err(),
            "no event fires for an already-present edge",
        );
    }

    /// A bulk import's membership burst must not overflow the domain-event
    /// broadcast and drop edges — the exact failure that leaves a collection empty
    /// on other devices. Emit a burst LARGER than the old 128 capacity and assert a
    /// subscriber receives every event (no `Lagged`). This guards the channel
    /// capacity: at 128 this burst would drop events; at 4096 it does not.
    #[tokio::test]
    async fn bulk_membership_burst_does_not_overflow_the_event_channel() {
        use tokio::sync::broadcast::error::TryRecvError;
        let (service, _temp) = create_test_service().await;

        // One collection plus a burst of content nodes, all created BEFORE the
        // subscription so only the membership events land in the receiver's buffer.
        let coll = service
            .create_node(Node::new("text".to_string(), "coll".to_string(), json!({})))
            .await
            .unwrap();
        const BURST: usize = 300; // > 128 (old cap), < 4096 (new cap)
        let mut memberships = Vec::with_capacity(BURST);
        for i in 0..BURST {
            let id = service
                .create_node(Node::new("text".to_string(), format!("n{i}"), json!({})))
                .await
                .unwrap();
            memberships.push((id, coll.clone()));
        }

        // Subscribe AFTER node creation → the receiver's buffer starts empty and
        // only accumulates the membership burst.
        let mut rx = service.subscribe_to_events();
        let created = service
            .bulk_add_to_collections_notify(&memberships)
            .await
            .unwrap();
        assert_eq!(created, BURST, "all memberships created");

        let mut member_of_events = 0usize;
        loop {
            match rx.try_recv() {
                Ok(env) => {
                    if let DomainEvent::RelationshipCreated { relationship } = &env.event {
                        if relationship.relationship_type == "member_of" {
                            member_of_events += 1;
                        }
                    }
                }
                Err(TryRecvError::Empty | TryRecvError::Closed) => break,
                Err(TryRecvError::Lagged(n)) => {
                    panic!("event channel lagged by {n} — capacity too small for the burst")
                }
            }
        }
        assert_eq!(
            member_of_events, BURST,
            "every membership event was buffered — none dropped by a too-small channel",
        );
    }

    /// Single-node creation via `create_node` must still emit immediately (not
    /// accidentally held back by a batch guard from a prior bulk import).
    #[tokio::test]
    async fn single_create_after_trusted_import_emits_immediately() {
        let (service, _temp) = create_test_service().await;

        // Run a bulk import first.
        let root_id = uuid::Uuid::new_v4().to_string();
        service
            .bulk_create_hierarchy_trusted(vec![(
                root_id.clone(),
                "text".to_string(),
                "imported".to_string(),
                None,
                1.0,
                serde_json::json!({}),
            )])
            .await
            .unwrap();

        // Subscribe after the import; batch guard should be dropped by now.
        let mut rx = service.subscribe_to_events();

        // A fresh single create must emit without batching.
        service
            .create_node(Node::new(
                "text".to_string(),
                "standalone".to_string(),
                serde_json::json!({}),
            ))
            .await
            .unwrap();

        assert!(
            rx.try_recv().is_ok(),
            "single create after bulk import must emit immediately"
        );
    }

    // --- DatabaseSettingsNode seeding tests (ADR-037) ---

    #[tokio::test]
    async fn test_seed_database_settings_singleton_with_owner_edge() {
        let (service, _temp) = create_test_service().await;

        // Exactly one database-settings node, with the reserved id and defaults.
        let settings = service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap();
        assert_eq!(
            settings.len(),
            1,
            "a fresh install must seed exactly one DatabaseSettingsNode"
        );
        let node = &settings[0];
        assert_eq!(node.id, DATABASE_SETTINGS_NODE_ID);
        assert_eq!(node.properties["database-settings"]["sync_enabled"], false);
        assert_eq!(node.properties["database-settings"]["auth_status"], "local");

        // Exactly one local person, and exactly one has_role owner edge to the singleton.
        let people = service.query_nodes_by_type("person", None).await.unwrap();
        assert_eq!(people.len(), 1);
        let person_id = people[0].id.clone();

        let targets = service
            .get_related_nodes(&person_id, "has_role", "out")
            .await
            .unwrap();
        assert_eq!(targets.len(), 1, "exactly one has_role edge must be seeded");
        assert_eq!(targets[0].id, DATABASE_SETTINGS_NODE_ID);

        let edge = service
            .store()
            .get_relationship_record(&person_id, DATABASE_SETTINGS_NODE_ID, "has_role")
            .await
            .unwrap()
            .expect("owner has_role edge exists");
        assert_eq!(edge.properties["role"], "owner");
        assert_eq!(edge.properties["status"], "active");
    }

    /// Relationship viewer aggregation (issue #1918): an outbound relationship
    /// (with edge_fields + edge properties) and its inbound reverse must both
    /// surface, carrying the edge data, from opposite ends of the SAME edge.
    #[tokio::test]
    async fn test_get_node_relationships_outbound_inbound_with_edges() {
        let (service, _temp) = create_test_service().await;
        let service = std::sync::Arc::new(service);
        let store = service.store();

        // Target schema (no relationships).
        store
            .create_node(
                Node::new_with_id(
                    "widget".to_string(),
                    "schema".to_string(),
                    "Widget".to_string(),
                    serde_json::json!({ "fields": [], "relationships": [] }),
                ),
                None,
                None,
            )
            .await
            .unwrap();

        // Source schema: gadget --assigned_to--> widget, with a `role` edge field.
        store
            .create_node(
                Node::new_with_id(
                    "gadget".to_string(),
                    "schema".to_string(),
                    "Gadget".to_string(),
                    serde_json::json!({
                        "fields": [],
                        "relationships": [{
                            "name": "assigned_to",
                            "targetType": "widget",
                            "direction": "out",
                            "cardinality": "many",
                            "reverseName": "gadgets",
                            "reverseCardinality": "many",
                            "edgeFields": [{ "name": "role", "type": "string" }]
                        }]
                    }),
                ),
                None,
                None,
            )
            .await
            .unwrap();

        // Instances + a single edge carrying `role`.
        store
            .create_node(
                Node::new_with_id(
                    "g1".to_string(),
                    "gadget".to_string(),
                    "Gadget One".to_string(),
                    serde_json::json!({}),
                ),
                None,
                None,
            )
            .await
            .unwrap();
        store
            .create_node(
                Node::new_with_id(
                    "w1".to_string(),
                    "widget".to_string(),
                    "Widget One".to_string(),
                    serde_json::json!({}),
                ),
                None,
                None,
            )
            .await
            .unwrap();
        store
            .create_generic_relationship("g1", "w1", "assigned_to", &serde_json::json!({"role":"lead"}))
            .await
            .unwrap();

        // Outbound (from the source): assigned_to → widget, edge role=lead.
        let out = crate::ops::rel_ops::get_node_relationships(&service, "g1")
            .await
            .unwrap();
        let group = out
            .groups
            .iter()
            .find(|g| g.relationship_name == "assigned_to" && g.direction == "out")
            .expect("outbound assigned_to group present");
        assert_eq!(group.target_type.as_deref(), Some("widget"));
        assert_eq!(group.count, 1);
        assert_eq!(group.related[0].id, "w1");
        assert_eq!(group.related[0].edge_properties["role"], "lead");
        assert!(group.edge_fields.is_some(), "edge_fields carried through");

        // Inbound (from the target): the SAME edge, labeled by reverse_name.
        let inb = crate::ops::rel_ops::get_node_relationships(&service, "w1")
            .await
            .unwrap();
        let group = inb
            .groups
            .iter()
            .find(|g| g.relationship_name == "assigned_to" && g.direction == "in")
            .expect("inbound assigned_to group present");
        assert_eq!(group.reverse_name.as_deref(), Some("gadgets"));
        assert_eq!(group.source_type, "gadget");
        assert_eq!(group.target_type.as_deref(), Some("gadget"));
        assert_eq!(group.count, 1);
        assert_eq!(group.related[0].id, "g1");
        assert_eq!(group.related[0].edge_properties["role"], "lead");

        // Built-in structural relationships are excluded from both views.
        assert!(
            out.groups
                .iter()
                .all(|g| g.relationship_name != "has_child" && g.relationship_name != "mentions"),
            "built-in relationships must not appear"
        );
    }

    #[tokio::test]
    async fn test_get_node_relationships_inbound_multiple_sources_not_duplicated() {
        // Regression (#1918): the inbound query keys only on relationship_type,
        // so two schemas declaring the SAME relationship name targeting the same
        // type must land in SEPARATE groups, each restricted to its own source
        // type — never doubled or cross-attributed.
        let (service, _temp) = create_test_service().await;
        let service = std::sync::Arc::new(service);
        let store = service.store();

        store
            .create_node(
                Node::new_with_id(
                    "widget".to_string(),
                    "schema".to_string(),
                    "Widget".to_string(),
                    serde_json::json!({ "fields": [], "relationships": [] }),
                ),
                None,
                None,
            )
            .await
            .unwrap();

        // Two source schemas, both declaring assigned_to -> widget.
        for (id, name, reverse) in [
            ("gadget", "Gadget", "gadgets"),
            ("sprocket", "Sprocket", "sprockets"),
        ] {
            store
                .create_node(
                    Node::new_with_id(
                        id.to_string(),
                        "schema".to_string(),
                        name.to_string(),
                        serde_json::json!({
                            "fields": [],
                            "relationships": [{
                                "name": "assigned_to",
                                "targetType": "widget",
                                "direction": "out",
                                "cardinality": "many",
                                "reverseName": reverse,
                                "reverseCardinality": "many"
                            }]
                        }),
                    ),
                    None,
                    None,
                )
                .await
                .unwrap();
        }

        for (id, ty) in [("g1", "gadget"), ("s1", "sprocket"), ("w1", "widget")] {
            store
                .create_node(
                    Node::new_with_id(
                        id.to_string(),
                        ty.to_string(),
                        id.to_uppercase(),
                        serde_json::json!({}),
                    ),
                    None,
                    None,
                )
                .await
                .unwrap();
        }
        store
            .create_generic_relationship("g1", "w1", "assigned_to", &serde_json::json!({}))
            .await
            .unwrap();
        store
            .create_generic_relationship("s1", "w1", "assigned_to", &serde_json::json!({}))
            .await
            .unwrap();

        let inb = crate::ops::rel_ops::get_node_relationships(&service, "w1")
            .await
            .unwrap();
        let inbound: Vec<_> = inb.groups.iter().filter(|g| g.direction == "in").collect();

        assert_eq!(inbound.len(), 2, "one inbound group per declaring source type");
        let gadget = inbound
            .iter()
            .find(|g| g.source_type == "gadget")
            .expect("gadget inbound group");
        assert_eq!(gadget.count, 1);
        assert_eq!(gadget.related[0].id, "g1");
        let sprocket = inbound
            .iter()
            .find(|g| g.source_type == "sprocket")
            .expect("sprocket inbound group");
        assert_eq!(sprocket.count, 1);
        assert_eq!(sprocket.related[0].id, "s1");
    }

    #[tokio::test]
    async fn test_get_set_bound_tenant_roundtrip() {
        let (service, _temp) = create_test_service().await;
        const COLL: &str = "c0000000-0000-0000-0000-000000000001";

        // Fresh install: unbound.
        assert_eq!(service.get_bound_tenant().await.unwrap(), None);

        // Bind → get returns exactly what was set.
        service.set_bound_tenant("tenant_demo", COLL).await.unwrap();
        assert_eq!(
            service.get_bound_tenant().await.unwrap(),
            Some(("tenant_demo".to_string(), COLL.to_string()))
        );

        // Sibling fields in the `database-settings` namespace are preserved (merge,
        // not overwrite).
        let node = &service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap()[0];
        assert_eq!(node.properties["database-settings"]["sync_enabled"], false);
        assert_eq!(node.properties["database-settings"]["auth_status"], "local");

        // Re-bind overwrites.
        service
            .set_bound_tenant("tenant_other", "c9999999-0000-0000-0000-000000000009")
            .await
            .unwrap();
        assert_eq!(
            service.get_bound_tenant().await.unwrap(),
            Some((
                "tenant_other".to_string(),
                "c9999999-0000-0000-0000-000000000009".to_string()
            ))
        );

        // Empty schema or collection is treated as unbound.
        service.set_bound_tenant("", "").await.unwrap();
        assert_eq!(service.get_bound_tenant().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_set_sync_enabled_and_auth_status_roundtrip() {
        let (service, _temp) = create_test_service().await;
        const COLL: &str = "c0000000-0000-0000-0000-000000000001";

        // Fresh install defaults, read through the accessors the Pro daemon uses.
        let node = &service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap()[0];
        assert_eq!(node.properties["database-settings"]["sync_enabled"], false);
        assert_eq!(node.properties["database-settings"]["auth_status"], "local");
        assert!(!service.get_sync_enabled().await.unwrap());
        assert_eq!(service.get_auth_status().await.unwrap(), "local");

        // Enabling sync flips only sync_enabled; auth_status is preserved.
        service.set_sync_enabled(true).await.unwrap();
        let node = &service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap()[0];
        assert_eq!(node.properties["database-settings"]["sync_enabled"], true);
        assert_eq!(node.properties["database-settings"]["auth_status"], "local");
        assert!(service.get_sync_enabled().await.unwrap());
        assert_eq!(service.get_auth_status().await.unwrap(), "local");

        // Advancing auth_status preserves sync_enabled.
        service.set_auth_status("connected").await.unwrap();
        let node = &service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap()[0];
        assert_eq!(
            node.properties["database-settings"]["auth_status"],
            "connected"
        );
        assert_eq!(node.properties["database-settings"]["sync_enabled"], true);
        assert!(service.get_sync_enabled().await.unwrap());
        assert_eq!(service.get_auth_status().await.unwrap(), "connected");

        // A later tenant binding does not clobber the sync state (cross-field merge).
        service
            .set_bound_tenant("tenant_public", COLL)
            .await
            .unwrap();
        let node = &service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap()[0];
        assert_eq!(node.properties["database-settings"]["sync_enabled"], true);
        assert_eq!(
            node.properties["database-settings"]["auth_status"],
            "connected"
        );

        // And enabling sync again preserves the tenant binding.
        service.set_sync_enabled(true).await.unwrap();
        assert_eq!(
            service.get_bound_tenant().await.unwrap(),
            Some(("tenant_public".to_string(), COLL.to_string()))
        );
    }

    #[tokio::test]
    async fn test_create_second_database_settings_is_noop() {
        let (service, _temp) = create_test_service().await;

        // A second database-settings create is idempotent: returns the existing
        // singleton id and does not create a duplicate.
        let duplicate = Node::new(
            "database-settings".to_string(),
            String::new(),
            json!({"sync_enabled": true, "auth_status": "connected"}),
        );
        let returned_id = service.create_node(duplicate).await.unwrap();
        assert_eq!(returned_id, DATABASE_SETTINGS_NODE_ID);

        let settings = service
            .query_nodes_by_type("database-settings", None)
            .await
            .unwrap();
        assert_eq!(
            settings.len(),
            1,
            "second database-settings create must be a no-op"
        );
        // The original singleton is untouched (still holds seeded defaults).
        assert_eq!(
            settings[0].properties["database-settings"]["auth_status"],
            "local"
        );
    }

    #[tokio::test]
    async fn test_reopening_database_does_not_reseed_database_settings() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        // First open seeds the singleton + owner edge.
        {
            let mut store = Arc::new(SqliteStore::new(db_path.clone()).await.unwrap());
            let service = NodeService::new(&mut store).await.unwrap();
            assert_eq!(
                service
                    .query_nodes_by_type("database-settings", None)
                    .await
                    .unwrap()
                    .len(),
                1
            );
        }

        // Re-opening the same database must be idempotent — still exactly one.
        let mut store = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let service = NodeService::new(&mut store).await.unwrap();
        assert_eq!(
            service
                .query_nodes_by_type("database-settings", None)
                .await
                .unwrap()
                .len(),
            1,
            "re-opening an existing database must not seed a second DatabaseSettingsNode"
        );

        let people = service.query_nodes_by_type("person", None).await.unwrap();
        let edges = service
            .get_related_nodes(&people[0].id, "has_role", "out")
            .await
            .unwrap();
        assert_eq!(
            edges.len(),
            1,
            "owner edge must not be duplicated on reopen"
        );
    }

    fn seed_template(
        title: &str,
        markdown: &str,
        tier: crate::markdown::SeedTier,
    ) -> crate::markdown::NodeTemplate {
        crate::markdown::NodeTemplate {
            title: title.to_string(),
            content: None,
            root_node_type: "agent-guidance".to_string(),
            root_properties: json!({}),
            child_node_type: Some("text".to_string()),
            child_properties: None,
            tier,
            markdown_content: markdown.to_string(),
        }
    }

    #[tokio::test]
    async fn reseed_replaces_system_tier_node_on_content_change() {
        use crate::markdown::{prepare_nodes_from_template, SeedTier};

        let (service, _temp) = create_test_service().await;

        let v1 = seed_template("Core Identity", "You are v1.", SeedTier::System);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v1).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].content, "Core Identity");

        // Re-seed with changed content under the same seed_key ("Core Identity").
        let v2 = seed_template("Core Identity", "You are v2, rewritten.", SeedTier::System);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v2).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "replace must not leave the stale node behind"
        );
        let children = service.get_children(&nodes[0].id).await.unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].content, "You are v2, rewritten.");
    }

    #[tokio::test]
    async fn reseed_updates_unmodified_starter_tier_node() {
        use crate::markdown::{prepare_nodes_from_template, SeedTier};

        let (service, _temp) = create_test_service().await;

        let v1 = seed_template("Welcome Note", "Starter v1.", SeedTier::Starter);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v1).unwrap()])
            .await
            .unwrap();

        let v2 = seed_template("Welcome Note", "Starter v2.", SeedTier::Starter);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v2).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        assert_eq!(nodes.len(), 1);
        let children = service.get_children(&nodes[0].id).await.unwrap();
        assert_eq!(
            children[0].content, "Starter v2.",
            "unedited starter-tier node must be updated on reseed"
        );
    }

    #[tokio::test]
    async fn reseed_skips_user_modified_starter_tier_node() {
        use crate::markdown::{prepare_nodes_from_template, SeedTier};

        let (service, _temp) = create_test_service().await;

        let v1 = seed_template("Welcome Note", "Starter v1.", SeedTier::Starter);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v1).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        let root = &nodes[0];

        // Simulate the user editing the seeded node through the normal update path.
        service
            .update_node(
                &root.id,
                root.version,
                NodeUpdate::new().with_content("User's own title".to_string()),
            )
            .await
            .unwrap();

        // Re-seed with changed content under the same seed_key.
        let v2 = seed_template("Welcome Note", "Starter v2.", SeedTier::Starter);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&v2).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "user-modified starter node must survive reseed, not be duplicated"
        );
        assert_eq!(
            nodes[0].content, "User's own title",
            "user-modified starter node must not be overwritten by reseed"
        );
    }

    #[tokio::test]
    async fn reseed_is_noop_when_hash_unchanged() {
        use crate::markdown::{prepare_nodes_from_template, SeedTier};

        let (service, _temp) = create_test_service().await;

        let tmpl = seed_template("Core Identity", "Stable content.", SeedTier::System);
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&tmpl).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        let version_before = nodes[0].version;

        // Re-seed with the exact same template (same content hash) — must be a no-op,
        // not a delete+recreate, so the node's identity/version is untouched.
        service
            .seed_nodes_from_templates(vec![prepare_nodes_from_template(&tmpl).unwrap()])
            .await
            .unwrap();

        let nodes = service
            .query_nodes_by_type("agent-guidance", None)
            .await
            .unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(
            nodes[0].version, version_before,
            "unchanged seed content must not touch the existing node"
        );
    }

    /// `prompt` (the type every other reseed test uses) is a core schema with
    /// zero fields, so its properties never round-trip through
    /// `normalize_flat_properties_to_namespace`'s flat-property-hoisting branch
    /// with anything worth namespacing. `skill` has real required fields
    /// (`description`, `tool_whitelist`) and hits that hoisting path on every
    /// create. This proves `_seed` still survives — landing at
    /// `properties._seed`, not buried under `properties.skill._seed` — for a
    /// type that actually exercises the mechanism the whole fix depends on.
    #[tokio::test]
    async fn reseed_replaces_skill_tier_node_with_real_schema_fields() {
        use crate::markdown::{prepare_nodes_from_template, NodeTemplate, SeedTier};

        let (service, _temp) = create_test_service().await;

        let skill_tmpl = |description: &str| NodeTemplate {
            title: "Research & Search".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: json!({
                "description": description,
                "tool_whitelist": ["search_semantic"],
            }),
            child_node_type: Some("text".to_string()),
            child_properties: None,
            tier: SeedTier::System,
            markdown_content: "Guidance v1.".to_string(),
        };

        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&skill_tmpl("Search v1")).unwrap()
            ])
            .await
            .unwrap();

        let nodes = service.query_nodes_by_type("skill", None).await.unwrap();
        assert_eq!(nodes.len(), 1);
        // Real schema fields land namespaced under properties.skill.* — confirms
        // this node actually went through the hoisting path this test targets.
        assert_eq!(
            nodes[0].properties["skill"]["description"], "Search v1",
            "schema field must be namespaced under properties.skill"
        );
        // _seed must NOT be nested under properties.skill — it must survive at
        // the top level, or the by-key lookup on reseed would never find it.
        assert!(
            nodes[0].properties.get("_seed").is_some(),
            "_seed must land at the top level even when the node_type has real schema fields"
        );

        // Re-seed with changed description under the same seed_key — must replace,
        // not duplicate, exactly like the empty-schema `agent-guidance` case.
        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&skill_tmpl("Search v2")).unwrap()
            ])
            .await
            .unwrap();

        let nodes = service.query_nodes_by_type("skill", None).await.unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "replace must not leave the stale skill node behind"
        );
        assert_eq!(nodes[0].properties["skill"]["description"], "Search v2");
    }

    /// `tool` seed templates pre-namespace their own properties under a `"tool"`
    /// key (see `skill_pipeline.rs::seed_tool_nodes`) for an unrelated reason —
    /// `tool` isn't a core schema, so `normalize_flat_properties_to_namespace`
    /// wouldn't hoist it on its own; the pre-namespacing exists to protect the
    /// nested `parameter_schema` object from being misclassified. This proves
    /// `_seed` (stamped alongside that pre-existing `"tool"` namespace) doesn't
    /// collide with it and both survive independently.
    #[tokio::test]
    async fn reseed_replaces_tool_tier_node_with_pre_namespaced_properties() {
        use crate::markdown::{prepare_nodes_from_template, NodeTemplate, SeedTier};

        let (service, _temp) = create_test_service().await;

        let tool_tmpl = |description: &str| NodeTemplate {
            title: "search_nodes".to_string(),
            content: None,
            root_node_type: "tool".to_string(),
            root_properties: json!({
                "tool": {
                    "handler": "search_nodes",
                    "description": description,
                    "parameter_schema": {"type": "object"},
                    "source": "internal",
                    "enabled": true,
                }
            }),
            child_node_type: None,
            child_properties: None,
            tier: SeedTier::System,
            markdown_content: String::new(),
        };

        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&tool_tmpl("Search v1")).unwrap()
            ])
            .await
            .unwrap();

        let nodes = service.query_nodes_by_type("tool", None).await.unwrap();
        assert_eq!(nodes.len(), 1);
        assert_eq!(nodes[0].properties["tool"]["description"], "Search v1");
        assert!(
            nodes[0].properties.get("_seed").is_some(),
            "_seed must coexist with the pre-namespaced 'tool' key, not be swallowed by it"
        );

        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&tool_tmpl("Search v2")).unwrap()
            ])
            .await
            .unwrap();

        let nodes = service.query_nodes_by_type("tool", None).await.unwrap();
        assert_eq!(
            nodes.len(),
            1,
            "replace must not leave the stale tool node behind"
        );
        assert_eq!(nodes[0].properties["tool"]["description"], "Search v2");
    }

    /// A single `seed_nodes_from_templates` call, as the daemon issues it, mixes
    /// `agent-guidance` + `skill` + `tool` template groups together. Reconciliation
    /// looks up existing nodes per-`node_type`, keyed by `_seed.key` — this
    /// proves that grouping doesn't cross-contaminate: changing one type's
    /// content doesn't touch, skip, or duplicate a sibling type's unrelated node
    /// sharing the same batch.
    #[tokio::test]
    async fn reseed_handles_mixed_node_types_in_one_batch_independently() {
        use crate::markdown::{prepare_nodes_from_template, NodeTemplate, SeedTier};

        let (service, _temp) = create_test_service().await;

        let prompt_tmpl = seed_template("Core Identity", "Prompt v1.", SeedTier::System);
        let skill_tmpl = NodeTemplate {
            title: "Research & Search".to_string(),
            content: None,
            root_node_type: "skill".to_string(),
            root_properties: json!({
                "description": "Skill v1",
                "tool_whitelist": ["search_semantic"],
            }),
            child_node_type: Some("text".to_string()),
            child_properties: None,
            tier: SeedTier::System,
            markdown_content: "Guidance v1.".to_string(),
        };

        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&prompt_tmpl).unwrap(),
                prepare_nodes_from_template(&skill_tmpl).unwrap(),
            ])
            .await
            .unwrap();

        assert_eq!(
            service
                .query_nodes_by_type("agent-guidance", None)
                .await
                .unwrap()
                .len(),
            1, // only the Core Identity root — the skill's child is now "text"
            "expected only the Core Identity root"
        );
        assert_eq!(
            service
                .query_nodes_by_type("skill", None)
                .await
                .unwrap()
                .len(),
            1
        );

        // Only the prompt template changes this round.
        let prompt_v2 = seed_template("Core Identity", "Prompt v2.", SeedTier::System);
        service
            .seed_nodes_from_templates(vec![
                prepare_nodes_from_template(&prompt_v2).unwrap(),
                prepare_nodes_from_template(&skill_tmpl).unwrap(),
            ])
            .await
            .unwrap();

        let skills = service.query_nodes_by_type("skill", None).await.unwrap();
        assert_eq!(
            skills.len(),
            1,
            "unchanged skill template must not be duplicated by the prompt's replace"
        );
        assert_eq!(
            skills[0].properties["skill"]["description"], "Skill v1",
            "unchanged skill content must be untouched"
        );
    }
}
