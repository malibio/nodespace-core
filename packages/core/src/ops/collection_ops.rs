//! Collection Operations
//!
//! Thin orchestration wrappers over `CollectionService`. Extracted from the daemon
//! layer so both the gRPC adapter and future callers share the same logic.

use crate::models::{Node, NodeUpdate};
use crate::ops::OpsError;
use crate::services::collection_service::deterministic_collection_id;
use crate::services::{
    CollectionService, CreateNodeParams, InsertPositionOwned, NodeService, NodeServiceError,
};
use std::sync::Arc;

// ============================================================================
// Input / Output types
// ============================================================================

#[derive(Debug)]
pub struct GetAllCollectionsInput;

#[derive(Debug)]
pub struct CollectionEntry {
    pub node: Node,
    pub member_count: usize,
    pub parent_collection_ids: Vec<String>,
}

#[derive(Debug)]
pub struct GetAllCollectionsOutput {
    pub collections: Vec<CollectionEntry>,
}

#[derive(Debug)]
pub struct GetCollectionMembersInput {
    pub collection_id: String,
}

#[derive(Debug)]
pub struct GetCollectionMembersOutput {
    pub members: Vec<Node>,
    pub collection_id: String,
}

#[derive(Debug)]
pub struct GetCollectionMembersRecursiveInput {
    pub collection_id: String,
}

#[derive(Debug)]
pub struct GetCollectionMembersRecursiveOutput {
    pub members: Vec<Node>,
    pub collection_id: String,
}

#[derive(Debug)]
pub struct GetNodeCollectionsInput {
    pub node_id: String,
}

#[derive(Debug)]
pub struct GetNodeCollectionsOutput {
    pub collection_ids: Vec<String>,
}

#[derive(Debug)]
pub struct AddNodeToCollectionInput {
    pub node_id: String,
    pub collection_id: String,
}

#[derive(Debug)]
pub struct AddNodeToCollectionOutput;

#[derive(Debug)]
pub struct AddNodeToCollectionByPathInput {
    pub node_id: String,
    pub collection_path: String,
}

#[derive(Debug)]
pub struct AddNodeToCollectionByPathOutput {
    pub collection_id: String,
}

#[derive(Debug)]
pub struct RemoveNodeFromCollectionInput {
    pub node_id: String,
    pub collection_id: String,
}

#[derive(Debug)]
pub struct RemoveNodeFromCollectionOutput;

#[derive(Debug)]
pub struct FindCollectionByPathInput {
    pub collection_path: String,
}

#[derive(Debug)]
pub struct FindCollectionByPathOutput {
    pub collection: Option<Node>,
}

#[derive(Debug)]
pub struct GetCollectionByNameInput {
    pub name: String,
}

#[derive(Debug)]
pub struct GetCollectionByNameOutput {
    pub collection: Option<Node>,
}

#[derive(Debug)]
pub struct CreateCollectionInput {
    pub name: String,
    pub description: String,
}

#[derive(Debug)]
pub struct CreateCollectionOutput {
    pub collection_id: String,
}

#[derive(Debug)]
pub struct RenameCollectionInput {
    pub collection_id: String,
    pub new_name: String,
    pub version: i64,
}

