//! Relationship Operations
//!
//! Typed orchestration for relationship CRUD. Extracted from MCP handlers.

use crate::models::schema::{EdgeField, RelationshipCardinality, BUILTIN_RELATIONSHIP_NAMES};
use crate::ops::OpsError;
use crate::services::NodeService;
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;

// ============================================================================
// Input / Output types
// ============================================================================

#[derive(Debug)]
pub struct CreateRelInput {
    pub source_id: String,
    pub relationship_name: String,
    pub target_id: String,
    pub edge_data: Option<Value>,
}

#[derive(Debug)]
pub struct CreateRelOutput {
    pub source_id: String,
    pub relationship_name: String,
    pub target_id: String,
}

#[derive(Debug)]
pub struct DeleteRelInput {
    pub source_id: String,
    pub relationship_name: String,
    pub target_id: String,
}

#[derive(Debug)]
pub struct UpdateRelPropsInput {
    pub source_id: String,
    pub relationship_name: String,
    pub target_id: String,
    /// The new edge attributes; replaces the edge's stored `properties` wholesale.
    pub properties: Value,
}

#[derive(Debug)]
pub struct GetRelatedInput {
    pub node_id: String,
    pub relationship_name: String,
    /// "out" (forward) or "in" (reverse)
    pub direction: String,
}

#[derive(Debug)]
pub struct GetRelatedOutput {
    pub node_id: String,
    pub relationship_name: String,
    pub direction: String,
    pub related_nodes: Vec<Value>,
    pub count: usize,
}

// ============================================================================
// Operations
// ============================================================================

/// Create a relationship edge between two nodes.
pub async fn create_relationship(
    node_service: &Arc<NodeService>,
    input: CreateRelInput,
) -> Result<CreateRelOutput, OpsError> {
    let edge_data = input.edge_data.unwrap_or(json!({}));

    node_service
        .create_relationship(
            &input.source_id,
            &input.relationship_name,
            &input.target_id,
            edge_data,
        )
        .await
        .map_err(OpsError::from)?;

    Ok(CreateRelOutput {
        source_id: input.source_id,
        relationship_name: input.relationship_name,
        target_id: input.target_id,
    })
}

/// Delete a relationship edge. Idempotent.
pub async fn delete_relationship(
    node_service: &Arc<NodeService>,
    input: DeleteRelInput,
) -> Result<(), OpsError> {
    node_service
        .delete_relationship(&input.source_id, &input.relationship_name, &input.target_id)
        .await
        .map_err(OpsError::from)?;

    Ok(())
}

/// Replace the edge attributes on an existing typed relationship edge.
pub async fn update_relationship_properties(
    node_service: &Arc<NodeService>,
    input: UpdateRelPropsInput,
) -> Result<(), OpsError> {
    node_service
        .update_relationship_properties(
            &input.source_id,
            &input.relationship_name,
            &input.target_id,
            input.properties,
        )
        .await
        .map_err(OpsError::from)?;

    Ok(())
}

/// Get nodes related via a specific relationship.
pub async fn get_related_nodes(
    node_service: &Arc<NodeService>,
    input: GetRelatedInput,
) -> Result<GetRelatedOutput, OpsError> {
    let nodes = node_service
        .get_related_nodes(&input.node_id, &input.relationship_name, &input.direction)
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to get related nodes: {}", e)))?;

    let count = nodes.len();
    let related_nodes: Vec<Value> = nodes
        .into_iter()
        .map(|n| serde_json::to_value(n).unwrap_or(json!(null)))
        .collect();

    Ok(GetRelatedOutput {
        node_id: input.node_id,
        relationship_name: input.relationship_name,
        direction: input.direction,
        related_nodes,
        count,
    })
}

// ============================================================================
// Relationship viewer aggregation (issue #1918 — read-only LIST)
// ============================================================================

