//! Local PersonNode seeding tests (Issue #133, ADR-037)
//!
//! ADR-037 mandates that every install — free included — seeds exactly one
//! local PersonNode (the local user, `auth_status: "local"`). On Pro upgrade
//! this node is *bound* to a Supabase identity (nodespace-sync#125), not
//! recreated. These tests verify:
//! 1. Constructing a NodeService on a fresh database seeds exactly one person.
//! 2. The seeded person carries `auth_status: "local"`.
//! 3. Re-opening the same database does NOT create a second person (idempotent).

#[cfg(test)]
mod person_seed_tests {
    use anyhow::Result;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::services::NodeService;
    use std::sync::Arc;
    use tempfile::TempDir;

    #[tokio::test]
    async fn fresh_install_seeds_exactly_one_local_person() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut store = Arc::new(SqliteStore::new(db_path).await?);
        let service = NodeService::new(&mut store).await?;

        let people = service.query_nodes_by_type("person", None).await?;
        assert_eq!(
            people.len(),
            1,
            "a fresh install must seed exactly one local PersonNode"
        );
        // Properties are stored namespaced under the node type:
        // properties["person"]["auth_status"].
        assert_eq!(
            people[0]
                .properties
                .get("person")
                .and_then(|p| p.get("auth_status"))
                .and_then(|v| v.as_str()),
            Some("local"),
            "the seeded PersonNode must be the local user (auth_status: local)"
        );

        Ok(())
    }

    #[tokio::test]
    async fn reopening_database_does_not_seed_a_second_person() -> Result<()> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");

        // First open seeds the local person.
        {
            let mut store = Arc::new(SqliteStore::new(db_path.clone()).await?);
            let service = NodeService::new(&mut store).await?;
            assert_eq!(service.query_nodes_by_type("person", None).await?.len(), 1);
        }

        // Second open of the same database must be idempotent — still one person.
        let mut store = Arc::new(SqliteStore::new(db_path).await?);
        let service = NodeService::new(&mut store).await?;
        assert_eq!(
            service.query_nodes_by_type("person", None).await?.len(),
            1,
            "re-opening an existing database must not seed a second PersonNode"
        );

        Ok(())
    }
}
