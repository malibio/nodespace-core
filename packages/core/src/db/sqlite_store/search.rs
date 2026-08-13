//! `SqliteStore` methods — search concern (split from the god-object per ADR-053 prep).
use super::*;

impl SqliteStore {
    pub async fn mention_autocomplete(
        &self,
        search_query: &str,
        limit: Option<i64>,
    ) -> Result<Vec<Node>> {
        let effective_limit = limit.unwrap_or(10);
        let search_lower = format!("%{}%", search_query.to_lowercase());
        let sql = format!(
            "SELECT * FROM node WHERE title IS NOT NULL AND node_type != 'collection' AND LOWER(title) LIKE ?1 LIMIT {}",
            effective_limit
        );
        self.query_nodes_from_sql(&sql, libsql::params![search_lower])
            .await
    }
}
