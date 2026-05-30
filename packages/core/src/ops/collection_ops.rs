//! Collection Operations
//!
//! Thin orchestration wrappers over `CollectionService`. Extracted from the daemon
//! layer so both the gRPC adapter and future callers share the same logic.

use crate::models::Node;
use crate::ops::OpsError;
use crate::services::{CollectionService, NodeService, NodeServiceError};
use std::sync::Arc;

// ============================================================================
// Input / Output types
// ============================================================================

pub struct GetAllCollectionsInput;

pub struct CollectionEntry {
    pub node: Node,
    pub member_count: usize,
    pub parent_collection_ids: Vec<String>,
}

pub struct GetAllCollectionsOutput {
    pub collections: Vec<CollectionEntry>,
}

pub struct GetCollectionMembersInput {
    pub collection_id: String,
}

pub struct GetCollectionMembersOutput {
    pub members: Vec<Node>,
    pub collection_id: String,
}

pub struct GetCollectionMembersRecursiveInput {
    pub collection_id: String,
}

pub struct GetCollectionMembersRecursiveOutput {
    /// Member node IDs in traversal order.
    pub member_ids: Vec<String>,
    pub collection_id: String,
}

pub struct GetNodeCollectionsInput {
    pub node_id: String,
}

pub struct GetNodeCollectionsOutput {
    pub collection_ids: Vec<String>,
}

pub struct AddNodeToCollectionInput {
    pub node_id: String,
    pub collection_id: String,
}

pub struct AddNodeToCollectionOutput;

pub struct AddNodeToCollectionByPathInput {
    pub node_id: String,
    pub collection_path: String,
}

pub struct AddNodeToCollectionByPathOutput {
    pub collection_id: String,
}

pub struct RemoveNodeFromCollectionInput {
    pub node_id: String,
    pub collection_id: String,
}

pub struct RemoveNodeFromCollectionOutput;

pub struct FindCollectionByPathInput {
    pub collection_path: String,
}

pub struct FindCollectionByPathOutput {
    pub collection: Option<Node>,
}

pub struct GetCollectionByNameInput {
    pub name: String,
}

pub struct GetCollectionByNameOutput {
    pub collection: Option<Node>,
}

pub struct CreateCollectionInput {
    pub name: String,
    pub description: String,
}

pub struct CreateCollectionOutput {
    pub collection_id: String,
}

pub struct RenameCollectionInput {
    pub collection_id: String,
    pub new_name: String,
    pub version: i64,
}

pub struct RenameCollectionOutput {
    pub node: Node,
}

// ============================================================================
// Operations
// ============================================================================

pub async fn get_all_collections(
    node_service: &Arc<NodeService>,
    _input: GetAllCollectionsInput,
) -> Result<GetAllCollectionsOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let entries = collection_service
        .get_all_collections_with_counts()
        .await
        .map_err(OpsError::from)?;

    Ok(GetAllCollectionsOutput {
        collections: entries
            .into_iter()
            .map(
                |(node, member_count, parent_collection_ids)| CollectionEntry {
                    node,
                    member_count,
                    parent_collection_ids,
                },
            )
            .collect(),
    })
}

pub async fn get_collection_members(
    node_service: &Arc<NodeService>,
    input: GetCollectionMembersInput,
) -> Result<GetCollectionMembersOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let members = collection_service
        .get_collection_members(&input.collection_id)
        .await
        .map_err(OpsError::from)?;

    Ok(GetCollectionMembersOutput {
        collection_id: input.collection_id,
        members,
    })
}

pub async fn get_collection_members_recursive(
    node_service: &Arc<NodeService>,
    input: GetCollectionMembersRecursiveInput,
) -> Result<GetCollectionMembersRecursiveOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let member_ids = collection_service
        .get_collection_members_recursive(&input.collection_id)
        .await
        .map_err(OpsError::from)?;

    Ok(GetCollectionMembersRecursiveOutput {
        collection_id: input.collection_id,
        member_ids,
    })
}

