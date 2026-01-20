//! Domain Events for SurrealStore
//!
//! This module defines the domain events emitted by SurrealStore when data changes.
//! These events follow the observer pattern, allowing other parts of the system
//! (like the Tauri layer) to subscribe to data changes without coupling to the
//! database layer implementation.
//!
//! # Architecture
//!
//! Events are emitted using tokio's broadcast channel, allowing multiple subscribers
//! to receive notifications asynchronously.
//!
//! # Event Flow
//!
//! 1. SurrealStore performs a data operation (create, update, delete)
//! 2. Domain event is emitted via broadcast channel
//! 3. All subscribers receive the event asynchronously
//! 4. LiveQueryService (Tauri layer) listens to events and forwards to frontend
//!
//! # Unified Relationship Event System (Issue #811)
//!
//! All relationships (`has_child`, `member_of`, `mentions`, and custom types)
//! use a generic `RelationshipEvent` struct with `relationship_type` for discrimination.
//! This allows adding new relationship types without modifying the event system.

use serde::{Deserialize, Serialize};

/// Represents hierarchy relationships between nodes (parent-child with ordering)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HierarchyRelationship {
    pub parent_id: String,
    pub child_id: String,
    pub order: f64,
}

/// Represents mention relationships between nodes (bidirectional references)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MentionRelationship {
    pub source_id: String,
    pub target_id: String,
}

/// Represents collection membership relationship (node belongs to collection)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionMembership {
    /// The node that is a member
    pub member_id: String,
    /// The collection the node belongs to
    pub collection_id: String,
}

/// Unified relationship event for all relationship types (Issue #811)
///
/// This generic structure supports all relationship types: `has_child`, `member_of`, `mentions`,
/// and any future custom relationship types. It replaces the enum-based approach
/// that required modifying the event system for each new relationship type.
///
/// # Relationship Types
///
/// - `"has_child"` - Hierarchical parent-child relationship with `order` property
/// - `"member_of"` - Collection membership (node belongs to collection)
/// - `"mentions"` - Bidirectional reference between nodes
/// - Custom types - Any string representing a user-defined relationship
///
/// # Properties
///
/// Type-specific data stored in `properties`:
/// - `has_child`: `{"order": 1.5}`
/// - `mentions`: `{"context": "optional context"}`
/// - `member_of`: `{}` (no additional properties)
/// - Custom: User-defined JSON properties
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipEvent {
    /// Unique relationship ID in SurrealDB format (e.g., "relationship:abc123")
    pub id: String,
    /// Source node ID (the "in" node in the relationship graph edge)
    pub from_id: String,
    /// Target node ID (the "out" node in the relationship graph edge)
    pub to_id: String,
    /// Relationship type: "has_child", "mentions", "member_of", or custom types
    pub relationship_type: String,
    /// Type-specific properties (order for hierarchy, context for mentions, etc.)
    pub properties: serde_json::Value,
}

/// Represents different types of edge relationships between nodes
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum EdgeRelationship {
    /// Hierarchical parent-child relationship with ordering
    #[serde(rename = "hierarchy")]
    Hierarchy(HierarchyRelationship),
    /// Bidirectional mention/reference relationship
    #[serde(rename = "mention")]
    Mention(MentionRelationship),
}

