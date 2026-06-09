//! NodeService — public facade and shared infrastructure.
//!
//! This file owns:
//! - The `NodeService` struct definition and `Clone` impl
//! - Construction (`new`, `seed_*`) and accessors (`store`, `behaviors`, `with_client`,
//!   `subscribe_to_events`)
//! - Hierarchy methods (Phase 4 — kept here until #1237 lands; will move to `hierarchy.rs`)
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
use std::sync::{Arc, Mutex, OnceLock};
use tokio::sync::broadcast;

// Sub-module declarations
pub(crate) mod bulk;
pub(crate) mod crud;
pub(crate) mod embedding;
pub(crate) mod query;
pub(crate) mod relationship;
pub(crate) mod schema;

/// Compute property changes between pre-mutation and post-mutation node properties (Issue #995)
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

/// Broadcast channel capacity for domain events.
///
/// 128 provides sufficient headroom for burst operations (bulk node creation)
/// while limiting memory overhead. Observer lag is acceptable - we only track
/// the current state, not historical events.
const DOMAIN_EVENT_CHANNEL_CAPACITY: usize = 128;

/// Internal state shared between the store notifier closure and `BatchEmitGuard`.
///
/// `Immediate` — every event is broadcast as it arrives (default).
/// `Batching` — events accumulate in the map; last-write-wins per node_id.
pub(crate) enum BatchState {
    Immediate,
    Batching(HashMap<String, crate::db::events::EventEnvelope>),
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
}