/// A single related node on one end of a typed relationship, carrying enough
/// identity to be recognizable plus the connecting edge's stored properties.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedNodeView {
    pub id: String,
    pub node_type: String,
    /// Current computed title (read at query time, never snapshotted).
    pub title: Option<String>,
    /// Truncated content, for types without a title template.
    pub content_preview: String,
    /// The connecting edge's stored `properties` JSON (edge attributes).
    pub edge_properties: Value,
}

/// One relationship group: a single (relationship name, direction) pairing, with
/// all of its related nodes. Outbound and inbound are kept as SEPARATE groups —
/// a self-referential relationship legitimately appears twice (once per side).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelationshipGroup {
    /// Underlying schema relationship name (the edge's `relationship_type`).
    pub relationship_name: String,
    /// "out" = declared on this node's own schema; "in" = another schema declares
    /// this node's type as its `target_type`.
    pub direction: String,
    /// The node type on the FAR end of the edge (outbound: the declared
    /// `target_type`; inbound: the declaring `source_type`).
    pub target_type: Option<String>,
    /// Reverse label supplied by the source schema; the inbound side displays
    /// this when present (falling back to a derived label in the viewer).
    pub reverse_name: Option<String>,
    /// The schema type that DECLARES this relationship (used to build the inbound
    /// fallback label when `reverse_name` is absent).
    pub source_type: String,
    /// Effective cardinality for THIS side: the forward `cardinality` outbound,
    /// the `reverse_cardinality` inbound (may be absent inbound).
    pub cardinality: Option<RelationshipCardinality>,
    /// Whether the forward relationship is required (outbound only). The viewer
    /// uses this to confirm-on-delete; last-edge removal is enforced server-side
    /// in `NodeService::delete_relationship`.
    pub required: Option<bool>,
    /// Declared edge field definitions, if any (drives the edge-attribute columns).
    pub edge_fields: Option<Vec<EdgeField>>,
    pub description: Option<String>,
    pub related: Vec<RelatedNodeView>,
    pub count: usize,
}

/// A node's typed relationships, grouped by (name, direction). Excludes the
/// built-in structural relationships. Declared outbound groups are always
/// included (even with no edges yet, so the viewer can offer to add the first
/// one); inbound groups appear only when they have at least one edge.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NodeRelationshipsOutput {
    pub node_id: String,
    pub node_type: String,
    pub groups: Vec<RelationshipGroup>,
}

/// Truncate content to a compact preview (character-safe).
fn content_preview(content: &str) -> String {
    const MAX: usize = 140;
    let trimmed = content.trim();
    if trimmed.chars().count() <= MAX {
        trimmed.to_string()
    } else {
        let truncated: String = trimmed.chars().take(MAX).collect();
        format!("{}…", truncated)
    }
}

/// Fetch the related nodes for one (relationship, direction) pairing, each with
/// its connecting edge's properties, mapped into `RelatedNodeView`s.
async fn collect_related(
    node_service: &Arc<NodeService>,
    node_id: &str,
    relationship_name: &str,
    direction: &str,
) -> Result<Vec<RelatedNodeView>, OpsError> {
    let pairs = node_service
        .get_related_nodes_with_edges(node_id, relationship_name, direction)
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to get related nodes: {}", e)))?;

    Ok(pairs
        .into_iter()
        .map(|(node, edge_properties)| RelatedNodeView {
            content_preview: content_preview(&node.content),
            title: node.title.clone(),
            id: node.id,
            node_type: node.node_type,
            edge_properties,
        })
        .collect())
}

