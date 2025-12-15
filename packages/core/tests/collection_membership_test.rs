//! Collection Membership Tests (Issue #756)
//!
//! Integration tests for the collection system's membership operations.
//!
//! ## Collection System Overview
//!
//! Collections provide flexible, hierarchical organization for nodes:
//! - Many-to-many membership (nodes can belong to multiple collections)
//! - DAG structure (directed acyclic graph, not strictly a tree)
//! - Path-based navigation (e.g., "hr:policy:vacation")
//!
//! ## Key Concepts
//!
//! ### member_of Edge Direction
//! - Edge direction: member node → collection node
//! - Query "what collections does X belong to": SELECT ->member_of->node FROM node:X
//! - Query "what nodes are in collection Y": SELECT <-member_of<-node FROM node:Y
//!
//! ### Path Resolution
//! Collections are organized using colon-separated paths:
//! - "hr" → Top-level HR collection
//! - "hr:policy" → "policy" collection under "hr"
//! - "hr:policy:vacation" → "vacation" under "hr:policy"
//!
//! ## Test Coverage
//! - member_of edge creation and querying
//! - Path parsing and validation
//! - Adding/removing collection memberships
//! - Querying nodes by collection
//! - Collection hierarchy traversal

#[cfg(test)]
mod collection_membership_tests {
    use anyhow::Result;
    use nodespace_core::db::SurrealStore;
    use serde::Deserialize;
    use surrealdb::sql::Thing;
    use tempfile::TempDir;

    /// Test helper struct for edge results
    #[derive(Debug, Deserialize)]
    struct MemberOfEdge {
        #[serde(rename = "in")]
        in_node: Thing,
        #[serde(rename = "out")]
        out_node: Thing,
    }

