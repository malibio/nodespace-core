//! Data Models
//!
//! This module contains the core data structures used throughout NodeSpace:
//!
//! - `Node` - Universal node model for all content types
//! - Type-specific structures (Task, Person, etc.) built on Node foundation
//!
//! All entities use the Pure JSON schema approach with data stored in the
//! `properties` field of the universal `nodes` table.

mod node;
pub mod schema;
mod task_node;
#[cfg(test)]
#[path = "task_node_test.rs"]
mod task_node_test;
pub mod time;

pub use node::{
    DeleteResult, FilterOperator, Node, NodeFilter, NodeQuery, NodeUpdate, OrderBy, PropertyFilter,
    ValidationError,
};
pub use schema::{ProtectionLevel, SchemaDefinition, SchemaField};
pub use task_node::{TaskNode, TaskNodeBuilder, TaskStatus};
pub use time::{SystemTimeProvider, TimeProvider};
