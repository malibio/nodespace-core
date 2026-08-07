//! Schema Management Types
//!
//! This module contains data structures for managing user-defined entity schemas
//! in NodeSpace. Schemas are stored as regular nodes with `node_type = 'schema'`.
//! Field definitions live in the schema node's `properties.fields` JSON;
//! relationship declarations are NOT stored in properties — each one is a row in
//! the `relationship` table between the declaring and target schema nodes.
//!
//! ## Schema Protection Levels
//!
//! - `Core`: Cannot be modified or deleted (UI components depend on these fields)
//! - `User`: Fully modifiable/deletable by users
//! - `System`: Auto-managed internal fields, read-only
//!
//! ## Relationships
//!
//! Schemas can define typed relationships to other node types. A declaration is
//! stored as a `relationship` table row: `in_node` = the declaring schema node,
//! `out_node` = the target schema node (or the declaring schema itself when
//! `target_type` is `None` — an untyped relationship accepting any target),
//! `relationship_type` = the declared name, and the full [`SchemaRelationship`]
//! serialized into the row's `properties` JSON. Instance-level edges reuse the
//! same table with the same `relationship_type`; the two are distinguished by
//! their endpoints (declaration edges connect schema nodes, instance edges
//! connect instance nodes).
//!
//! Reads are consolidated in `SqliteStore::get_schema_declarations` /
//! `get_all_schema_declarations`, which hydrate `SchemaNode.relationships`;
//! writes go through `SqliteStore::set_schema_declarations`.
//!
//! - **Bidirectional querying**: both directions query the same edge rows
//! - **Edge fields**: custom properties on the relationship itself
//! - **Cardinality**: "one" or "many" constraints (enforced at application level)
//!
//! ## Source of truth
//!
//! These field/relationship primitives are the wire shapes shared with the Tauri
//! command layer, so they live in `nodespace-types` and are re-exported here.
//! There is exactly one definition per type — adding a field in `nodespace-types`
//! surfaces here automatically, and the round-trip tests in that crate fail if a
//! field is dropped at the conversion boundary.

pub use nodespace_types::{
    EdgeField, EnumValue, RelationshipCardinality, RelationshipDirection, SchemaField,
    SchemaProtectionLevel, SchemaRelationship,
};

/// Built-in structural relationship types. These are not schema-declared: they
/// have hardcoded semantics (hierarchy, mentions, collection membership, roles)
/// and their own UI affordances.
///
/// Because schema relationship declarations and these primitives share the one
/// `relationship` table's `relationship_type` column, a declared relationship
/// must never take one of these names — schema creation/update rejects them.
pub const BUILTIN_RELATIONSHIP_NAMES: [&str; 4] =
    ["member_of", "has_child", "mentions", "has_role"];

/// Whether `name` is one of the built-in structural relationship types.
pub fn is_builtin_relationship(name: &str) -> bool {
    BUILTIN_RELATIONSHIP_NAMES.contains(&name)
}
