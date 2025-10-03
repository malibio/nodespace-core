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

pub use node::{Node, NodeFilter, NodeUpdate, ValidationError};