/// Domain events emitted by SurrealStore
///
/// These events are emitted whenever data changes in the database.
/// They represent domain-level changes, not database operations.
///
/// Each event includes an optional `source_client_id` to identify the originating client,
/// allowing event subscribers to filter out their own events (prevent feedback loops).
///
/// Node events send only the `node_id` (not full payload) for efficiency.
/// Subscribers fetch the full node data via `get_node()` if needed (Issue #724).
/// This reduces bandwidth during bulk operations and lets clients decide what they need.
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// A new node was created
    NodeCreated {
        node_id: String,
        source_client_id: Option<String>,
    },

    /// An existing node was updated
    NodeUpdated {
        node_id: String,
        source_client_id: Option<String>,
    },

    /// A node was deleted
    NodeDeleted {
        id: String,
        source_client_id: Option<String>,
    },

    /// A new edge relationship (hierarchy or mention) was created
    EdgeCreated {
        relationship: EdgeRelationship,
        source_client_id: Option<String>,
    },

    /// An existing edge relationship was updated
    EdgeUpdated {
        relationship: EdgeRelationship,
        source_client_id: Option<String>,
    },

    /// An edge relationship was deleted
    EdgeDeleted {
        id: String,
        source_client_id: Option<String>,
    },

    /// A node was added to a collection
    ///
    /// DEPRECATED: Use UnifiedEdgeCreated with relationship_type="member_of" instead.
    /// Will be removed after migration to unified edge events (Issue #811).
    CollectionMemberAdded {
        membership: CollectionMembership,
        source_client_id: Option<String>,
    },

    /// A node was removed from a collection
    ///
    /// DEPRECATED: Use UnifiedEdgeDeleted with relationship_type="member_of" instead.
    /// Will be removed after migration to unified edge events (Issue #811).
    CollectionMemberRemoved {
        membership: CollectionMembership,
        source_client_id: Option<String>,
    },

    // ============================================================================
    // Unified Relationship Events (Issue #811)
    // These replace the typed EdgeCreated/EdgeUpdated/EdgeDeleted and
    // CollectionMemberAdded/CollectionMemberRemoved events with a generic format.
    // ============================================================================
    /// A new relationship was created (unified format for all relationship types)
    ///
    /// Supports: `has_child`, `member_of`, `mentions`, and custom relationship types.
    RelationshipCreated {
        relationship: RelationshipEvent,
        source_client_id: Option<String>,
    },

    /// An existing relationship was updated (unified format for all relationship types)
    ///
    /// Typically used for reordering (updating `order` property on `has_child` relationships).
    RelationshipUpdated {
        relationship: RelationshipEvent,
        source_client_id: Option<String>,
    },

    /// A relationship was deleted (unified format for all relationship types)
    ///
    /// Contains only the relationship ID for efficiency.
    RelationshipDeleted {
        /// The SurrealDB relationship ID (e.g., "relationship:abc123")
        id: String,
        /// Relationship type hint for handlers that need it
        relationship_type: String,
        source_client_id: Option<String>,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Contract test: Documents and enforces the exact JSON format for EdgeRelationship
    ///
    /// IMPORTANT: The frontend TypeScript types in event-types.ts MUST match this format.
    /// If this test fails, either the Rust serialization or TypeScript types need updating.
    ///
    /// Serde's `#[serde(tag = "type")]` produces an INTERNALLY-TAGGED format where the
    /// discriminator field is merged with the struct fields (NOT nested).
    #[test]
    fn test_edge_relationship_serialization_contract() {
        // Test Hierarchy variant
        let hierarchy = EdgeRelationship::Hierarchy(HierarchyRelationship {
            parent_id: "parent-123".to_string(),
            child_id: "child-456".to_string(),
            order: 1.5,
        });

        let json = serde_json::to_string(&hierarchy).unwrap();

        // Internally-tagged format: type field merged with struct fields (FLAT, not nested)
        // Expected: {"type":"hierarchy","parentId":"...","childId":"...","order":1.5}
        // NOT: {"type":"hierarchy","hierarchy":{"parentId":"...","childId":"...","order":1.5}}
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.get("type").unwrap(), "hierarchy");
        assert_eq!(parsed.get("parentId").unwrap(), "parent-123");
        assert_eq!(parsed.get("childId").unwrap(), "child-456");
        assert_eq!(parsed.get("order").unwrap(), 1.5);
        // Verify NOT nested - there should be no "hierarchy" key
        assert!(
            parsed.get("hierarchy").is_none(),
            "Should NOT be nested under 'hierarchy' key"
        );

        // Test Mention variant
        let mention = EdgeRelationship::Mention(MentionRelationship {
            source_id: "source-123".to_string(),
            target_id: "target-456".to_string(),
        });

        let json = serde_json::to_string(&mention).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.get("type").unwrap(), "mention");
        assert_eq!(parsed.get("sourceId").unwrap(), "source-123");
        assert_eq!(parsed.get("targetId").unwrap(), "target-456");
        // Verify NOT nested
        assert!(
            parsed.get("mention").is_none(),
            "Should NOT be nested under 'mention' key"
        );
    }

    /// Test that deserialization works correctly (round-trip)
    #[test]
    fn test_edge_relationship_deserialization() {
        // Hierarchy round-trip
        let original = EdgeRelationship::Hierarchy(HierarchyRelationship {
            parent_id: "parent-123".to_string(),
            child_id: "child-456".to_string(),
            order: 1.5,
        });
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EdgeRelationship = serde_json::from_str(&json).unwrap();

        match deserialized {
            EdgeRelationship::Hierarchy(h) => {
                assert_eq!(h.parent_id, "parent-123");
                assert_eq!(h.child_id, "child-456");
                assert!((h.order - 1.5).abs() < f64::EPSILON);
            }
            _ => panic!("Expected Hierarchy variant"),
        }

        // Mention round-trip
        let original = EdgeRelationship::Mention(MentionRelationship {
            source_id: "source-123".to_string(),
            target_id: "target-456".to_string(),
        });
        let json = serde_json::to_string(&original).unwrap();
        let deserialized: EdgeRelationship = serde_json::from_str(&json).unwrap();

        match deserialized {
            EdgeRelationship::Mention(m) => {
                assert_eq!(m.source_id, "source-123");
                assert_eq!(m.target_id, "target-456");
            }
            _ => panic!("Expected Mention variant"),
        }
    }

    /// Contract test: Documents and enforces the exact JSON format for RelationshipEvent (Issue #811)
    ///
    /// IMPORTANT: The frontend TypeScript types MUST match this format.
    /// This is the unified format that supports all relationship types.
    #[test]
    fn test_relationship_event_serialization_contract() {
        // Test has_child relationship (hierarchy)
        let has_child = RelationshipEvent {
            id: "relationship:abc123".to_string(),
            from_id: "node:parent-123".to_string(),
            to_id: "node:child-456".to_string(),
            relationship_type: "has_child".to_string(),
            properties: serde_json::json!({"order": 1.5}),
        };

        let json = serde_json::to_string(&has_child).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        // Verify camelCase field names
        assert_eq!(parsed.get("id").unwrap(), "relationship:abc123");
        assert_eq!(parsed.get("fromId").unwrap(), "node:parent-123");
        assert_eq!(parsed.get("toId").unwrap(), "node:child-456");
        assert_eq!(parsed.get("relationshipType").unwrap(), "has_child");
        assert_eq!(parsed.get("properties").unwrap().get("order").unwrap(), 1.5);

        // Test member_of relationship (collection membership)
        let member_of = RelationshipEvent {
            id: "relationship:xyz789".to_string(),
            from_id: "node:item-001".to_string(),
            to_id: "node:collection-002".to_string(),
            relationship_type: "member_of".to_string(),
            properties: serde_json::json!({}),
        };

        let json = serde_json::to_string(&member_of).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.get("relationshipType").unwrap(), "member_of");
        assert!(parsed
            .get("properties")
            .unwrap()
            .as_object()
            .unwrap()
            .is_empty());

        // Test mentions relationship
        let mentions = RelationshipEvent {
            id: "relationship:mention-456".to_string(),
            from_id: "node:source-123".to_string(),
            to_id: "node:target-456".to_string(),
            relationship_type: "mentions".to_string(),
            properties: serde_json::json!({"context": "see also"}),
        };

        let json = serde_json::to_string(&mentions).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.get("relationshipType").unwrap(), "mentions");
        assert_eq!(
            parsed.get("properties").unwrap().get("context").unwrap(),
            "see also"
        );
    }

    /// Test RelationshipEvent round-trip deserialization
    #[test]
    fn test_relationship_event_deserialization() {
        let original = RelationshipEvent {
            id: "relationship:test123".to_string(),
            from_id: "node:from-id".to_string(),
            to_id: "node:to-id".to_string(),
            relationship_type: "custom_type".to_string(),
            properties: serde_json::json!({"custom_prop": "value", "number": 42}),
        };

        let json = serde_json::to_string(&original).unwrap();
        let deserialized: RelationshipEvent = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.id, "relationship:test123");
        assert_eq!(deserialized.from_id, "node:from-id");
        assert_eq!(deserialized.to_id, "node:to-id");
        assert_eq!(deserialized.relationship_type, "custom_type");
        assert_eq!(deserialized.properties.get("custom_prop").unwrap(), "value");
        assert_eq!(deserialized.properties.get("number").unwrap(), 42);
    }
}