/// List a node's schema-declared typed relationships, grouped by name and
/// covering BOTH directions (read-only; issue #1918 first slice).
///
/// - **Outbound** groups come from the node's own schema's `relationships`
///   (queried with direction `"out"`; the node is the edge source).
/// - **Inbound** groups come from [`NodeService::get_inbound_relationships`],
///   which resolves every schema whose relationship targets this node's type
///   (including untyped `target_type: None` relationships that apply to any
///   type), queried with direction `"in"` (the node is the edge target). The
///   inbound side is governed by `reverse_cardinality` and labeled by
///   `reverse_name`.
///
/// Each related node carries its connecting edge's `properties` so the viewer can
/// render edge attributes. Built-in structural relationships (`has_child`,
/// `mentions`, `member_of`, `has_role`) are excluded. Declared outbound groups are
/// always returned (even when empty, so the viewer can add the first edge);
/// inbound groups are returned only when they have at least one edge.
pub async fn get_node_relationships(
    node_service: &Arc<NodeService>,
    node_id: &str,
) -> Result<NodeRelationshipsOutput, OpsError> {
    let node = node_service
        .get_node(node_id)
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to load node: {}", e)))?
        .ok_or_else(|| OpsError::NotFound {
            id: node_id.to_string(),
        })?;
    let node_type = node.node_type.clone();

    let mut groups: Vec<RelationshipGroup> = Vec::new();

    // ---- Outbound: relationships declared on this node's own schema ----
    // Schema nodes have id == node_type. A node whose type has no schema (e.g.
    // plain text) simply has no outbound typed relationships. Declarations
    // arrive hydrated from the relationship table — the same consolidated read
    // path `NodeService::create_relationship` validates against, so the viewer
    // and the write path can never see different declaration sets.
    if let Some(schema_node) = node_service
        .get_schema_node(&node_type)
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to load schema node: {}", e)))?
    {
        for rel in schema_node.relationships {
            if BUILTIN_RELATIONSHIP_NAMES.contains(&rel.name.as_str()) {
                continue;
            }
            // Emit the group even when it has no edges yet: an empty declared
            // outbound relationship still needs to render so the viewer can add
            // its first edge. (Inbound groups below keep skipping empties.)
            let related = collect_related(node_service, node_id, &rel.name, "out").await?;
            let count = related.len();
            groups.push(RelationshipGroup {
                relationship_name: rel.name.clone(),
                direction: "out".to_string(),
                target_type: rel.target_type.clone(),
                reverse_name: rel.reverse_name.clone(),
                source_type: node_type.clone(),
                cardinality: Some(rel.cardinality.clone()),
                required: rel.required,
                edge_fields: rel.edge_fields.clone(),
                description: rel.description.clone(),
                related,
                count,
            });
        }
    }

    // ---- Inbound: other schemas whose relationship targets this node's type ----
    let inbound = node_service
        .get_inbound_relationships(&node_type)
        .await
        .map_err(|e| {
            OpsError::Internal(format!("Failed to resolve inbound relationships: {}", e))
        })?;

    for (source_type, rel) in inbound {
        if BUILTIN_RELATIONSHIP_NAMES.contains(&rel.name.as_str()) {
            continue;
        }
        let mut related = collect_related(node_service, node_id, &rel.name, "in").await?;
        // The "in" query keys only on relationship_type; two schemas can declare
        // the same relationship name targeting this type, so restrict this group
        // to nodes of the declaring source type — otherwise e.g. `task` and `bug`
        // both declaring `assigned_to → person` would each surface the other's
        // edges under the wrong group and double the count.
        related.retain(|r| r.node_type == source_type);
        if related.is_empty() {
            continue;
        }
        let count = related.len();
        groups.push(RelationshipGroup {
            relationship_name: rel.name.clone(),
            direction: "in".to_string(),
            target_type: Some(source_type.clone()),
            reverse_name: rel.reverse_name.clone(),
            source_type,
            // Reading from the inbound side, the reverse cardinality governs.
            cardinality: rel.reverse_cardinality.clone(),
            required: None,
            edge_fields: rel.edge_fields.clone(),
            description: rel.description.clone(),
            related,
            count,
        });
    }

    Ok(NodeRelationshipsOutput {
        node_id: node_id.to_string(),
        node_type,
        groups,
    })
}
