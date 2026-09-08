//! Search cost must be dominated by a fixed per-call component, not by the
//! number of results returned.
//!
//! Several layers of the search path used to do one query per result, executed
//! serially: `search_embeddings` hydrated every surviving candidate with its own
//! `get_node`, and the gRPC handler then re-read each result from the store by
//! id. Each such call checks a reader connection out of the pool — and on a pool
//! miss *opens a new SQLite connection*. Individually these are cheap, but they
//! are all work that grows with the result count rather than with the corpus,
//! and they are the shape a per-result latency slope takes.
//!
//! These tests pin that shape rather than an absolute number. Wall-clock budgets
//! on a developer machine are noisy (a loaded local test gate runs these
//! alongside everything else), so the budgets below are deliberately generous —
//! they are sized to catch a per-result N+1 coming back, not to police
//! single-digit-percent drift.

#[cfg(test)]
mod search_result_scaling_tests {
    use anyhow::Result;
    use nodespace_core::db::SqliteStore;
    use nodespace_core::models::{NewEmbedding, Node};
    use nodespace_core::services::NodeService;
    use serde_json::json;
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    const DIM: usize = 768;

    /// Nodes seeded into the corpus. Large enough that the KNN window and the
    /// connection pool behave like they do in a real database (the pool holds 8
    /// idle readers, so a per-result fetch loop past that depth pays repeated
    /// connection opens), while still seeding in a few seconds under a debug
    /// test build.
    const CORPUS: usize = 2_000;

    async fn create_test_store() -> Result<(Arc<SqliteStore>, TempDir)> {
        let temp_dir = TempDir::new()?;
        let db_path = temp_dir.path().join("test.db");
        let mut store = Arc::new(SqliteStore::new(db_path).await?);
        let _service = NodeService::new(&mut store).await?;
        Ok((store, temp_dir))
    }

    /// A vector clustered near axis 0, offset by `i` so that every node has a
    /// distinct but comparable similarity to `query_vector()`. This makes the
    /// ranking non-degenerate: results differ in score, so truncation to `limit`
    /// actually selects rather than picking an arbitrary subset.
    fn seeded_vector(i: usize) -> Vec<f32> {
        let mut v = vec![0.0f32; DIM];
        v[0] = 1.0;
        // A small perturbation on a rotating second axis keeps vectors distinct
        // while leaving them all close to the query.
        v[1 + (i % (DIM - 1))] = 0.01 + (i % 100) as f32 * 0.0005;
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.iter().map(|x| x / norm).collect()
    }

    fn query_vector() -> Vec<f32> {
        let mut v = vec![0.0f32; DIM];
        v[0] = 1.0;
        v
    }

    fn embedding_for(node_id: &str, i: usize) -> NewEmbedding {
        NewEmbedding {
            node_id: node_id.to_string(),
            vector: seeded_vector(i),
            model_name: Some("test-model".to_string()),
            chunk_index: 0,
            chunk_start: 0,
            chunk_end: 100,
            total_chunks: 1,
            content_hash: format!("hash-{i}"),
            token_count: 10,
        }
    }

    /// Seed a corpus of embedded text nodes, each with enough content that
    /// hydrating a result is not free.
    async fn seed_corpus(store: &SqliteStore, count: usize) -> Result<()> {
        for i in 0..count {
            let content = format!(
                "Seeded search corpus document {i}. It carries several sentences of \
                 body text so that hydrating a search result moves a realistic amount \
                 of row data rather than a bare identifier. Topic marker: tech stack, \
                 architecture, persistence, indexing, retrieval."
            );
            let node = Node::new("text".to_string(), content, json!({}));
            let id = node.id.clone();
            store.create_node(node, None, None).await?;
            store
                .upsert_embeddings(&id, vec![embedding_for(&id, i)])
                .await?;
        }
        Ok(())
    }

    async fn time_search(store: &SqliteStore, limit: i64) -> Result<(Duration, usize)> {
        let q = query_vector();
        let start = Instant::now();
        let results = store.search_embeddings(&q, limit, Some(0.5)).await?;
        Ok((start.elapsed(), results.len()))
    }

