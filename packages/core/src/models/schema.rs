//! Schema Management Types
//!
//! This module contains data structures for managing user-defined entity schemas
//! in NodeSpace. Schemas are stored as regular nodes with `node_type = 'schema'`
//! and follow the Pure JSON schema-as-node pattern.
//!
//! ## Schema Protection Levels
//!
//! - `Core`: Cannot be modified or deleted (UI components depend on these fields)
//! - `User`: Fully modifiable/deletable by users
//! - `System`: Auto-managed internal fields, read-only
//!
//! ## Example Schema Node
//!
//! ```json
//! {
//!   "id": "task",
//!   "nodeType": "schema",
//!   "content": "Task",
//!   "isCore": true,
//!   "schemaVersion": 2,
//!   "description": "Task tracking schema",
//!   "fields": [
//!     {
//!       "name": "status",
//!       "type": "enum",
//!       "protection": "core",
//!       "coreValues": [
//!         { "value": "open", "label": "Open" },
//!         { "value": "in_progress", "label": "In Progress" },
//!         { "value": "done", "label": "Done" }
//!       ],
//!       "userValues": [
//!         { "value": "blocked", "label": "Blocked" }
//!       ],
//!       "extensible": true,
//!       "indexed": true,
//!       "required": true,
//!       "default": "open"
//!     }
//!   ],
//!   "relationships": [
//!     {
//!       "name": "assigned_to",
//!       "targetType": "person",
//!       "direction": "out",
//!       "cardinality": "many",
//!       "reverseName": "tasks",
//!       "reverseCardinality": "many",
//!       "edgeFields": [
//!         { "name": "role", "type": "string" }
//!       ]
//!     }
//!   ]
//! }
//! ```
//!
//! ## Relationships
//!
//! Schemas can define relationships to other node types. Relationships are stored
//! in edge tables and support:
//!
//! - **Edge table storage**: Edge table is single source of truth
//! - **Bidirectional querying**: Both directions query the same edge table
//! - **Edge fields**: Custom properties on the relationship itself
//! - **Cardinality**: "one" or "many" constraints (enforced at application level)
//!
//! See [`../nodespace-docs/archived/architecture/data/schema-relational-fields.md`] for complete details.
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
