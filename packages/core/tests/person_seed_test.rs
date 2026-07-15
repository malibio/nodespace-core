//! Local PersonNode seeding tests (ADR-037)
//!
//! ADR-037 mandates that every install — free included — seeds exactly one
//! local PersonNode (the local user). On Pro upgrade this node is bound to a
//! Supabase identity, not recreated. These tests verify:
//! 1. Constructing a NodeService on a fresh database seeds exactly one person.
//! 2. The seeded person has no auth_status (that lives on DatabaseSettingsNode).
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
        // auth_status lives on DatabaseSettingsNode, not on PersonNode
        assert!(
            people[0]
                .properties
                .get("person")
                .and_then(|p| p.get("auth_status"))
                .is_none(),
            "seeded PersonNode must not carry auth_status — that belongs on DatabaseSettingsNode"
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