impl Drop for BatchEmitGuard {
    fn drop(&mut self) {
        let mut lock = self.state.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::mem::replace(&mut *lock, BatchState::Immediate);
        if let BatchState::Batching(buf) = prev {
            for envelope in buf.into_values() {
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
/// "Customer Profile" → "customer-profile") so they can be referenced
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
/// Supports both markdown format and plain URIs:
/// - Markdown: [@text](nodespace://node-id) or [text](nodespace://node-id)
/// - Plain: nodespace://node-id
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
    /// Issue #995: Changed from DomainEvent to EventEnvelope
    pub(crate) event_tx: broadcast::Sender<crate::db::events::EventEnvelope>,

    /// Shared batch state for coalescing events during bulk operations.
    ///
    /// When `BatchState::Batching`, the store notifier accumulates events instead
    /// of broadcasting immediately. `begin_batch_emit()` activates batching and
    /// returns a `BatchEmitGuard` that flushes on drop.
    pub(crate) batch_state: Arc<Mutex<BatchState>>,

    /// Optional client identifier for event source tracking (Issue #665)
    ///
    /// When set, all emitted events will include this client_id as source_client_id
    /// in the EventEnvelope metadata.
    ///
    /// Use `with_client()` to create a new NodeService instance with client_id set.
    pub(crate) client_id: Option<String>,

    /// Optional playbook execution context for cycle detection (Issue #995)
    ///
    /// When set, emitted events carry this context in EventEnvelope metadata.
    /// Use `scoped_for_playbook()` to create a scoped instance.
    pub(crate) execution_context: Option<crate::db::events::PlaybookExecutionContext>,

    /// Optional waker to trigger embedding processor (Issue #729)
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
}

impl Clone for NodeService {
    fn clone(&self) -> Self {
        Self {
            store: self.store.clone(),
            behaviors: self.behaviors.clone(),
            migration_registry: self.migration_registry.clone(),
            event_tx: self.event_tx.clone(),
            batch_state: self.batch_state.clone(),
            client_id: self.client_id.clone(),
            execution_context: self.execution_context.clone(),
            // Share the same OnceLock so any clone can observe the waker once set.
            #[cfg(feature = "nlp")]
            embedding_waker: self.embedding_waker.clone(),
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
    /// # Cache Population (Issue #704)
    ///
    /// Takes `&mut Arc<SqliteStore>` to enable cache updates during schema seeding:
    /// - On first launch: Seeds schemas and updates caches incrementally via `Arc::get_mut()`
    /// - On subsequent launches: Caches already populated by `SqliteStore::new()`
    pub async fn new(store: &mut Arc<SqliteStore>) -> Result<Self, NodeServiceError> {
        // Create empty migration registry (no migrations registered yet - pre-deployment)
        // Infrastructure exists for future schema evolution post-deployment
        let migration_registry = MigrationRegistry::new();

        // Initialize broadcast channel for domain events (Issue #995: EventEnvelope)
        let (event_tx, _) = broadcast::channel(DOMAIN_EVENT_CHANNEL_CAPACITY);

        // Shared batch state — Immediate by default; swapped to Batching during bulk ops.
        let batch_state: Arc<Mutex<BatchState>> = Arc::new(Mutex::new(BatchState::Immediate));

        // Register store-level notifier for automatic domain event emission (Issue #718)
        // This callback converts StoreChange notifications to EventEnvelopes.
        // Must be set BEFORE seed_core_schemas so schema seeding also emits events.
        //
        // Issue #724: Events now send only node_id (not full payload) for efficiency.
        // Issue #995: Events wrapped in EventEnvelope with metadata.
        // Issue #1306: Batch mode coalesces events per node during bulk operations.
        {
            let tx = event_tx.clone();
            let batch_state_ref = Arc::clone(&batch_state);
            let notifier = Arc::new(move |change: StoreChange| {
                use crate::db::events::{EventEnvelope, EventMetadata};

                // Compute changed properties for updates (Issue #995)
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

                // Wrap in EventEnvelope with metadata (Issue #995)
                let envelope = EventEnvelope {
                    event,
                    metadata: EventMetadata {
                        source_client_id: change.source,
                        playbook_context: change.playbook_context,
                    },
                };

                // Issue #1306: In batch mode, accumulate last-write-wins per node.
                // In immediate mode (default), broadcast directly.
                let mut state = batch_state_ref.lock().unwrap_or_else(|e| e.into_inner());
                match &mut *state {
                    BatchState::Immediate => {
                        let _ = tx.send(envelope);
                    }
                    BatchState::Batching(buf) => {
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

        // Seed core schemas if needed (Issue #704)
        // This must happen BEFORE we clone the Arc into Self, so we can use Arc::get_mut()
        // to update schema caches incrementally during seeding.
        Self::seed_core_schemas_if_needed(store).await?;

        let service = Self {
            store: Arc::clone(store),
            behaviors: Arc::new(NodeBehaviorRegistry::new()),
            migration_registry: Arc::new(migration_registry),
            event_tx,
            batch_state,
            client_id: None,
            execution_context: None,
            #[cfg(feature = "nlp")]
            embedding_waker: std::sync::Arc::new(std::sync::OnceLock::new()),
        };

        // Issue #1351: backfill description subtrees for schemas that still have
        // properties.description but no child nodes (databases created before this change).
        service.backfill_schema_description_subtrees().await?;

        // ADR-037 (#133): every install has exactly one local PersonNode (the user).
        service.seed_local_person_if_needed().await?;

        Ok(service)
    }

    /// ADR-037 (#133): seed exactly one **local** PersonNode — the local user,
    /// `auth_status: "local"`. Idempotent: skips when a person already exists, so an
    /// existing database is backfilled on next open too. On Pro upgrade this node is
    /// *bound* to a Supabase identity (nodespace-sync#125), not recreated — there is
    /// no "now set up your user" migration step.
    async fn seed_local_person_if_needed(&self) -> Result<(), NodeServiceError> {
        if !self.query_nodes_by_type("person", None).await?.is_empty() {
            return Ok(());
        }
        let person = Node::new(
            "person".to_string(),
            "Me".to_string(),
            serde_json::json!({ "auth_status": "local" }),
        );
        let id = self.create_node(person).await?;
        tracing::info!(node_id = %id, "🌱 Seeded local PersonNode (ADR-037)");
        Ok(())
    }

    /// Seed core schema definitions if database is fresh
    ///
    /// Checks if schema nodes exist. If not, creates all core schemas
    /// (task, text, date, header, code-block, quote-block, ordered-list).
    ///
    /// This is idempotent - safe to call multiple times.
    async fn seed_core_schemas_if_needed(
        store: &mut Arc<SqliteStore>,
    ) -> Result<(), NodeServiceError> {
        use crate::models::core_schemas::get_core_schemas;

        // Check if schemas already exist by trying to get task schema
        // If task exists, assume all core schemas are seeded
        let task_exists = store
            .get_node("task")
            .await
            .map_err(|e| {
                NodeServiceError::QueryFailed(format!("Failed to check for schemas: {}", e))
            })?
            .is_some();

        if task_exists {
            tracing::info!("✅ Core schemas already seeded");
            return Ok(());
        }

        tracing::info!("🌱 Seeding core schemas...");

        // Get core schemas from canonical source
        let core_schemas = get_core_schemas();

        // Collect schema info for cache updates (before we start creating nodes)
        let schema_cache_updates: Vec<(String, bool)> = core_schemas
            .iter()
            .map(|s| (s.id.clone(), !s.fields.is_empty()))
            .collect();

        // Universal Graph Architecture (Issue #783): Properties stored in node.properties.
        // Only relationship tables are created for relationships.
        {
            let table_manager = crate::services::schema_table_manager::SchemaTableManager::new();

            // For each schema: atomically create schema node + relationship table DDL (if any)
            for schema in &core_schemas {
                let schema_id = schema.id.clone();
                let node = schema.clone().into_node();

                // Universal Graph Architecture: Only generate relationship table DDL for relationships
                let ddl_statements = if !schema.relationships.is_empty() {
                    table_manager
                        .generate_relationship_ddl_statements(&schema_id, &schema.relationships)
                        .map_err(|e| {
                            NodeServiceError::SerializationError(format!(
                                "Failed to generate relationship DDL for '{}': {}",
                                schema_id, e
                            ))
                        })?
                } else {
                    vec![]
                };

                // Atomically create schema node + execute DDL
                store
                    .create_schema_node_atomic(node, ddl_statements, None)
                    .await
                    .map_err(|e| {
                        NodeServiceError::SerializationError(format!(
                            "Failed to create schema node '{}': {}",
                            schema_id, e
                        ))
                    })?;
            }
        } // ← Arc clone dropped here, enabling Arc::get_mut() below

        // Update schema caches incrementally (Issue #704)
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

    /// Seed node hierarchies from pre-expanded template node lists (Issue #1056).
    ///
    /// Each element of `template_groups` is a flat `Vec<PreparedNode>` produced
    /// by [`crate::markdown::prepare_nodes_from_template`].
    /// The first element of each group is the root node; subsequent elements are
    /// its children.
    ///
    /// Idempotency rule: if any node of a given `node_type` already exists in the
    /// database, the entire type is skipped.
    pub async fn seed_nodes_from_templates(
        &self,
        template_groups: Vec<Vec<crate::markdown::PreparedNode>>,
    ) -> Result<(), NodeServiceError> {
        if template_groups.is_empty() {
            return Ok(());
        }

        // Collect the root node_types we need to check for existence.
        let root_types: std::collections::HashSet<String> = template_groups
            .iter()
            .filter_map(|g| g.first())
            .map(|n| n.node_type.clone())
            .collect();

        let mut seeded_types: std::collections::HashSet<String> = std::collections::HashSet::new();
        for node_type in &root_types {
            let filter = crate::models::NodeFilter {
                node_type: Some(node_type.clone()),
                ..Default::default()
            };
            if !self.query_nodes(filter).await?.is_empty() {
                seeded_types.insert(node_type.clone());
            }
        }

        let mut created_roots = 0u32;
        let mut created_children = 0u32;
        let mut skipped = 0u32;

        for group in template_groups {
            let root = match group.first() {
                Some(r) => r,
                None => continue,
            };

            if seeded_types.contains(&root.node_type) {
                skipped += 1;
                continue;
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

        if created_roots > 0 {
            tracing::info!(
                created_roots,
                created_children,
                skipped,
                "Agent nodes seeded from templates"
            );
        }

        Ok(())
    }

    /// Backfill description child subtrees for schemas that still have `properties.description`
    /// (Issue #1351).
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

    /// Get a reference to the behavior registry (Issue #1018)
    pub fn behaviors(&self) -> &Arc<NodeBehaviorRegistry> {
        &self.behaviors
    }

    /// Resolve the behavior for a node type, falling back to CustomNodeBehavior.
    pub(crate) fn behavior_for(&self, node_type: &str) -> Arc<dyn crate::behaviors::NodeBehavior> {
        self.behaviors
            .get(node_type)
            .unwrap_or_else(|| Arc::new(crate::behaviors::CustomNodeBehavior::new(node_type)))
    }

    /// Check if a node type is embeddable according to its behavior (Issue #1018)
    ///
    /// Uses `NodeBehavior::get_embeddable_content()` on a probe node to determine
    /// if this node type can ever produce embeddable content. Types that unconditionally
    /// return `None` (task, date, collection, etc.) are not embeddable.
    ///
    /// For types that are conditionally embeddable (based on content), this creates
    /// a probe node with non-empty content. If the behavior still returns `None`,
    /// the type is never embeddable.
    fn is_embeddable_type(&self, node_type: &str) -> bool {
        let behavior: Arc<dyn crate::behaviors::NodeBehavior> = self
            .behaviors
            .get(node_type)
            .unwrap_or_else(|| Arc::new(crate::behaviors::CustomNodeBehavior::new(node_type)));
        // Probe with non-empty content to see if the behavior can ever return Some
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
    /// playbook execution context (Issue #995).
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

    /// Subscribe to domain events (Issue #995: returns EventEnvelope)
    ///
    /// Returns a broadcast receiver that receives all domain events wrapped
    /// in `EventEnvelope` with metadata (source_client_id, playbook_context).
    pub fn subscribe_to_events(&self) -> broadcast::Receiver<crate::db::events::EventEnvelope> {
        self.event_tx.subscribe()
    }

    /// Begin batched event emission for bulk operations (Issue #1306).
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
        }
    }

    /// Emit a domain event to all subscribers (Issue #995: wraps in EventEnvelope)
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
                buf.insert(id, envelope);
            }
            _ => {
                let _ = self.event_tx.send(envelope);
            }
        }
    }

    // =========================================================================
    // Hierarchy methods (Phase 4 deferred - stay in mod.rs until #1237 merges)
    // =========================================================================

    /// Get children of a node
    ///
    /// Returns all direct children of the specified parent node.
    ///
    /// # Arguments
    ///
    /// * `parent_id` - The parent node ID
    ///
    /// # Returns
    ///
    /// Vector of child nodes (empty if no children)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// let children = service.get_children("parent-id").await?;
    /// println!("Found {} children", children.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_children(&self, parent_id: &str) -> Result<Vec<Node>, NodeServiceError> {
        // Use edge-based query from SqliteStore (graph-native architecture)
        // Children are already sorted by fractional order on edges
        self.store
            .get_children(parent_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Returns all root nodes — nodes with no parent edge in the graph.
    pub async fn get_roots(
        &self,
        limit: Option<usize>,
        offset: Option<usize>,
    ) -> Result<Vec<Node>, NodeServiceError> {
        self.store
            .get_roots(limit, offset)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Get all descendants of a node (recursive children)
    ///
    /// Fetches all nodes in the subtree rooted at the specified node,
    /// excluding the root node itself. Uses iterative breadth-first traversal.
    ///
    /// # Arguments
    ///
    /// * `root_id` - The root node ID to fetch descendants for
    ///
    /// # Returns
    ///
    /// `Vec<Node>` containing all descendant nodes (not including the root)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # async fn example(service: NodeService) -> Result<(), Box<dyn std::error::Error>> {
    /// let descendants = service.get_descendants("parent-123").await?;
    /// println!("Found {} descendants", descendants.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_descendants(&self, root_id: &str) -> Result<Vec<Node>, NodeServiceError> {
        // Use store's breadth-first traversal implementation
        let descendants = self
            .store
            .get_nodes_in_subtree(root_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        Ok(descendants)
    }

    /// Get a complete nested tree structure using efficient adjacency list strategy
    pub async fn get_children_tree(
        &self,
        parent_id: &str,
    ) -> Result<serde_json::Value, NodeServiceError> {
        // Use shared subtree data fetching
        let (root_node, node_map, adjacency_list) = self.get_subtree_data(parent_id).await?;

        match root_node {
            Some(mut root) => {
                // Fetch incoming mention containers for the root node
                // Uses optimized batch query with recursive ancestor traversal
                // Returns NodeReference with {id, title, nodeType} for each container
                root.mentioned_in = self
                    .store
                    .get_incoming_mention_containers(&root.id)
                    .await
                    .map_err(|e| {
                        NodeServiceError::query_failed(format!(
                            "Failed to fetch incoming mention containers: {}",
                            e
                        ))
                    })?;

                // Recursively build tree structure
                let tree_json = build_node_tree_recursive(&root, &node_map, &adjacency_list);
                Ok(tree_json)
            }
            None => {
                // Root node not found, return empty object
                Ok(serde_json::json!({}))
            }
        }
    }

    /// Fetch all data needed to traverse a subtree efficiently
    pub async fn get_subtree_data(&self, root_id: &str) -> Result<SubtreeData, NodeServiceError> {
        use std::collections::HashMap;

        // Single consolidated query fetches root + all descendants + all relationships
        let (all_nodes, relationships) = self
            .store
            .get_subtree_with_relationships(root_id)
            .await
            .map_err(|e| {
            NodeServiceError::query_failed(format!("Failed to fetch subtree: {}", e))
        })?;

        // Find root node from the results
        let root_node = all_nodes.iter().find(|n| n.id == root_id).cloned();

        // Create a map of node_id → Node for O(1) lookup
        let mut node_map: HashMap<String, Node> = HashMap::new();
        for node in all_nodes {
            node_map.insert(node.id.clone(), node);
        }

        // Create adjacency list: parent_id → Vec of child_ids (sorted by order)
        // Issue #788: RelationshipRecord now stores order in properties, accessed via order() method
        let mut adjacency_with_order: HashMap<String, Vec<(String, f64)>> = HashMap::new();
        for rel in relationships {
            adjacency_with_order
                .entry(rel.in_node.clone())
                .or_default()
                .push((rel.out_node.clone(), rel.order()));
        }

        // Sort children by order for each parent, then extract just the IDs
        let mut adjacency_list: HashMap<String, Vec<String>> = HashMap::new();
        for (parent_id, mut children) in adjacency_with_order {
            children.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
            adjacency_list.insert(parent_id, children.into_iter().map(|(id, _)| id).collect());
        }

        Ok((root_node, node_map, adjacency_list))
    }

    /// Check if a node is a root node (has no parent)
    pub async fn is_root_node(&self, node_id: &str) -> Result<bool, NodeServiceError> {
        // A node is a root if it has no incoming has_child relationships
        // We check this by trying to get its parent - if parent is None, it's a root
        let parent = self.get_parent(node_id).await?;
        Ok(parent.is_none())
    }

    /// Get the parent of a node (via incoming has_child relationship)
    pub async fn get_parent(&self, node_id: &str) -> Result<Option<Node>, NodeServiceError> {
        // Query for nodes that have has_child relationship pointing to this node
        // This is done by querying the relationships table for has_child edges into this node
        let parent = self
            .store
            .get_parent(node_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        Ok(parent)
    }

    /// Search nodes for mention autocomplete with proper filtering
    pub async fn mention_autocomplete(
        &self,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<Node>, NodeServiceError> {
        self.store
            .mention_autocomplete(query, limit.map(|l| l as i64))
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))
    }

    /// Get the root (root ancestor) of a node
    pub async fn get_root_id(&self, node_id: &str) -> Result<String, NodeServiceError> {
        let mut current_id = node_id.to_string();

        // Traverse up the parent chain until we find a root
        // Uses get_parent_id for efficiency (no full node fetch)
        loop {
            let parent_id = self
                .store
                .get_parent_id(&current_id)
                .await
                .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

            match parent_id {
                Some(pid) => {
                    // Keep traversing up
                    current_id = pid;
                }
                None => {
                    // Found the root
                    return Ok(current_id);
                }
            }
        }
    }

    /// Bulk fetch all nodes belonging to an origin node (viewer/page)
    ///
    /// This is the efficient way to load a complete document tree:
    /// 1. Single database query fetches all nodes with the same root_id
    /// 2. In-memory hierarchy reconstruction using parent_id and before_sibling_id
    ///
    /// This avoids making multiple queries for each level of the tree.
    ///
    /// # Arguments
    ///
    /// * `root_node_id` - The ID of the origin node (e.g., date page ID)
    ///
    /// # Returns
    ///
    /// Vector of all nodes that belong to this origin, unsorted.
    /// Caller should use `sort_by_sibling_order()` or build a tree structure.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::NodeService;
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Fetch all nodes for a date page
    /// let nodes = service.get_nodes_by_root_id("2025-10-05").await?;
    /// println!("Found {} nodes in this document", nodes.len());
    /// # Ok(())
    /// # }
    /// ```
    pub async fn get_nodes_by_root_id(
        &self,
        root_node_id: &str,
    ) -> Result<Vec<Node>, NodeServiceError> {
        // Hierarchy is now managed via relationships - use get_children instead
        self.get_children(root_node_id).await
    }

    /// Move a node to a new parent without version checking (no OCC).
    ///
    /// **Prefer `move_node()`** which enforces optimistic concurrency control.
    /// This unchecked variant is for internal operations (imports, type
    /// conversions) where version conflicts are not a concern.
    ///
    /// Updates the parent_id and root_id of a node, maintaining hierarchy consistency.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to move
    /// * `new_parent` - The new parent ID (None to make it a root node)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Node doesn't exist
    /// - New parent doesn't exist
    /// - Move would create circular reference
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::{NodeService, InsertPosition};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Move node under new parent, appending at end
    /// service.move_node_unchecked("node-id", Some("new-parent-id"), InsertPosition::End).await?;
    ///
    /// // Make node a root
    /// service.move_node_unchecked("node-id", None, InsertPosition::End).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn move_node_unchecked(
        &self,
        node_id: &str,
        new_parent: Option<&str>,
        position: crate::services::InsertPosition<'_>,
    ) -> Result<(), NodeServiceError> {
        // Verify node exists
        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Date nodes are top-level containers and cannot be moved
        if node.node_type == "date" {
            return Err(NodeServiceError::hierarchy_violation(format!(
                "Date node '{}' cannot be moved (it's a top-level container)",
                node_id
            )));
        }

        // Verify new parent exists if provided
        if let Some(parent_id) = new_parent {
            let parent_exists = self.node_exists(parent_id).await?;
            if !parent_exists {
                return Err(NodeServiceError::invalid_parent(parent_id));
            }

            // Check for circular reference - parent_id cannot be a descendant of node_id
            if self.is_descendant(node_id, parent_id).await? {
                return Err(NodeServiceError::circular_reference(format!(
                    "Cannot move node {} under its descendant {}",
                    node_id, parent_id
                )));
            }
        }

        let insert_after = self.resolve_insert_position(position, new_parent).await?;

        // Hierarchy is now managed via relationships - use store's move_node
        let actual_order = self
            .store
            .move_node(node_id, new_parent, insert_after.as_deref())
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit RelationshipUpdated event (Issue #811: unified relationship events)
        if let Some(parent_id) = new_parent {
            self.emit_event(DomainEvent::RelationshipUpdated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", parent_id, node_id),
                    parent_id,
                    node_id,
                    "has_child",
                    serde_json::json!({"order": actual_order}),
                ),
            });
        }

        Ok(())
    }

    /// Move a node to a new parent with OCC (Optimistic Concurrency Control)
    ///
    /// This method validates version before moving, preventing concurrent modifications
    /// from silently overwriting each other. The node's version is bumped after a
    /// successful move.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to move
    /// * `expected_version` - The version the caller expects (for OCC)
    /// * `new_parent` - The new parent ID (None to make it a root node)
    /// * `position` - Where to insert among the new parent's children
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Node doesn't exist
    /// - Version doesn't match (concurrent modification detected)
    /// - New parent doesn't exist
    /// - Move would create circular reference
    /// - Node is a date container (cannot be moved)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::{NodeService, InsertPosition};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Move node under new parent, appending at end
    /// service.move_node("node-id", 5, Some("new-parent-id"), InsertPosition::End).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn move_node(
        &self,
        node_id: &str,
        expected_version: i64,
        new_parent: Option<&str>,
        position: crate::services::InsertPosition<'_>,
    ) -> Result<Node, NodeServiceError> {
        // Get current node and verify version
        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Check version before proceeding
        if node.version != expected_version {
            return Err(NodeServiceError::version_conflict(
                node_id,
                expected_version,
                node.version,
            ));
        }

        // Date nodes are top-level containers and cannot be moved
        if node.node_type == "date" {
            return Err(NodeServiceError::hierarchy_violation(format!(
                "Date node '{}' cannot be moved (it's a top-level container)",
                node_id
            )));
        }

        // Verify new parent exists if provided
        if let Some(parent_id) = new_parent {
            let parent_node = self
                .get_node(parent_id)
                .await?
                .ok_or_else(|| NodeServiceError::invalid_parent(parent_id))?;

            // Enforce container rule: reject moves into non-container node types
            if !self
                .behavior_for(&parent_node.node_type)
                .can_have_children()
            {
                return Err(NodeServiceError::not_a_container(
                    parent_id,
                    &parent_node.node_type,
                ));
            }

            // Check for circular reference - parent_id cannot be a descendant of node_id
            if self.is_descendant(node_id, parent_id).await? {
                return Err(NodeServiceError::circular_reference(format!(
                    "Cannot move node {} under its descendant {}",
                    node_id, parent_id
                )));
            }
        }

        let insert_after = self.resolve_insert_position(position, new_parent).await?;

        // Perform the move
        let actual_order = self
            .store
            .move_node(node_id, new_parent, insert_after.as_deref())
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit RelationshipUpdated event (Issue #811: unified relationship events)
        if let Some(parent_id) = new_parent {
            self.emit_event(DomainEvent::RelationshipUpdated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", parent_id, node_id),
                    parent_id,
                    node_id,
                    "has_child",
                    serde_json::json!({"order": actual_order}),
                ),
            });
        }

        // Bump the node's version to support OCC
        // Even though we're only modifying edge relationships, we bump the node version
        // so that concurrent move operations will fail with version conflict
        // Returns the updated node with new version so frontend can sync its local state
        self.update_node_with_version_bump(node_id, expected_version)
            .await
    }

    /// Reorder a node within its siblings with OCC
    ///
    /// This method validates version, prevents root reordering, and bumps
    /// node version after reordering for OCC safety.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to reorder
    /// * `expected_version` - Version for optimistic concurrency control
    /// * `insert_after` - Sibling to position after (None = first position)
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Node not found
    /// - Version mismatch
    /// - Node is a root (roots cannot be reordered)
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::{NodeService, InsertPosition};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Reorder after a sibling
    /// service.reorder_node("node-id", 5, InsertPosition::After("sibling-id")).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn reorder_node(
        &self,
        node_id: &str,
        expected_version: i64,
        position: crate::services::InsertPosition<'_>,
    ) -> Result<(), NodeServiceError> {
        // Get current node and verify version
        let node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Check version before proceeding
        if node.version != expected_version {
            return Err(NodeServiceError::version_conflict(
                node_id,
                expected_version,
                node.version,
            ));
        }

        // Root nodes cannot be reordered (they have no parent)
        if self.is_root_node(node_id).await? {
            return Err(NodeServiceError::hierarchy_violation(format!(
                "Root node '{}' cannot be reordered (it has no parent)",
                node_id
            )));
        }

        // Use graph-native reordering
        self.reorder_child(node_id, position).await?;

        // Bump the node's version to support OCC
        // Even though we're only modifying edge ordering, we bump the node version
        // so that concurrent reorder operations will fail with version conflict
        // Note: We discard the returned Node since reorder_node returns ()
        let _ = self
            .update_node_with_version_bump(node_id, expected_version)
            .await?;

        Ok(())
    }

    /// Atomically re-parent an ordered set of existing children to `new_parent_id`
    /// in a single transaction (all-or-nothing OCC).
    ///
    /// All version checks happen up-front inside a single DB transaction. If any
    /// child has a version mismatch the entire batch is rolled back — nothing moves.
    /// On success each child's version is bumped and a `RelationshipUpdated` event
    /// is emitted so the frontend hierarchy-sync path reconciles order idempotently.
    ///
    /// # Arguments
    ///
    /// * `new_parent_id` — freshly-created split node; must be an empty container
    /// * `children`      — `(node_id, expected_version)` pairs in sibling order
    pub async fn move_children_to_parent(
        &self,
        new_parent_id: &str,
        children: &[(String, i64)],
    ) -> Result<Vec<Node>, NodeServiceError> {
        if children.is_empty() {
            return Ok(Vec::new());
        }

        // Verify new parent exists and can hold children.
        let parent_node = self
            .get_node(new_parent_id)
            .await?
            .ok_or_else(|| NodeServiceError::invalid_parent(new_parent_id))?;

        if !self
            .behavior_for(&parent_node.node_type)
            .can_have_children()
        {
            return Err(NodeServiceError::not_a_container(
                new_parent_id,
                &parent_node.node_type,
            ));
        }

        // Pre-validation: fetch all children, check versions, apply move_node guards.
        // Version conflicts return immediately before any write touches the DB.
        let mut nodes = Vec::with_capacity(children.len());
        for (node_id, expected_version) in children {
            let node = self
                .get_node(node_id)
                .await?
                .ok_or_else(|| NodeServiceError::node_not_found(node_id.as_str()))?;

            if node.version != *expected_version {
                return Err(NodeServiceError::version_conflict(
                    node_id,
                    *expected_version,
                    node.version,
                ));
            }

            // Date nodes are top-level containers and cannot be moved.
            if node.node_type == "date" {
                return Err(NodeServiceError::hierarchy_violation(format!(
                    "Date node '{}' cannot be moved (it's a top-level container)",
                    node_id
                )));
            }

            // Root nodes have no has_child edge — the in-transaction DELETE would
            // return 0 changes and be misidentified as a version conflict. Reject
            // root nodes explicitly so callers get a clear InvalidParent error.
            if self.is_root_node(node_id).await? {
                return Err(NodeServiceError::hierarchy_violation(format!(
                    "Root node '{}' cannot be batch-moved (no parent edge to replace)",
                    node_id
                )));
            }

            // Cycle guard: the new parent must not be a descendant of any moved child.
            if self.is_descendant(node_id, new_parent_id).await? {
                return Err(NodeServiceError::circular_reference(format!(
                    "Cannot move node {} under its descendant {}",
                    node_id, new_parent_id
                )));
            }

            nodes.push(node);
        }

        // Delegate the atomic edge-swap to the store. Version tokens are passed
        // so the store can re-validate inside the transaction (eliminates TOCTOU).
        let children_with_versions: Vec<(&str, i64)> = children
            .iter()
            .map(|(id, ver)| (id.as_str(), *ver))
            .collect();
        let orders = self
            .store
            .move_children_to_parent(new_parent_id, &children_with_versions)
            .await
            .map_err(|e| {
                let msg = e.to_string();
                // Re-map in-transaction VERSION_CONFLICT errors. The store embeds the
                // node ID in the error string: "VERSION_CONFLICT: node '<id>' ...".
                // Parse it out so the caller gets an actionable conflict message.
                if let Some(rest) = msg.strip_prefix("VERSION_CONFLICT: node '") {
                    let node_id = rest.split('\'').next().unwrap_or("unknown");
                    NodeServiceError::version_conflict(node_id, 0, 0)
                } else {
                    NodeServiceError::query_failed(msg)
                }
            })?;

        // Bump each child's version and emit RelationshipUpdated so hierarchy-sync
        // can reconcile order idempotently (C3a-consistent path).
        let mut updated = Vec::with_capacity(nodes.len());
        for (node, order) in nodes.iter().zip(orders.iter()) {
            let updated_node = self
                .update_node_with_version_bump(&node.id, node.version)
                .await?;

            self.emit_event(crate::db::events::DomainEvent::RelationshipUpdated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", new_parent_id, node.id),
                    new_parent_id,
                    &node.id,
                    "has_child",
                    serde_json::json!({"order": order}),
                ),
            });

            updated.push(updated_node);
        }

        Ok(updated)
    }

    /// Resolve an `InsertPosition` to a concrete `Option<String>` for the store layer.
    ///
    /// - `Beginning` → `None` (store interprets `None` as "before the first child")
    /// - `End`       → `Some(last_child_id)` (or `None` if the parent has no children yet)
    /// - `After(id)` → `Some(id.to_string())`
    async fn resolve_insert_position(
        &self,
        position: crate::services::InsertPosition<'_>,
        parent_id: Option<&str>,
    ) -> Result<Option<String>, NodeServiceError> {
        match position {
            crate::services::InsertPosition::Beginning => Ok(None),
            crate::services::InsertPosition::After(id) => Ok(Some(id.to_string())),
            crate::services::InsertPosition::End => {
                if let Some(pid) = parent_id {
                    let children = self.get_children(pid).await?;
                    Ok(children.last().map(|n| n.id.clone()))
                } else {
                    // `End` with no parent (root-level moves) resolves to `None`.
                    Ok(None)
                }
            }
        }
    }

    /// Create parent-child edge atomically with sibling positioning
    ///
    /// Used during node creation to establish parent relationship while preserving
    /// sibling ordering. This is separate from move_node() which is for moving existing nodes.
    ///
    /// # Arguments
    ///
    /// * `child_id` - ID of the child node (must already exist)
    /// * `parent_id` - ID of the parent node
    /// * `position` - Where to insert among the parent's children
    pub async fn create_parent_edge(
        &self,
        child_id: &str,
        parent_id: &str,
        position: crate::services::InsertPosition<'_>,
    ) -> Result<(), NodeServiceError> {
        tracing::debug!(
            child_id = %child_id,
            parent_id = %parent_id,
            position = ?position,
            "create_parent_edge: START"
        );

        // Idempotency guard for nodespace-sync#77 (alice-side echo) — if
        // `child_id` is already a child of `parent_id` AND the position is
        // End (no explicit reorder hint), treat this call as a no-op.
        // `Beginning` and `After(_)` still trigger a real reorder.
        if matches!(position, crate::services::InsertPosition::End) {
            if let Some(existing_parent) = self.get_parent(child_id).await? {
                if existing_parent.id == parent_id {
                    tracing::debug!(
                        child_id = %child_id,
                        parent_id = %parent_id,
                        "create_parent_edge: edge already exists with End position, treating as no-op"
                    );
                    return Ok(());
                }
            }
        }

        // Resolve InsertPosition::End to the actual last sibling id so the
        // store's move_node gets a concrete Option<&str>.
        let resolved = self
            .resolve_insert_position(position, Some(parent_id))
            .await?;
        let insert_after_id: Option<&str> = resolved.as_deref();

        // SQLite is synchronous/ACID: move_node commits before returning; the result
        // is immediately visible on the next read. Trust the single call result.
        let actual_order = self
            .store
            .move_node(child_id, Some(parent_id), insert_after_id)
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit RelationshipCreated event (Issue #811: unified relationship events)
        self.emit_event(DomainEvent::RelationshipCreated {
            relationship: crate::db::events::RelationshipEvent::new(
                format!("relationship:{}:{}", parent_id, child_id),
                parent_id,
                child_id,
                "has_child",
                serde_json::json!({"order": actual_order}),
            ),
        });

        tracing::debug!("create_parent_edge: COMPLETE");
        Ok(())
    }

    /// Reorder a child within its parent's children list.
    ///
    /// Updates the `has_child` edge `order` field to reposition a node among its siblings.
    ///
    /// # Arguments
    ///
    /// * `node_id` - The node to reorder
    /// * `position` - Where to place the node among its siblings
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nodespace_core::services::{NodeService, InsertPosition};
    /// # use nodespace_core::db::SqliteStore;
    /// # use std::path::PathBuf;
    /// # use std::sync::Arc;
    /// # #[tokio::main]
    /// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let mut db = Arc::new(SqliteStore::new(PathBuf::from("./test.db")).await?);
    /// # let service = NodeService::new(&mut db).await?;
    /// // Position node after sibling
    /// service.reorder_child("node-id", InsertPosition::After("sibling-id")).await?;
    ///
    /// // Move to first position
    /// service.reorder_child("node-id", InsertPosition::Beginning).await?;
    /// # Ok(())
    /// # }
    /// ```
    pub async fn reorder_child(
        &self,
        node_id: &str,
        position: crate::services::InsertPosition<'_>,
    ) -> Result<(), NodeServiceError> {
        // Verify node exists
        let _node = self
            .get_node(node_id)
            .await?
            .ok_or_else(|| NodeServiceError::node_not_found(node_id))?;

        // Verify sibling exists for After variant
        if let crate::services::InsertPosition::After(sibling_id) = position {
            let sibling_exists = self.node_exists(sibling_id).await?;
            if !sibling_exists {
                return Err(NodeServiceError::hierarchy_violation(format!(
                    "Sibling node {} does not exist",
                    sibling_id
                )));
            }
        }

        // Get current parent to move within the same parent
        let parent = self.get_parent(node_id).await?;
        let parent_id = parent.map(|p| p.id);

        let insert_after = self
            .resolve_insert_position(position, parent_id.as_deref())
            .await?;

        // Use move_node to handle edge ordering
        let actual_order = self
            .store
            .move_node(node_id, parent_id.as_deref(), insert_after.as_deref())
            .await
            .map_err(|e| NodeServiceError::query_failed(e.to_string()))?;

        // Emit RelationshipUpdated event (Issue #811: unified relationship events)
        // Reordering updates the hierarchy edge's order field
        if let Some(ref parent_id) = parent_id {
            self.emit_event(DomainEvent::RelationshipUpdated {
                relationship: crate::db::events::RelationshipEvent::new(
                    format!("relationship:{}:{}", parent_id, node_id),
                    parent_id,
                    node_id,
                    "has_child",
                    serde_json::json!({"order": actual_order}),
                ),
            });
        }

        Ok(())
    }

    /// Check if potential_descendant is a descendant of node_id
    /// This prevents circular references when moving nodes
    async fn is_descendant(
        &self,
        node_id: &str,
        potential_descendant: &str,
    ) -> Result<bool, NodeServiceError> {
        // Walk up from potential_descendant to see if we reach node_id
        let mut current_id = potential_descendant.to_string();

        for _ in 0..1000 {
            // Prevent infinite loops
            if current_id == node_id {
                return Ok(true); // Found node_id, so potential_descendant IS a descendant
            }

            // Walk up via parent relationship
            if let Ok(Some(parent)) = self.get_parent(&current_id).await {
                current_id = parent.id;
            } else {
                break; // Reached root or node not found
            }
        }

        Ok(false)
    }
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

/// Issue #1018: NodeAccessor implementation for NodeService
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

        // Issue #838: Client sends flat properties, backend normalizes to namespaced storage
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
        // Note: bulk_update replaces properties entirely (unlike single-node update_node
        // which merges key-by-key).
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
        assert_eq!(after_props_update.properties, json!({"key": "value"}));
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
    // Atomic subtree cascade delete tests (Issue #220)
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

    // =========================================================================
    // Issue #1306: BatchEmitGuard — batched event emission tests
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

    /// `bulk_create_hierarchy_trusted` must emit exactly one Created event per inserted
    /// node with no duplicates, delivered in a single flush rather than one-at-a-time
    /// (Issue #1311). The batch guard coalesces last-write-wins per node_id on drop.
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
}
