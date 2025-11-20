//! Hub-and-Spoke Schema Tests (Issue #560)
//!
//! Tests for SCHEMAFULL schema with bidirectional Record Links:
//! - Hub table (node) validation
//! - Spoke tables (task, date, schema) validation
//! - Bidirectional link creation and querying
//! - Atomic transaction rollback

#[cfg(test)]
mod hub_spoke_tests {
    use anyhow::Result;
    use nodespace_core::db::SurrealStore;
    use tempfile::TempDir;

    /// Helper to create test database
    async fn create_test_db() -> Result<(SurrealStore, TempDir)> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let store = SurrealStore::new(db_path).await?;
        Ok((store, temp_dir))
    }

    #[tokio::test]
    async fn test_hub_table_schemafull_validation() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Attempt to create node without required fields should fail
        let result = store
            .db()
            .query(
                r#"
                CREATE node:test CONTENT {
                    content: "Missing nodeType field"
                }
            "#,
            )
            .await;

        assert!(
            result.is_err(),
            "Should fail validation without required nodeType field"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_spoke_table_schemafull_validation() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Attempt to create task without required node link should fail
        let result = store
            .db()
            .query(
                r#"
                CREATE task:test CONTENT {
                    status: "todo"
                }
            "#,
            )
            .await;

        assert!(
            result.is_err(),
            "Should fail validation without required node link"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_bidirectional_links_hub_to_spoke() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create task with bidirectional links atomically
        let uuid = "550e8400-e29b-41d4-a716-446655440000";

        store
            .db()
            .query(format!(
                r#"
                BEGIN TRANSACTION;

                -- Step 1: Create spoke with reverse link to hub
                CREATE task:{uuid} CONTENT {{
                    id: type::thing('task', '{uuid}'),
                    node: type::thing('node', '{uuid}'),
                    status: 'in_progress',
                    priority: 'high'
                }};

                -- Step 2: Create hub with forward link to spoke
                CREATE node:{uuid} CONTENT {{
                    id: type::thing('node', '{uuid}'),
                    nodeType: 'task',
                    content: 'Test task',
                    data: type::thing('task', '{uuid}'),
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                }};

                COMMIT TRANSACTION;
            "#
            ))
            .await?;

        // Query hub → spoke (via data Record Link)
        let mut result = store
            .db()
            .query(format!(
                r#"
                SELECT
                    id,
                    content,
                    data.status,
                    data.priority
                FROM node:{uuid}
            "#
            ))
            .await?;

        let response: Option<serde_json::Value> = result.take(0)?;
        assert!(response.is_some(), "Should find node via hub→spoke query");

        let node = response.unwrap();
        assert_eq!(node["content"], "Test task");
        assert_eq!(node["data"]["status"], "in_progress");
        assert_eq!(node["data"]["priority"], "high");

        Ok(())
    }

    #[tokio::test]
    async fn test_bidirectional_links_spoke_to_hub() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create task with bidirectional links
        let uuid = "550e8400-e29b-41d4-a716-446655440001";

        store
            .db()
            .query(format!(
                r#"
                BEGIN TRANSACTION;

                CREATE task:{uuid} CONTENT {{
                    id: type::thing('task', '{uuid}'),
                    node: type::thing('node', '{uuid}'),
                    status: 'todo'
                }};

                CREATE node:{uuid} CONTENT {{
                    id: type::thing('node', '{uuid}'),
                    nodeType: 'task',
                    content: 'Task via spoke',
                    data: type::thing('task', '{uuid}'),
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                }};

                COMMIT TRANSACTION;
            "#
            ))
            .await?;

        // Query spoke → hub (via node reverse link)
        let mut result = store
            .db()
            .query(format!(
                r#"
                SELECT
                    *,
                    node.content AS title,
                    node.createdAt
                FROM task:{uuid}
            "#
            ))
            .await?;

        let response: Option<serde_json::Value> = result.take(0)?;
        assert!(response.is_some(), "Should find task via spoke→hub query");

        let task = response.unwrap();
        assert_eq!(task["status"], "todo");
        assert_eq!(task["title"], "Task via spoke");
        assert!(task["node"]["createdAt"].is_string());

        Ok(())
    }

    #[tokio::test]
    async fn test_atomic_transaction_rollback() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        let uuid = "550e8400-e29b-41d4-a716-446655440002";

        // Attempt transaction with invalid data (should rollback)
        let result = store
            .db()
            .query(format!(
                r#"
                BEGIN TRANSACTION;

                -- Valid spoke creation
                CREATE task:{uuid} CONTENT {{
                    id: type::thing('task', '{uuid}'),
                    node: type::thing('node', '{uuid}'),
                    status: 'todo'
                }};

                -- Invalid hub creation (missing required nodeType)
                CREATE node:{uuid} CONTENT {{
                    id: type::thing('node', '{uuid}'),
                    content: 'Missing nodeType'
                }};

                COMMIT TRANSACTION;
            "#
            ))
            .await;

        assert!(result.is_err(), "Transaction should fail and rollback");

        // Verify spoke was NOT created (transaction rolled back)
        let mut check_result = store
            .db()
            .query(format!("SELECT * FROM task:{uuid}"))
            .await?;

        let response: Option<serde_json::Value> = check_result.take(0)?;
        assert!(response.is_none(), "Spoke should not exist after rollback");

        Ok(())
    }

    #[tokio::test]
    async fn test_has_child_relation_with_fractional_order() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create parent and child nodes
        store
            .db()
            .query(
                r#"
                CREATE node:parent CONTENT {
                    id: type::thing('node', 'parent'),
                    nodeType: 'text',
                    content: 'Parent node',
                    data: NONE,
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                };

                CREATE node:child CONTENT {
                    id: type::thing('node', 'child'),
                    nodeType: 'text',
                    content: 'Child node',
                    data: NONE,
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                };
            "#,
            )
            .await?;

        // Create has_child edge with fractional order
        store
            .db()
            .query(
                r#"
                RELATE node:parent->has_child->node:child CONTENT {
                    order: 1.5,
                    createdAt: time::now(),
                    version: 1
                }
            "#,
            )
            .await?;

        // Verify edge was created with fractional order
        let mut result = store
            .db()
            .query(
                r#"
                SELECT order FROM has_child WHERE in = node:parent AND out = node:child
            "#,
            )
            .await?;

        let response: Option<serde_json::Value> = result.take(0)?;
        assert!(response.is_some(), "Should find has_child edge");

        let edge = response.unwrap();
        assert_eq!(edge["order"], 1.5);

        Ok(())
    }

    #[tokio::test]
    async fn test_mentions_relation_bidirectional() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create two nodes
        store
            .db()
            .query(
                r#"
                CREATE node:source CONTENT {
                    id: type::thing('node', 'source'),
                    nodeType: 'text',
                    content: 'Source mentions target',
                    data: NONE,
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                };

                CREATE node:target CONTENT {
                    id: type::thing('node', 'target'),
                    nodeType: 'text',
                    content: 'Target node',
                    data: NONE,
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                };
            "#,
            )
            .await?;

        // Create mention edge
        store
            .db()
            .query(
                r#"
                RELATE node:source->mentions->node:target CONTENT {
                    createdAt: time::now(),
                    context: "inline mention",
                    offset: 10
                }
            "#,
            )
            .await?;

        // Query outgoing mentions
        let mut result = store
            .db()
            .query("SELECT ->mentions->node.* FROM node:source")
            .await?;

        let response: Vec<serde_json::Value> = result.take(0)?;
        assert!(!response.is_empty(), "Should find outgoing mention");

        // Query incoming mentions (backlinks)
        let mut result = store
            .db()
            .query("SELECT <-mentions<-node.* FROM node:target")
            .await?;

        let response: Vec<serde_json::Value> = result.take(0)?;
        assert!(!response.is_empty(), "Should find incoming mention");

        Ok(())
    }

    #[tokio::test]
    async fn test_simple_nodes_without_spokes() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create text node (no spoke needed - data = NULL)
        let uuid = "550e8400-e29b-41d4-a716-446655440003";

        store
            .db()
            .query(format!(
                r#"
                CREATE node:{uuid} CONTENT {{
                    id: type::thing('node', '{uuid}'),
                    nodeType: 'text',
                    content: 'Simple text node',
                    data: NONE,
                    version: 1,
                    createdAt: time::now(),
                    modifiedAt: time::now()
                }}
            "#
            ))
            .await?;

        // Verify node was created without spoke
        let mut result = store
            .db()
            .query(format!("SELECT * FROM node:{uuid}"))
            .await?;

        let response: Option<serde_json::Value> = result.take(0)?;
        assert!(response.is_some(), "Should find text node");

        let node = response.unwrap();
        assert_eq!(node["nodeType"], "text");
        assert_eq!(node["content"], "Simple text node");
        assert!(
            node["data"].is_null(),
            "Text nodes should have NULL data (no spoke)"
        );

        Ok(())
    }
}
