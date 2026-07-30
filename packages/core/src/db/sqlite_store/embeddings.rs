//! `SqliteStore` methods — embeddings concern (split from the god-object per ADR-053 prep).
use super::*;

impl SqliteStore {
    /// Replace a node's embeddings with locally-generated vectors (`origin =
    /// 'local'`). This is what the embedding generation path uses; the cloud-push
    /// sweep reads only `'local'` rows.
    pub async fn upsert_embeddings(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
    ) -> Result<()> {
        self.upsert_embeddings_with_origin(node_id, embeddings, "local")
            .await
    }

    /// Replace a node's embeddings with vectors PULLED from another device
    /// (`origin = 'remote'`). Identical to `upsert_embeddings` except
    /// for the provenance tag, which keeps the push sweep from re-pushing a vector
    /// this device merely received (no cross-device re-push loop).
    pub async fn apply_remote_embeddings(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
    ) -> Result<()> {
        self.upsert_embeddings_with_origin(node_id, embeddings, "remote")
            .await
    }

    async fn upsert_embeddings_with_origin(
        &self,
        node_id: &str,
        embeddings: Vec<crate::models::NewEmbedding>,
        origin: &str,
    ) -> Result<()> {
        if embeddings.is_empty() {
            return Ok(());
        }

        // Replace the node's embeddings atomically across both `embedding` and the
        // `vec_embeddings` vec0 mirror. vec0 is keyed by embedding_id, so the leading
        // DELETE must clear the node's existing vec rows via its current embedding ids
        // BEFORE the rows disappear from `embedding`.
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin upsert_embeddings transaction")?;

        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for node")?;

        tx.execute(
            "DELETE FROM embedding WHERE node_id = ?1",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to delete existing embeddings")?;

        let now = Utc::now().to_rfc3339();
        let rows: Vec<(String, Vec<u8>, Vec<libsql::Value>)> = embeddings
            .into_iter()
            .map(|emb| {
                let id = uuid::Uuid::new_v4().to_string();
                let dimension = emb.vector.len() as i64;
                let vector_blob: Vec<u8> =
                    emb.vector.iter().flat_map(|f| f.to_le_bytes()).collect();
                let model_name = emb
                    .model_name
                    .unwrap_or_else(|| "nomic-embed-text-v1.5".to_string());

                let params = vec![
                    libsql::Value::Text(id.clone()),
                    libsql::Value::Text(emb.node_id.clone()),
                    libsql::Value::Blob(vector_blob.clone()),
                    libsql::Value::Integer(dimension),
                    libsql::Value::Text(model_name),
                    libsql::Value::Integer(emb.chunk_index as i64),
                    libsql::Value::Integer(emb.chunk_start as i64),
                    libsql::Value::Integer(emb.chunk_end as i64),
                    libsql::Value::Integer(emb.total_chunks as i64),
                    libsql::Value::Text(emb.content_hash),
                    libsql::Value::Integer(emb.token_count as i64),
                    libsql::Value::Text(origin.to_string()),
                    libsql::Value::Text(now.clone()),
                    libsql::Value::Text(now.clone()),
                ];

                (id, vector_blob, params)
            })
            .collect();

        // Batch into multi-row INSERTs, chunked so each statement's bound
        // parameter count stays under SQLite's ~999 ceiling.
        const EMBEDDING_CHUNK: usize = 60; // 14 params/row
        for chunk in rows.chunks(EMBEDDING_CHUNK) {
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|i| {
                    let base = i * 14;
                    format!(
                        "(?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, ?{}, 0, 0, NULL, ?{}, ?{}, ?{})",
                        base + 1,
                        base + 2,
                        base + 3,
                        base + 4,
                        base + 5,
                        base + 6,
                        base + 7,
                        base + 8,
                        base + 9,
                        base + 10,
                        base + 11,
                        base + 12,
                        base + 13,
                        base + 14
                    )
                })
                .collect();
            let sql = format!(
                "INSERT INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, origin, created_at, modified_at) VALUES {}",
                placeholders.join(", ")
            );
            let params: Vec<libsql::Value> = chunk.iter().flat_map(|(_, _, p)| p.clone()).collect();

            tx.execute(&sql, params)
                .await
                .context("Failed to insert embeddings batch")?;
        }

        // Mirror the (non-stale) vectors into vec0 for KNN search, using the same ids.
        const VEC_CHUNK: usize = 400; // 2 params/row
        for chunk in rows.chunks(VEC_CHUNK) {
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|i| format!("(?{}, ?{})", i * 2 + 1, i * 2 + 2))
                .collect();
            let sql = format!(
                "INSERT INTO vec_embeddings (embedding_id, vector) VALUES {}",
                placeholders.join(", ")
            );
            let params: Vec<libsql::Value> = chunk
                .iter()
                .flat_map(|(id, vector_blob, _)| {
                    vec![
                        libsql::Value::Text(id.clone()),
                        libsql::Value::Blob(vector_blob.clone()),
                    ]
                })
                .collect();

            tx.execute(&sql, params)
                .await
                .context("Failed to insert into vec_embeddings batch")?;
        }