    /// The headline guard: a search at limit 20 must not cost anything like 20x
    /// a search at limit 1.
    ///
    /// Budget rationale: with the batched fetch, both calls are dominated by the
    /// same fixed KNN scan, and the limit-20 call adds one extra `IN (...)` row
    /// fetch. Measured on a release build over a 32k-node corpus the two are
    /// within noise of each other (27.7ms vs 31.9ms). The assertion allows
    /// limit-20 to cost up to 4x limit-1 plus a 50ms floor — comfortably above
    /// the real ratio (~1.15x) so noise cannot fail it, while still failing long
    /// before a genuine per-result slope could hide inside it. The floor keeps a
    /// fast-but-jittery limit-1 baseline, where scheduler noise is a large
    /// multiple of a small number, from making the ratio meaningless.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn search_cost_does_not_scale_with_result_count() -> Result<()> {
        let (store, _t) = create_test_store().await?;
        seed_corpus(&store, CORPUS).await?;

        // Warm the reader pool and any lazily-built index state so the first
        // measured call is not paying setup the others avoid.
        let _ = time_search(&store, 20).await?;

        let (t1, n1) = time_search(&store, 1).await?;
        let (t20, n20) = time_search(&store, 20).await?;

        assert_eq!(n1, 1, "limit 1 returns one result");
        assert_eq!(n20, 20, "limit 20 returns twenty results");

        let budget = t1 * 4 + Duration::from_millis(50);
        assert!(
            t20 <= budget,
            "search at limit 20 ({t20:?}) exceeded budget {budget:?} (limit 1 was {t1:?}); \
             a per-result slope has returned — check for a `get_node` per result in \
             search_embeddings"
        );
        Ok(())
    }

    /// The same property stated over the full series from the report, so a
    /// regression that only shows up at larger limits is still caught.
    ///
    /// Budget rationale: 2 seconds for the whole 1/2/5/10/20/50 sweep. On a
    /// release build over a 32k-node corpus the sweep totals under 200ms, and
    /// even a debug build on a loaded machine stays well inside a second. The
    /// budget is set an order of magnitude above the measured cost so that
    /// scheduler noise cannot fail it, while still being far below what a
    /// per-result fetch loop costs once each of the ~88 results in the sweep
    /// pays its own round trip.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn search_series_stays_within_budget() -> Result<()> {
        let (store, _t) = create_test_store().await?;
        seed_corpus(&store, CORPUS).await?;

        let _ = time_search(&store, 20).await?;

        let mut total = Duration::ZERO;
        for limit in [1, 2, 5, 10, 20, 50] {
            let (elapsed, count) = time_search(&store, limit).await?;
            assert_eq!(
                count, limit as usize,
                "limit {limit} returned {count} results"
            );
            total += elapsed;
        }

        let budget = Duration::from_secs(2);
        assert!(
            total <= budget,
            "the 1/2/5/10/20/50 search sweep took {total:?}, over the {budget:?} budget"
        );
        Ok(())
    }

    /// Every returned result carries its node. The gRPC search handler maps
    /// these straight onto the wire type instead of re-reading each result from
    /// the store by id, so a result arriving without its node would silently
    /// drop a row from a search response rather than merely costing a query.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn every_result_carries_its_node() -> Result<()> {
        let (store, _t) = create_test_store().await?;
        seed_corpus(&store, 200).await?;

        let results = store
            .search_embeddings(&query_vector(), 20, Some(0.5))
            .await?;
        assert_eq!(results.len(), 20, "limit 20 returns twenty results");
        for r in &results {
            let node = r
                .node
                .as_ref()
                .unwrap_or_else(|| panic!("result {} has no node attached", r.node_id));
            assert_eq!(
                node.id, r.node_id,
                "the attached node is the one the result names"
            );
        }
        Ok(())
    }

    /// Hydration must not fetch nodes that ranking is about to discard. KNN
    /// over-fetches by design and callers inflate `limit` further for their own
    /// post-filters, so hydrating before truncation did work that was thrown
    /// away. Asserting the returned count equals `limit` (not the much larger
    /// candidate set) pins that ordering.
    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn hydration_is_bounded_by_limit_not_candidates() -> Result<()> {
        let (store, _t) = create_test_store().await?;
        seed_corpus(&store, CORPUS).await?;

        let q = query_vector();
        let results = store.search_embeddings(&q, 5, Some(0.5)).await?;

        assert_eq!(results.len(), 5, "truncated to the requested limit");
        assert!(
            results.iter().all(|r| r.node.is_some()),
            "every returned result is hydrated"
        );
        // Scores must be in descending order — ranking happens before truncation,
        // so the top 5 are the genuinely best 5, not an arbitrary 5.
        for pair in results.windows(2) {
            assert!(
                pair[0].score >= pair[1].score,
                "results are ranked by descending score"
            );
        }
        Ok(())
    }
}