pub async fn get_node_collections(
    node_service: &Arc<NodeService>,
    input: GetNodeCollectionsInput,
) -> Result<GetNodeCollectionsOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let collection_ids = collection_service
        .get_node_collections(&input.node_id)
        .await
        .map_err(OpsError::from)?;

    Ok(GetNodeCollectionsOutput { collection_ids })
}

pub async fn add_node_to_collection(
    node_service: &Arc<NodeService>,
    input: AddNodeToCollectionInput,
) -> Result<AddNodeToCollectionOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    collection_service
        .add_to_collection(&input.node_id, &input.collection_id)
        .await
        .map_err(OpsError::from)?;

    Ok(AddNodeToCollectionOutput)
}

pub async fn add_node_to_collection_by_path(
    node_service: &Arc<NodeService>,
    input: AddNodeToCollectionByPathInput,
) -> Result<AddNodeToCollectionByPathOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let resolved = collection_service
        .add_to_collection_by_path(&input.node_id, &input.collection_path)
        .await
        .map_err(OpsError::from)?;

    Ok(AddNodeToCollectionByPathOutput {
        collection_id: resolved.leaf_id().to_string(),
    })
}

pub async fn remove_node_from_collection(
    node_service: &Arc<NodeService>,
    input: RemoveNodeFromCollectionInput,
) -> Result<RemoveNodeFromCollectionOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    collection_service
        .remove_from_collection(&input.node_id, &input.collection_id)
        .await
        .map_err(OpsError::from)?;

    Ok(RemoveNodeFromCollectionOutput)
}

pub async fn find_collection_by_path(
    node_service: &Arc<NodeService>,
    input: FindCollectionByPathInput,
) -> Result<FindCollectionByPathOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let collection = collection_service
        .find_collection_by_path(&input.collection_path)
        .await
        .map_err(OpsError::from)?;

    Ok(FindCollectionByPathOutput { collection })
}

pub async fn get_collection_by_name(
    node_service: &Arc<NodeService>,
    input: GetCollectionByNameInput,
) -> Result<GetCollectionByNameOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let collection = collection_service
        .get_collection_by_name(&input.name)
        .await
        .map_err(OpsError::from)?;

    Ok(GetCollectionByNameOutput { collection })
}

pub async fn create_collection(
    node_service: &Arc<NodeService>,
    input: CreateCollectionInput,
) -> Result<CreateCollectionOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);

    if collection_service
        .get_collection_by_name(&input.name)
        .await
        .map_err(OpsError::from)?
        .is_some()
    {
        return Err(OpsError::ValidationFailed(format!(
            "Collection '{}' already exists",
            input.name
        )));
    }

    let properties = if input.description.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "description": input.description })
    };

    let collection_id = node_service
        .create_node_with_parent(crate::services::CreateNodeParams {
            id: None,
            node_type: "collection".to_string(),
            content: input.name,
            parent_id: None,
            position: crate::services::InsertPositionOwned::End,
            properties,
        })
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to create collection node: {}", e)))?;

    Ok(CreateCollectionOutput { collection_id })
}

pub async fn rename_collection(
    node_service: &Arc<NodeService>,
    input: RenameCollectionInput,
) -> Result<RenameCollectionOutput, OpsError> {
    let collection_service = CollectionService::new(node_service.store(), node_service);

    if let Some(existing) = collection_service
        .get_collection_by_name(&input.new_name)
        .await
        .map_err(OpsError::from)?
    {
        if existing.id != input.collection_id {
            return Err(OpsError::ValidationFailed(format!(
                "Collection '{}' already exists",
                input.new_name
            )));
        }
    }

    let update = crate::models::NodeUpdate {
        content: Some(input.new_name),
        ..Default::default()
    };

    let node = node_service
        .update_node(&input.collection_id, input.version, update)
        .await
        .map_err(|e| match e {
            NodeServiceError::VersionConflict {
                node_id,
                expected_version,
                actual_version,
            } => OpsError::VersionConflict {
                node_id,
                expected: expected_version,
                actual: actual_version,
                current_node: None,
            },
            other => OpsError::from(other),
        })?;

    Ok(RenameCollectionOutput { node })
}