        tx.commit()
            .await
            .context("Failed to commit upsert_embeddings transaction")?;

        Ok(())
    }

    /// Decode a stored embedding row into the `Embedding` model. Vectors are
    /// persisted by `upsert_embeddings` as a little-endian f32 blob; decode it
    /// back to `Vec<f32>`. Column order must match the SELECTs below.
    fn row_to_embedding(row: &libsql::Row) -> Result<crate::models::Embedding> {
        let id: String = row.get(0)?;
        let node: String = row.get(1)?;
        let vector_blob: Vec<u8> = row.get(2)?;
        let dimension: i64 = row.get(3)?;
        let model_name: String = row.get(4)?;
        let chunk_index: i64 = row.get(5)?;
        let chunk_start: i64 = row.get(6)?;
        let chunk_end: Option<i64> = row.get(7)?;
        let total_chunks: i64 = row.get(8)?;
        let content_hash: Option<String> = row.get(9)?;
        let token_count: Option<i64> = row.get(10)?;
        let stale: i64 = row.get(11)?;
        let error_count: i64 = row.get(12)?;
        let last_error: Option<String> = row.get(13)?;
        let created_at_str: String = row.get(14)?;
        let modified_at_str: String = row.get(15)?;

        let vector: Vec<f32> = vector_blob
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        let created_at = DateTime::parse_from_rfc3339(&created_at_str)
            .with_context(|| format!("Invalid embedding created_at: {}", created_at_str))?
            .with_timezone(&Utc);
        let modified_at = DateTime::parse_from_rfc3339(&modified_at_str)
            .with_context(|| format!("Invalid embedding modified_at: {}", modified_at_str))?
            .with_timezone(&Utc);

        Ok(crate::models::Embedding {
            id,
            node,
            vector,
            dimension: dimension as i32,
            model_name,
            chunk_index: chunk_index as i32,
            chunk_start: chunk_start as i32,
            chunk_end: chunk_end.map(|v| v as i32),
            total_chunks: total_chunks as i32,
            content_hash,
            token_count: token_count.map(|v| v as i32),
            stale: stale != 0,
            error_count: error_count as i32,
            last_error,
            created_at,
            modified_at,
        })
    }

    /// Read all locally-stored embedding records for a node (one per chunk),
    /// ordered by chunk index. Used by the Pro daemon's cloud push to
    /// mirror a node's vectors into Supabase pgvector.
    pub async fn get_embeddings(&self, node_id: &str) -> Result<Vec<crate::models::Embedding>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, node_id, vector, dimension, model_name, chunk_index, chunk_start, \
                 chunk_end, total_chunks, content_hash, token_count, stale, error_count, \
                 last_error, created_at, modified_at \
                 FROM embedding WHERE node_id = ?1 ORDER BY chunk_index",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to query embeddings for node")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(Self::row_to_embedding(&row)?);
        }
        Ok(out)
    }

    /// Read **locally-generated** (`origin = 'local'`) embedding records modified
    /// at or after `since`, across all nodes, ordered by `modified_at`. Drives the
    /// Pro daemon's cloud-push sweep: the daemon keeps a cursor over
    /// `modified_at` and pushes newly (re)computed vectors. Stale rows are
    /// included — the caller decides whether to skip them.
    ///
    /// The `origin = 'local'` filter excludes vectors PULLED from
    /// other devices, so a received vector is never re-pushed — without it, a
    /// pull's `modified_at = now` would re-arm this sweep and bounce the vector
    /// back to cloud, amplifying writes and (on heterogeneous devices) looping.
    ///
    /// INVARIANT: assumes every writer stores `modified_at` as a UTC rfc3339
    /// string (`Utc::now().to_rfc3339()`, as `upsert_embeddings` does). The cursor
    /// compares lexicographically, which equals chronological order ONLY for that
    /// fixed `+00:00`-offset form; a `Z`-suffixed or non-UTC timestamp would break
    /// ordering and make the sweep skip rows. Served by `idx_emb_modified`.
    pub async fn embeddings_modified_since(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<crate::models::Embedding>> {
        let mut rows = self
            .db
            .query(
                "SELECT id, node_id, vector, dimension, model_name, chunk_index, chunk_start, \
                 chunk_end, total_chunks, content_hash, token_count, stale, error_count, \
                 last_error, created_at, modified_at \
                 FROM embedding WHERE origin = 'local' AND modified_at >= ?1 \
                 ORDER BY modified_at, node_id, chunk_index",
                libsql::params![since.to_rfc3339()],
            )
            .await
            .context("Failed to query embeddings modified since")?;

        let mut out = Vec::new();
        while let Some(row) = rows.next().await? {
            out.push(Self::row_to_embedding(&row)?);
        }
        Ok(out)
    }

    pub async fn mark_root_embedding_stale(&self, node_id: &str) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin mark-stale transaction")?;

        // Stale vectors must not be searchable: drop them from the vec0 mirror.
        // `upsert_embeddings` repopulates vec0 when the node is re-embedded.
        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for stale node")?;

        tx.execute(
            "UPDATE embedding SET stale = 1, modified_at = ?1 WHERE node_id = ?2",
            libsql::params![now, node_id.to_string()],
        )
        .await
        .context("Failed to mark embedding stale")?;

        tx.commit()
            .await
            .context("Failed to commit mark-stale transaction")?;
        Ok(())
    }

    pub async fn get_stale_embedding_root_ids(
        &self,
        limit: Option<i64>,
        debounce_secs: u64,
        max_retries: u8,
    ) -> Result<Vec<String>> {
        let cutoff = Utc::now() - chrono::Duration::seconds(debounce_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let max_retries_i = max_retries as i64;

        let sql = if let Some(l) = limit {
            format!(
                "SELECT DISTINCT node_id FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at < ?2 LIMIT {}",
                l
            )
        } else {
            "SELECT DISTINCT node_id FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at < ?2".to_string()
        };

        let mut rows = self
            .db
            .query(&sql, libsql::params![max_retries_i, cutoff_str])
            .await
            .context("Failed to get stale embedding root IDs")?;

        let mut ids = Vec::new();
        while let Some(row) = rows.next().await? {
            ids.push(row.get(0)?);
        }
        Ok(ids)
    }

    pub async fn has_pending_stale_embeddings(
        &self,
        debounce_secs: u64,
        max_retries: u8,
    ) -> Result<bool> {
        let cutoff = Utc::now() - chrono::Duration::seconds(debounce_secs as i64);
        let cutoff_str = cutoff.to_rfc3339();
        let max_retries_i = max_retries as i64;

        let mut rows = self.db.query(
            "SELECT COUNT(*) FROM embedding WHERE stale = 1 AND error_count < ?1 AND modified_at >= ?2",
            libsql::params![max_retries_i, cutoff_str],
        ).await.context("Failed to check for pending stale embeddings")?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    pub async fn has_embeddings(&self, node_id: &str) -> Result<bool> {
        let mut rows = self
            .db
            .query(
                "SELECT COUNT(*) FROM embedding WHERE node_id = ?1",
                libsql::params![node_id.to_string()],
            )
            .await
            .context("Failed to check for embeddings")?;

        if let Some(row) = rows.next().await? {
            let count: i64 = row.get(0)?;
            Ok(count > 0)
        } else {
            Ok(false)
        }
    }

    pub async fn delete_embeddings(&self, node_id: &str) -> Result<()> {
        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin delete_embeddings transaction")?;

        // Clear the vec0 mirror first (keyed by embedding_id, so resolve via embedding).
        tx.execute(
            "DELETE FROM vec_embeddings WHERE embedding_id IN (SELECT id FROM embedding WHERE node_id = ?1)",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to clear vec_embeddings for node")?;

        tx.execute(
            "DELETE FROM embedding WHERE node_id = ?1",
            libsql::params![node_id.to_string()],
        )
        .await
        .context("Failed to delete embeddings")?;

        tx.commit()
            .await
            .context("Failed to commit delete_embeddings transaction")?;
        Ok(())
    }

    pub async fn record_embedding_error(
        &self,
        node_id: &str,
        error: &str,
        max_retries: u8,
    ) -> Result<()> {
        let now = Utc::now().to_rfc3339();
        let max_retries_i = max_retries as i64;

        // Increment error_count, set last_error, clear stale if error_count reaches max_retries
        self.db.execute(
            "UPDATE embedding SET error_count = error_count + 1, last_error = ?1, modified_at = ?2, stale = CASE WHEN error_count + 1 >= ?3 THEN 0 ELSE stale END WHERE node_id = ?4",
            libsql::params![error.to_string(), now, max_retries_i, node_id.to_string()],
        ).await.context("Failed to record embedding error")?;
        Ok(())
    }

    pub async fn search_embeddings(
        &self,
        query_vector: &[f32],
        limit: i64,
        threshold: Option<f64>,
    ) -> Result<Vec<crate::models::EmbeddingSearchResult>> {
        let min_score = threshold.unwrap_or(0.5);

        // vec0 KNN over the compact vector store, then JOIN back to recover node_id /
        // total_chunks. vec0 holds only non-stale vectors (see upsert/delete/mark-stale),
        // so `e.stale = 0` is a cheap defensive guard. We over-fetch chunks because many
        // chunks map to one node and results are grouped per node.
        //
        // Note: with KNN, `matching_chunks` counts a node's chunks that landed in the
        // top-k near the query — not all of its chunks (as the old full scan did). So
        // `density` now genuinely measures "fraction of the node's chunks near the query"
        // rather than always being ~1.0; it still feeds the same composite formula.
        let query_blob: Vec<u8> = query_vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        let k = (limit * EMBEDDING_KNN_OVERFETCH).max(limit);

        let mut rows = self
            .db
            .query(
                "SELECT e.node_id, e.total_chunks, v.distance \
                 FROM vec_embeddings v JOIN embedding e ON e.id = v.embedding_id \
                 WHERE v.vector MATCH ?1 AND k = ?2 AND e.stale = 0",
                libsql::params![query_blob, k],
            )
            .await
            .context("Failed to run vec0 KNN search")?;

        // Group by node_id: track max similarity, chunk counts
        let mut node_scores: HashMap<String, (f64, i64, i64)> = HashMap::new(); // node_id -> (max_sim, matching_chunks, total_chunks)

        while let Some(row) = rows.next().await? {
            let node_id: String = row.get(0)?;
            let total_chunks: i64 = row.get(1)?;
            let distance: f64 = row.get(2)?;
            // vec0 cosine distance_metric returns distance = 1 - cosine similarity
            let similarity = 1.0 - distance;

            let entry = node_scores.entry(node_id).or_insert((0.0, 0, total_chunks));
            if similarity > entry.0 {
                entry.0 = similarity;
            }
            entry.1 += 1;
        }

        // Compute composite scores and filter
        let mut results: Vec<crate::models::EmbeddingSearchResult> = Vec::new();
        for (node_id, (max_similarity, matching_chunks, total_chunks)) in node_scores {
            let density = if total_chunks > 0 {
                matching_chunks as f64 / total_chunks as f64
            } else {
                1.0
            };
            let composite_score = max_similarity * (1.0 + 0.3 * density);

            if composite_score > min_score {
                let node = self.get_node(&node_id).await?;
                results.push(crate::models::EmbeddingSearchResult {
                    node_id: node_id.clone(),
                    score: composite_score,
                    max_similarity,
                    matching_chunks,
                    node,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(limit as usize);
        Ok(results)
    }

    pub async fn search_embeddings_by_node_type(
        &self,
        query_vector: &[f32],
        node_type: &str,
        limit: i64,
        threshold: Option<f64>,
    ) -> Result<Vec<crate::models::EmbeddingSearchResult>> {
        let min_score = threshold.unwrap_or(0.5);

        // Same vec0 KNN as `search_embeddings`, with the node-type filter folded into the
        // JOIN. The type filter is applied AFTER KNN, so use a larger over-fetch to keep
        // enough surviving candidates of the requested type.
        let query_blob: Vec<u8> = query_vector.iter().flat_map(|f| f.to_le_bytes()).collect();
        let k = (limit * EMBEDDING_KNN_OVERFETCH * 5).max(limit);

        let mut rows = self
            .db
            .query(
                "SELECT e.node_id, e.total_chunks, v.distance \
             FROM vec_embeddings v \
             JOIN embedding e ON e.id = v.embedding_id \
             JOIN node n ON n.id = e.node_id \
             WHERE v.vector MATCH ?1 AND k = ?2 AND e.stale = 0 AND n.node_type = ?3",
                libsql::params![query_blob, k, node_type.to_string()],
            )
            .await
            .context("Failed to run typed vec0 KNN search")?;

        let mut node_scores: HashMap<String, (f64, i64, i64)> = HashMap::new();

        while let Some(row) = rows.next().await? {
            let node_id: String = row.get(0)?;
            let total_chunks: i64 = row.get(1)?;
            let distance: f64 = row.get(2)?;
            let similarity = 1.0 - distance;

            let entry = node_scores.entry(node_id).or_insert((0.0, 0, total_chunks));
            if similarity > entry.0 {
                entry.0 = similarity;
            }
            entry.1 += 1;
        }

        let mut results = Vec::new();
        for (node_id, (max_similarity, matching_chunks, total_chunks)) in node_scores {
            let density = if total_chunks > 0 {
                matching_chunks as f64 / total_chunks as f64
            } else {
                1.0
            };
            let composite_score = max_similarity * (1.0 + 0.3 * density);

            if composite_score > min_score {
                let node = self.get_node(&node_id).await?;
                results.push(crate::models::EmbeddingSearchResult {
                    node_id: node_id.clone(),
                    score: composite_score,
                    max_similarity,
                    matching_chunks,
                    node,
                });
            }
        }

        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        // The node_type filter runs after the global top-k, so a type that is rare
        // relative to the corpus can be crowded out of the KNN window — surfacing as
        // fewer than `limit` results. Surface that as a debug signal rather than failing
        // silently; raising EMBEDDING_KNN_OVERFETCH is the lever if recall suffers.
        if (results.len() as i64) < limit {
            tracing::debug!(
                node_type,
                returned = results.len(),
                limit,
                k,
                "typed embedding search returned fewer than `limit` results; node_type may be under-represented in the KNN window"
            );
        }

        results.truncate(limit as usize);
        Ok(results)
    }

    pub async fn bm25_search_roots(
        &self,
        query: &str,
        candidate_limit: i64,
    ) -> Result<HashSet<String>> {
        let tokens: Vec<String> = query
            .split_whitespace()
            .map(|t| {
                t.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_lowercase()
            })
            .filter(|t| !t.is_empty() && !BM25_STOP_WORDS.contains(&t.as_str()))
            .take(BM25_MAX_TOKENS)
            .collect();

        if tokens.is_empty() {
            return Ok(HashSet::new());
        }

        // Build FTS5 query: "token1" OR "token2" OR ...
        let fts_query = tokens
            .iter()
            .map(|t| format!("\"{}\"", t.replace('"', "")))
            .collect::<Vec<_>>()
            .join(" OR ");

        let sql = format!(
            "SELECT n.id FROM node n JOIN node_fts f ON f.id = n.id WHERE node_fts MATCH ?1 ORDER BY rank LIMIT {}",
            candidate_limit
        );

        let mut rows = self
            .db
            .query(&sql, libsql::params![fts_query])
            .await
            .context("Failed to execute BM25 search")?;

        let mut matching_ids: Vec<String> = Vec::new();
        while let Some(row) = rows.next().await? {
            matching_ids.push(row.get(0)?);
        }

        if matching_ids.is_empty() {
            return Ok(HashSet::new());
        }

        // Resolve every match to its EMBEDDING root in one set operation: seed the
        // recursive CTE with the whole match set (depth 0), walk `has_child`
        // parents, and for each seed keep the deepest ancestor reached.
        //
        // The walk stops BELOW a non-embeddable *container* (a `date` page, but also
        // a `task`/`collection`/`agent-guidance` — see `NON_EMBEDDABLE_CONTAINER_TYPES`):
        // such a node is a non-embeddable organizational root whose children each
        // carry their own content and are their own embedding roots. This mirrors
        // the behavior probe in `NodeService::get_embedding_root_id`, which SQL
        // can't run — so we refuse to traverse INTO any parent of a container type.
        // Without this, a bullet hit resolved up to the container (e.g. the date or
        // task node), which the default `Knowledge` search scope excludes — so that
        // content was silently unfindable. The parity of this list with the
        // non-embeddable child-bearing behaviors is enforced by
        // `container_type_parity_tests::non_embeddable_container_types_match_behaviors`.
        let placeholders: Vec<String> = (1..=matching_ids.len())
            .map(|i| format!("?{}", i))
            .collect();
        // Container types come from a hardcoded const of static identifiers — no
        // user input — so inlining them as SQL literals is injection-safe.
        let container_types: Vec<String> = crate::behaviors::NON_EMBEDDABLE_CONTAINER_TYPES
            .iter()
            .map(|t| format!("'{t}'"))
            .collect();
        let sql = format!(
            r#"WITH RECURSIVE ancestors(seed_id, node_id, depth) AS (
                SELECT id, id, 0 FROM node WHERE id IN ({})
                UNION ALL
                SELECT a.seed_id, r.in_node, a.depth + 1 FROM relationship r
                JOIN ancestors a ON r.out_node = a.node_id
                JOIN node pn ON pn.id = r.in_node
                WHERE r.relationship_type = 'has_child' AND a.depth < 100
                  AND pn.node_type NOT IN ({})
            )
            SELECT seed_id, node_id FROM ancestors a
            WHERE a.depth = (SELECT MAX(depth) FROM ancestors WHERE seed_id = a.seed_id)"#,
            placeholders.join(", "),
            container_types.join(", ")
        );

        let params: Vec<libsql::Value> = matching_ids
            .iter()
            .map(|id| libsql::Value::Text(id.clone()))
            .collect();

        let mut rows = self
            .db
            .query(&sql, params)
            .await
            .context("Failed to resolve BM25 roots")?;

        let mut candidate_roots: HashSet<String> = HashSet::new();
        while let Some(row) = rows.next().await? {
            let root_id: String = row.get(1)?;
            candidate_roots.insert(root_id);
        }

        if candidate_roots.is_empty() {
            return Ok(HashSet::new());
        }

        // Defensive existence re-check, batched into a single `id IN (...)` query
        // instead of a per-root `get_node` round-trip.
        let candidate_vec: Vec<String> = candidate_roots.into_iter().collect();
        let existing = self.get_nodes_by_ids(&candidate_vec).await?;
        Ok(candidate_vec
            .into_iter()
            .filter(|id| existing.contains_key(id))
            .collect())
    }

    pub async fn create_stale_embedding_marker(&self, node_id: &str) -> Result<()> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        // Deliberately NOT mirrored into vec_embeddings: this is a stale (stale=1)
        // placeholder with a dummy vector and must never surface in KNN results.
        // Unit vector [1, 0, 0, ...] as 768×f32 LE bytes
        let mut vector_bytes = vec![0u8; 768 * 4];
        vector_bytes[0..4].copy_from_slice(&1.0f32.to_le_bytes());

        self.db.execute(
            "INSERT OR IGNORE INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, created_at, modified_at) VALUES (?1, ?2, ?3, 768, 'nomic-embed-text-v1.5', 0, 0, NULL, 1, NULL, NULL, 1, 0, NULL, ?4, ?5)",
            libsql::params![id, node_id.to_string(), vector_bytes, now.clone(), now],
        ).await.context("Failed to create stale embedding marker")?;
        Ok(())
    }

    pub async fn create_stale_embedding_markers_bulk(&self, node_ids: &[String]) -> Result<usize> {
        if node_ids.is_empty() {
            return Ok(0);
        }

        let start = std::time::Instant::now();
        let now = Utc::now().to_rfc3339();
        // Stale placeholders are deliberately NOT mirrored into vec_embeddings (see
        // create_stale_embedding_marker) — they must never appear in KNN results.
        let mut vector_bytes = vec![0u8; 768 * 4];
        vector_bytes[0..4].copy_from_slice(&1.0f32.to_le_bytes());

        let tx = self
            .db
            .transaction()
            .await
            .context("Failed to begin markers transaction")?;

        // Batch into multi-row INSERTs, chunked so each statement's bound
        // parameter count stays under SQLite's ~999 ceiling (5 params/row).
        const ID_CHUNK: usize = 180;
        for chunk in node_ids.chunks(ID_CHUNK) {
            let placeholders: Vec<String> = (0..chunk.len())
                .map(|i| {
                    let base = i * 5;
                    format!(
                        "(?{}, ?{}, ?{}, 768, 'nomic-embed-text-v1.5', 0, 0, NULL, 1, NULL, NULL, 1, 0, NULL, ?{}, ?{})",
                        base + 1,
                        base + 2,
                        base + 3,
                        base + 4,
                        base + 5
                    )
                })
                .collect();
            let sql = format!(
                "INSERT OR IGNORE INTO embedding (id, node_id, vector, dimension, model_name, chunk_index, chunk_start, chunk_end, total_chunks, content_hash, token_count, stale, error_count, last_error, created_at, modified_at) VALUES {}",
                placeholders.join(", ")
            );

            let mut params: Vec<libsql::Value> = Vec::with_capacity(chunk.len() * 5);
            for node_id in chunk {
                params.push(libsql::Value::Text(uuid::Uuid::new_v4().to_string()));
                params.push(libsql::Value::Text(node_id.clone()));
                params.push(libsql::Value::Blob(vector_bytes.clone()));
                params.push(libsql::Value::Text(now.clone()));
                params.push(libsql::Value::Text(now.clone()));
            }

            tx.execute(&sql, params)
                .await
                .context("Failed to insert stale embedding markers batch")?;
        }
        tx.commit()
            .await
            .context("Failed to commit markers transaction")?;

        tracing::debug!(
            "create_stale_embedding_markers_bulk: {} markers in {:?}",
            node_ids.len(),
            start.elapsed()
        );

        Ok(node_ids.len())
    }
}
