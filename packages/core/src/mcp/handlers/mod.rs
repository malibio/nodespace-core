//! MCP Request Handlers
//!
//! Handler modules for different MCP operations.
//!
//! Note: Schema-specific handlers removed per Issue #690.
//! Use generic CRUD (create_node, update_node, query_nodes) for schema management.
//! Relationship CRUD is available via the relationships module (Issue #703).

pub mod initialize;
pub mod markdown;
pub mod natural_language_schema;
pub mod nodes;
pub mod relationships;
pub mod search;
pub mod tools;
