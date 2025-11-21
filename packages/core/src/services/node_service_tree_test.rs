//! Integration Tests for Recursive Tree Fetching
//!
//! Validates the `get_children_tree` functionality which uses SurrealDB's
//! recursive FETCH capabilities to retrieve nested node structures.

#[cfg(test)]
mod tree_tests {
    use crate::db::SurrealStore;
    use crate::models::Node;
    use crate::services::NodeService;
    use serde_json::json;
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Helper to create test services
    async fn create_test_services() -> (Arc<NodeService>, Arc<SurrealStore>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let store = Arc::new(SurrealStore::new(db_path).await.unwrap());
        let node_service = Arc::new(NodeService::new(store.clone()).unwrap());

        (node_service, store, temp_dir)
    }

    #[tokio::test]
    async fn test_get_children_tree_recursive() {
        let (service, store, _temp) = create_test_services().await;

        // 1. Create a root node
        let root = Node::new("text".to_string(), "Root".to_string(), json!({}));
        let root_id = root.id.clone();
        service.create_node(root).await.unwrap();

        // 2. Create a child node (Level 1)
        let child1 = Node::new("text".to_string(), "Child 1".to_string(), json!({}));
        let child1_id = child1.id.clone();
        service.create_node(child1).await.unwrap();

        // Link child1 to root
        store
            .move_node(&child1_id, Some(&root_id), None)
            .await
            .unwrap();

        // 3. Create a grandchild node (Level 2)
        let grandchild = Node::new("text".to_string(), "Grandchild".to_string(), json!({}));
        let grandchild_id = grandchild.id.clone();
        service.create_node(grandchild).await.unwrap();

        // Link grandchild to child1
        store
            .move_node(&grandchild_id, Some(&child1_id), None)
            .await
            .unwrap();

        // 4. Create a great-grandchild node (Level 3)
        let great_grandchild = Node::new(
            "text".to_string(),
            "Great Grandchild".to_string(),
            json!({}),
        );
        let great_grandchild_id = great_grandchild.id.clone();
        service.create_node(great_grandchild).await.unwrap();

        // Link great-grandchild to grandchild
        store
            .move_node(&great_grandchild_id, Some(&grandchild_id), None)
            .await
            .unwrap();

        // 5. Fetch the tree starting from root
        let tree = service
            .get_children_tree(&root_id)
            .await
            .unwrap()
            .expect("Root node not found");

        // 6. Verify the structure
        assert_eq!(tree.node.id, root_id);
        assert_eq!(tree.children.len(), 1);

        let fetched_child1 = &tree.children[0];
        assert_eq!(fetched_child1.node.id, child1_id);
        assert_eq!(fetched_child1.children.len(), 1);

        let fetched_grandchild = &fetched_child1.children[0];
        assert_eq!(fetched_grandchild.node.id, grandchild_id);
        assert_eq!(fetched_grandchild.children.len(), 1);

        let fetched_great_grandchild = &fetched_grandchild.children[0];
        assert_eq!(fetched_great_grandchild.node.id, great_grandchild_id);
        assert_eq!(fetched_great_grandchild.children.len(), 0);
    }

    #[tokio::test]
    async fn test_get_children_tree_ordering() {
        let (service, store, _temp) = create_test_services().await;

        // Create root
        let root = Node::new("text".to_string(), "Root".to_string(), json!({}));
        let root_id = root.id.clone();
        service.create_node(root).await.unwrap();

        // Create 3 children
        let child1 = Node::new("text".to_string(), "Child 1".to_string(), json!({}));
        let child1_id = child1.id.clone();
        service.create_node(child1).await.unwrap();

        let child2 = Node::new("text".to_string(), "Child 2".to_string(), json!({}));
        let child2_id = child2.id.clone();
        service.create_node(child2).await.unwrap();

        let child3 = Node::new("text".to_string(), "Child 3".to_string(), json!({}));
        let child3_id = child3.id.clone();
        service.create_node(child3).await.unwrap();

        // Link them to root in specific order: 2 -> 1 -> 3
        // First add child 3
        store
            .move_node(&child3_id, Some(&root_id), None)
            .await
            .unwrap();

        // Add child 1 before child 3
        store
            .move_node(&child1_id, Some(&root_id), Some(&child3_id))
            .await
            .unwrap();

        // Add child 2 before child 1
        store
            .move_node(&child2_id, Some(&root_id), Some(&child1_id))
            .await
            .unwrap();

        // Fetch tree
        let tree = service.get_children_tree(&root_id).await.unwrap().unwrap();

        // Verify order
        assert_eq!(tree.children.len(), 3);
        assert_eq!(
            tree.children[0].node.id, child2_id,
            "First child should be Child 2"
        );
        assert_eq!(
            tree.children[1].node.id, child1_id,
            "Second child should be Child 1"
        );
        assert_eq!(
            tree.children[2].node.id, child3_id,
            "Third child should be Child 3"
        );
    }
}