#[derive(Debug)]
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
    let store = node_service.store();
    let collection_service = CollectionService::new(store, node_service);
    let member_ids = collection_service
        .get_collection_members_recursive(&input.collection_id)
        .await
        .map_err(OpsError::from)?;

    let nodes_map = store
        .get_nodes_by_ids(&member_ids)
        .await
        .map_err(|e| OpsError::Internal(format!("Failed to batch fetch nodes: {}", e)))?;

    // Preserve ordering from member_ids; filter out missing entries.
    let members: Vec<Node> = member_ids
        .iter()
        .filter_map(|id| nodes_map.get(id).cloned())
        .collect();

    Ok(GetCollectionMembersRecursiveOutput {
        collection_id: input.collection_id,
        members,
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
        return Err(OpsError::AlreadyExists {
            id: input.name.clone(),
        });
    }

    let properties = if input.description.is_empty() {
        serde_json::json!({})
    } else {
        serde_json::json!({ "description": input.description })
    };

    // Deterministic id from the (globally-unique) name so a UI-created collection
    // converges with the same-named collection created on another device or by import
    // (`CollectionService::create_collection` derives the same id) instead of minting a
    // random UUID that syncs up as a duplicate. The name-existence check above already
    // rejects a local duplicate.
    let collection_id = node_service
        .create_node_with_parent(CreateNodeParams {
            id: Some(deterministic_collection_id(&input.name)),
            node_type: "collection".to_string(),
            content: input.name,
            parent_id: None,
            position: InsertPositionOwned::End,
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
            return Err(OpsError::AlreadyExists {
                id: input.new_name.clone(),
            });
        }
    }

    let update = NodeUpdate {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SqliteStore;
    use crate::services::NodeService;
    use std::sync::Arc;
    use tempfile::TempDir;

    async fn make_service() -> (Arc<NodeService>, TempDir) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let mut store = Arc::new(SqliteStore::new(db_path).await.unwrap());
        let svc = Arc::new(NodeService::new(&mut store).await.unwrap());
        (svc, tmp)
    }

    #[tokio::test]
    async fn create_collection_duplicate_name_returns_already_exists() {
        let (svc, _tmp) = make_service().await;

        let input = CreateCollectionInput {
            name: "my-collection".to_string(),
            description: String::new(),
        };
        create_collection(&svc, input).await.unwrap();

        let dup = CreateCollectionInput {
            name: "my-collection".to_string(),
            description: String::new(),
        };
        let err = create_collection(&svc, dup).await.unwrap_err();
        assert!(
            matches!(err, OpsError::AlreadyExists { ref id } if id == "my-collection"),
            "expected AlreadyExists, got {:?}",
            err
        );
    }

    /// A collection created via the UI/gRPC op on one store must get the SAME id as the
    /// same-named collection created via the import path (`resolve_path`) on another
    /// store, so two devices converge instead of syncing up duplicate collection nodes.
    #[tokio::test]
    async fn ui_created_collection_id_matches_import_path_across_stores() {
        let (svc_a, _a) = make_service().await;
        let (svc_b, _b) = make_service().await;

        let ui = create_collection(
            &svc_a,
            CreateCollectionInput {
                name: "Architecture".to_string(),
                description: String::new(),
            },
        )
        .await
        .unwrap();

        let imported = CollectionService::new(svc_b.store(), &svc_b)
            .resolve_path("Architecture")
            .await
            .unwrap();

        assert_eq!(
            ui.collection_id, imported.leaf.id,
            "UI-created and import-created collections of the same name converge on one id"
        );
        assert_eq!(
            ui.collection_id,
            deterministic_collection_id("Architecture")
        );
    }

    #[tokio::test]
    async fn rename_collection_to_same_name_succeeds() {
        let (svc, _tmp) = make_service().await;

        let output = create_collection(
            &svc,
            CreateCollectionInput {
                name: "orig".to_string(),
                description: String::new(),
            },
        )
        .await
        .unwrap();

        let node = svc.get_node(&output.collection_id).await.unwrap().unwrap();
        let result = rename_collection(
            &svc,
            RenameCollectionInput {
                collection_id: output.collection_id.clone(),
                new_name: "orig".to_string(),
                version: node.version,
            },
        )
        .await;
        assert!(result.is_ok(), "self-rename should succeed");
    }

    #[tokio::test]
    async fn rename_collection_to_existing_name_returns_already_exists() {
        let (svc, _tmp) = make_service().await;

        create_collection(
            &svc,
            CreateCollectionInput {
                name: "alpha".to_string(),
                description: String::new(),
            },
        )
        .await
        .unwrap();

        let beta = create_collection(
            &svc,
            CreateCollectionInput {
                name: "beta".to_string(),
                description: String::new(),
            },
        )
        .await
        .unwrap();

        let node = svc.get_node(&beta.collection_id).await.unwrap().unwrap();
        let err = rename_collection(
            &svc,
            RenameCollectionInput {
                collection_id: beta.collection_id,
                new_name: "alpha".to_string(),
                version: node.version,
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(err, OpsError::AlreadyExists { ref id } if id == "alpha"),
            "expected AlreadyExists, got {:?}",
            err
        );
    }
}