    /// Helper to create test database
    async fn create_test_db() -> Result<(SurrealStore, TempDir)> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let store = SurrealStore::new(db_path).await?;
        Ok((store, temp_dir))
    }

    #[tokio::test]
    async fn test_member_of_edge_creation() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create a collection node
        store
            .db()
            .query(
                r#"
                CREATE node:collection1 CONTENT {
                    node_type: 'collection',
                    content: 'HR Collection',
                    data: NONE,
                    version: 1,
                    properties: { description: 'Human resources documents' },
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Create a text node
        store
            .db()
            .query(
                r#"
                CREATE node:doc1 CONTENT {
                    node_type: 'text',
                    content: 'Employee Handbook',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Create member_of edge (direction: member → collection)
        store
            .db()
            .query(
                r#"
                RELATE node:doc1->member_of->node:collection1 CONTENT {
                    created_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Verify edge was created by querying outward direction
        let result = store
            .db()
            .query("SELECT * FROM member_of WHERE in = node:doc1")
            .await?;

        let mut result = result.check()?;
        let edges: Vec<MemberOfEdge> = result.take(0)?;
        assert_eq!(edges.len(), 1, "Should find one member_of edge");
        assert_eq!(edges[0].in_node.id.to_string(), "doc1");
        assert_eq!(edges[0].out_node.id.to_string(), "collection1");

        Ok(())
    }

    #[tokio::test]
    async fn test_query_node_collections() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create multiple collections
        store
            .db()
            .query(
                r#"
                CREATE node:coll_hr CONTENT {
                    node_type: 'collection',
                    content: 'HR',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:coll_policy CONTENT {
                    node_type: 'collection',
                    content: 'Policy',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc1 CONTENT {
                    node_type: 'text',
                    content: 'Vacation Policy',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Add doc1 to both collections
        store
            .db()
            .query(
                r#"
                RELATE node:doc1->member_of->node:coll_hr CONTENT {
                    created_at: time::now()
                };
                RELATE node:doc1->member_of->node:coll_policy CONTENT {
                    created_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Query what collections doc1 belongs to (count edges instead of selecting Thing)
        let result = store
            .db()
            .query("SELECT count() FROM member_of WHERE in = node:doc1 GROUP ALL")
            .await?;

        let mut result = result.check()?;
        let count_result: Option<serde_json::Value> = result.take(0)?;
        let count = count_result.and_then(|v| v["count"].as_u64()).unwrap_or(0);
        assert_eq!(count, 2, "Document should belong to 2 collections");

        Ok(())
    }

    #[tokio::test]
    async fn test_query_collection_members() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create a collection and multiple documents
        store
            .db()
            .query(
                r#"
                CREATE node:coll_team CONTENT {
                    node_type: 'collection',
                    content: 'Team Docs',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc_a CONTENT {
                    node_type: 'text',
                    content: 'Document A',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc_b CONTENT {
                    node_type: 'text',
                    content: 'Document B',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc_c CONTENT {
                    node_type: 'text',
                    content: 'Document C (not in collection)',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Add doc_a and doc_b to collection (but not doc_c)
        store
            .db()
            .query(
                r#"
                RELATE node:doc_a->member_of->node:coll_team CONTENT {
                    created_at: time::now()
                };
                RELATE node:doc_b->member_of->node:coll_team CONTENT {
                    created_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Query members of collection (count edges instead of selecting Thing)
        let result = store
            .db()
            .query("SELECT count() FROM member_of WHERE out = node:coll_team GROUP ALL")
            .await?;

        let mut result = result.check()?;
        let count_result: Option<serde_json::Value> = result.take(0)?;
        let count = count_result.and_then(|v| v["count"].as_u64()).unwrap_or(0);
        assert_eq!(count, 2, "Collection should have 2 members");

        Ok(())
    }

    #[tokio::test]
    async fn test_remove_membership() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create collection and document
        store
            .db()
            .query(
                r#"
                CREATE node:coll_remove CONTENT {
                    node_type: 'collection',
                    content: 'Collection for removal test',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc_remove CONTENT {
                    node_type: 'text',
                    content: 'Document to remove',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Add membership
        store
            .db()
            .query(
                r#"
                RELATE node:doc_remove->member_of->node:coll_remove CONTENT {
                    created_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Verify membership exists (count edges)
        let result = store
            .db()
            .query("SELECT count() FROM member_of WHERE in = node:doc_remove GROUP ALL")
            .await?;

        let mut result = result.check()?;
        let count_result: Option<serde_json::Value> = result.take(0)?;
        let count = count_result.and_then(|v| v["count"].as_u64()).unwrap_or(0);
        assert_eq!(count, 1, "Membership should exist");

        // Remove membership
        store
            .db()
            .query("DELETE member_of WHERE in = node:doc_remove AND out = node:coll_remove")
            .await?
            .check()?;

        // Verify membership is removed (count should be 0)
        let result = store
            .db()
            .query("SELECT count() FROM member_of WHERE in = node:doc_remove GROUP ALL")
            .await?;

        let mut result = result.check()?;
        let count_result: Option<serde_json::Value> = result.take(0)?;
        // When no rows match GROUP ALL, the result is empty/none
        let count = count_result.and_then(|v| v["count"].as_u64()).unwrap_or(0);
        assert_eq!(count, 0, "Membership should be removed");

        Ok(())
    }

    #[tokio::test]
    async fn test_nested_collections() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create nested collection hierarchy: hr -> policy -> vacation
        store
            .db()
            .query(
                r#"
                CREATE node:coll_hr CONTENT {
                    node_type: 'collection',
                    content: 'HR',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:coll_policy CONTENT {
                    node_type: 'collection',
                    content: 'Policy',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:coll_vacation CONTENT {
                    node_type: 'collection',
                    content: 'Vacation',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Create parent-child relationships using has_child edges
        // This establishes the collection hierarchy
        store
            .db()
            .query(
                r#"
                RELATE node:coll_hr->has_child->node:coll_policy CONTENT {
                    order: 1.0,
                    created_at: time::now(),
                    version: 1
                };

                RELATE node:coll_policy->has_child->node:coll_vacation CONTENT {
                    order: 1.0,
                    created_at: time::now(),
                    version: 1
                };
                "#,
            )
            .await?
            .check()?;

        // Verify hierarchy by traversing from root
        let result = store
            .db()
            .query("SELECT out.content FROM has_child WHERE in = node:coll_hr")
            .await?;

        let mut result = result.check()?;
        let children: Vec<serde_json::Value> = result.take(0)?;
        assert_eq!(children.len(), 1, "HR should have 1 child (policy)");

        // Verify policy -> vacation link
        let result = store
            .db()
            .query("SELECT out.content FROM has_child WHERE in = node:coll_policy")
            .await?;

        let mut result = result.check()?;
        let grandchildren: Vec<serde_json::Value> = result.take(0)?;
        assert_eq!(
            grandchildren.len(),
            1,
            "Policy should have 1 child (vacation)"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_collection_with_multiple_node_types() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create a collection
        store
            .db()
            .query(
                r#"
                CREATE node:coll_mixed CONTENT {
                    node_type: 'collection',
                    content: 'Mixed Content Collection',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Create nodes of different types
        let uuid = "550e8400-e29b-41d4-a716-446655440100";
        store
            .db()
            .query(format!(
                r#"
                -- Text node
                CREATE node:text_node CONTENT {{
                    node_type: 'text',
                    content: 'Text document',
                    data: NONE,
                    version: 1,
                    properties: {{}},
                    created_at: time::now(),
                    modified_at: time::now()
                }};

                -- Task node with spoke
                CREATE task:`{uuid}` CONTENT {{
                    node: type::thing('node', '{uuid}'),
                    status: 'open',
                    priority: 'high'
                }};

                CREATE node:`{uuid}` CONTENT {{
                    node_type: 'task',
                    content: 'Task item',
                    data: type::thing('task', '{uuid}'),
                    version: 1,
                    properties: {{}},
                    created_at: time::now(),
                    modified_at: time::now()
                }};
                "#
            ))
            .await?
            .check()?;

        // Add both to collection
        store
            .db()
            .query(format!(
                r#"
                RELATE node:text_node->member_of->node:coll_mixed CONTENT {{
                    created_at: time::now()
                }};
                RELATE node:`{uuid}`->member_of->node:coll_mixed CONTENT {{
                    created_at: time::now()
                }};
                "#
            ))
            .await?
            .check()?;

        // Query collection members with their types
        let result = store
            .db()
            .query("SELECT in.node_type AS node_type FROM member_of WHERE out = node:coll_mixed")
            .await?;

        let mut result = result.check()?;
        let members: Vec<serde_json::Value> = result.take(0)?;
        assert_eq!(
            members.len(),
            2,
            "Collection should have 2 members of different types"
        );

        // Verify we have both types
        let types: Vec<&str> = members
            .iter()
            .filter_map(|m| m["node_type"].as_str())
            .collect();
        assert!(types.contains(&"text"), "Should contain text node");
        assert!(types.contains(&"task"), "Should contain task node");

        Ok(())
    }

    #[tokio::test]
    async fn test_membership_idempotency() -> Result<()> {
        let (store, _temp_dir) = create_test_db().await?;

        // Create collection and document
        store
            .db()
            .query(
                r#"
                CREATE node:coll_idem CONTENT {
                    node_type: 'collection',
                    content: 'Idempotency Test',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };

                CREATE node:doc_idem CONTENT {
                    node_type: 'text',
                    content: 'Test Document',
                    data: NONE,
                    version: 1,
                    properties: {},
                    created_at: time::now(),
                    modified_at: time::now()
                };
                "#,
            )
            .await?
            .check()?;

        // Add membership twice - should not create duplicate edges
        // Using IF NOT EXISTS pattern
        store
            .db()
            .query(
                r#"
                LET $existing = SELECT * FROM member_of WHERE in = node:doc_idem AND out = node:coll_idem;
                IF array::len($existing) == 0 THEN {
                    RELATE node:doc_idem->member_of->node:coll_idem CONTENT {
                        created_at: time::now()
                    }
                } END;
                "#,
            )
            .await?
            .check()?;

        // Try to add again
        store
            .db()
            .query(
                r#"
                LET $existing = SELECT * FROM member_of WHERE in = node:doc_idem AND out = node:coll_idem;
                IF array::len($existing) == 0 THEN {
                    RELATE node:doc_idem->member_of->node:coll_idem CONTENT {
                        created_at: time::now()
                    }
                } END;
                "#,
            )
            .await?
            .check()?;

        // Verify only one edge exists (count)
        let result = store
            .db()
            .query("SELECT count() FROM member_of WHERE in = node:doc_idem AND out = node:coll_idem GROUP ALL")
            .await?;

        let mut result = result.check()?;
        let count_result: Option<serde_json::Value> = result.take(0)?;
        let count = count_result.and_then(|v| v["count"].as_u64()).unwrap_or(0);
        assert_eq!(count, 1, "Should have exactly one membership edge");

        Ok(())
    }
}
