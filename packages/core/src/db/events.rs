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

use crate::models::Node;
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
#[derive(Debug, Clone)]
pub enum DomainEvent {
    /// A new node was created
    NodeCreated(Node),

    /// An existing node was updated
    NodeUpdated(Node),

    /// A node was deleted
    NodeDeleted { id: String },

    /// A new edge relationship (hierarchy or mention) was created
    EdgeCreated(EdgeRelationship),

    /// An existing edge relationship was updated
    EdgeUpdated(EdgeRelationship),

    /// An edge relationship was deleted
    EdgeDeleted { id: String },
}

impl DomainEvent {
    /// Get a string representation of the event type
    ///
    /// Currently unused but provides useful API surface for debugging, logging,
    /// and future consumers of domain events (e.g., MCP servers, webhooks).
    #[allow(dead_code)]
    pub fn event_type(&self) -> &str {
        match self {
            DomainEvent::NodeCreated(_) => "node:created",
            DomainEvent::NodeUpdated(_) => "node:updated",
            DomainEvent::NodeDeleted { .. } => "node:deleted",
            DomainEvent::EdgeCreated(_) => "edge:created",
            DomainEvent::EdgeUpdated(_) => "edge:updated",
            DomainEvent::EdgeDeleted { .. } => "edge:deleted",
        }
    }
}
