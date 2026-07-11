//! End-to-end integration test for the `nodespace` CLI.
//!
//! Spins an in-process `nodespaced` gRPC server up against a tempdir-backed
//! SQLite database, then drives the CLI's command handlers (via the library
//! surface) at it. This validates that the CLI's gRPC plumbing — connection,
//! request construction, response unwrapping, error mapping — works end to
//! end against the same service stack the real binary uses.
//!
//! We exercise the handlers directly rather than spawning the compiled
//! binary so test failures point at the code path under test rather than
//! at fork/exec or stdout-capture plumbing.
//!
//! Two harnesses back the tests:
//! - [`spawn_test_daemon`] serves a single, plain `NodeService` — enough for the
//!   node/mention/schema/search flows that don't touch the database registry.
//! - [`spawn_routing_daemon`] serves the full multi-database stack (ADR-053):
//!   `DbManagerLayer` + `DatabaseService` + `NodeService` over a `DatabaseManager`
//!   seeded with one default database, so `database` subcommands and
//!   `--database` routing can be exercised over the real transport.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nodespace_agent::pty::PtySessionManager;
use nodespace_cli::{commands, connect, connect_database, DatabaseIdInterceptor};
use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_daemon::nodespace::{
    CreateDatabaseRequest, CreateNodeRequest, GetNodeRequest, GetRelatedNodesRequest,
    ListDatabasesRequest, QueryNodesSimpleRequest,
};
use nodespace_daemon::{
    DatabaseManager, DatabaseServiceImpl, DatabaseServiceServer, DbManagerLayer, NodeServiceImpl,
    NodeServiceServer, SharedContext,
};
use nodespace_nlp_engine::EmbeddingService;
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio::sync::{oneshot, watch};
use tokio_stream::wrappers::UnixListenerStream;
use tonic::transport::Server;
use tonic::Code;

/// Spawn an in-process daemon over a temp-dir UDS and return the socket path.
async fn spawn_test_daemon() -> (PathBuf, oneshot::Sender<()>, TempDir) {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let sock_path = tempdir.path().join("test-daemon.sock");

    let mut store = Arc::new(
        SqliteStore::new(tempdir.path().join("daemon-db"))
            .await
            .expect("failed to open SqliteStore"),
    );
    let node_service = Arc::new(
        CoreNodeService::new(&mut store)
            .await
            .expect("failed to build NodeService"),
    );
    let service = NodeServiceImpl::new(
        node_service,
        Arc::new(tokio::sync::RwLock::new(None)),
        Arc::new(nodespace_core::services::EmbeddingScheduler::new()),
    );

    let listener = UnixListener::bind(&sock_path).expect("failed to bind test UDS socket");
    let incoming = UnixListenerStream::new(listener);

    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    tokio::spawn(async move {
        Server::builder()
            .add_service(NodeServiceServer::new(service))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server crashed");
    });

    for _ in 0..50 {
        if connect(&sock_path, DatabaseIdInterceptor::none())
            .await
            .is_ok()
        {
            return (sock_path, shutdown_tx, tempdir);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "daemon did not start accepting connections on {}",
        sock_path.display()
    );
}

/// A model-less build context — with `has_model = false` no embedding wiring
/// runs, so the dropped watch sender is harmless (it is never read).
fn routing_test_context() -> SharedContext {
    let (_tx, model) = watch::channel::<Option<Arc<EmbeddingService>>>(None);
    SharedContext {
        pty_manager: Arc::new(PtySessionManager::new()),
        model,
        has_model: false,
        scheduler: Arc::new(nodespace_core::services::EmbeddingScheduler::new()),
    }
}

/// Spawn an in-process daemon over a temp-dir UDS serving the full ADR-053
/// multi-database stack: the `DbManagerLayer` injects a `DatabaseManager` into
/// every request, `DatabaseService` manages the registry, and `NodeService`
/// routes by the `x-ns-database-id` header. The registry is seeded with a single
/// default database so header-less requests work and a second can be created.
async fn spawn_routing_daemon() -> (PathBuf, oneshot::Sender<()>, TempDir) {
    let tempdir = TempDir::new().expect("failed to create tempdir");
    let sock_path = tempdir.path().join("routing-daemon.sock");
    let registry_path = tempdir.path().join("databases.toml");
    let default_db = tempdir.path().join("default.db");

    let manager = Arc::new(
        DatabaseManager::load(registry_path, routing_test_context())
            .await
            .expect("failed to load DatabaseManager"),
    );
    let default_id = manager
        .ensure_default_registered("Default".into(), default_db)
        .await
        .expect("failed to register default database");
    // Open the default through the manager and serve that bundle's NodeService
    // (exactly as the daemon boot path does) so the manager's cache is the served
    // handle — no double-open.
    let default_bundle = manager
        .get_or_open(&default_id)
        .await
        .expect("failed to open default database");
    let node_default = default_bundle.node_service_grpc.clone();

    let listener = UnixListener::bind(&sock_path).expect("failed to bind routing UDS socket");
    let incoming = UnixListenerStream::new(listener);
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let mgr = manager.clone();
    tokio::spawn(async move {
        Server::builder()
            .layer(DbManagerLayer::new(mgr.clone()))
            .add_service(DatabaseServiceServer::new(DatabaseServiceImpl::new(mgr)))
            .add_service(NodeServiceServer::new(node_default))
            .serve_with_incoming_shutdown(incoming, async move {
                let _ = shutdown_rx.await;
            })
            .await
            .expect("server crashed");
    });

    for _ in 0..50 {
        if connect_database(&sock_path).await.is_ok() {
            return (sock_path, shutdown_tx, tempdir);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "routing daemon did not start accepting connections on {}",
        sock_path.display()
    );
}

/// Route a fresh `NodeService` client to `id` via the resolved routing header.
async fn node_client_for(sock: &std::path::Path, id: &str) -> nodespace_cli::NodeClient {
    let interceptor = DatabaseIdInterceptor::for_id(id).expect("build interceptor");
    connect(sock, interceptor)
        .await
        .expect("connect routed node")
}

#[tokio::test]
async fn create_get_update_children_delete_round_trip() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Create(commands::node::CreateArgs {
            node_type: "text".into(),
            content: "root via CLI".into(),
            parent: None,
        }),
        true,
    )
    .await
    .expect("create root");

    let mut raw_client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw client connect");

    let created = raw_client
        .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
            node_type: "text".into(),
            content: "parent".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed parent")
        .into_inner();
    let parent_id = created.node_id;

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Create(commands::node::CreateArgs {
            node_type: "text".into(),
            content: "child via CLI".into(),
            parent: Some(parent_id.clone()),
        }),
        false,
    )
    .await
    .expect("create child");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Get(commands::node::GetArgs {
            id: parent_id.clone(),
        }),
        false,
    )
    .await
    .expect("get parent");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Update(commands::node::UpdateArgs {
            id: parent_id.clone(),
            content: Some("parent updated via CLI".into()),
            properties: vec![],
        }),
        true,
    )
    .await
    .expect("update parent");

    let fetched = raw_client
        .get_node(GetNodeRequest {
            node_id: parent_id.clone(),
        })
        .await
        .expect("post-update fetch")
        .into_inner();
    assert_eq!(
        fetched.node_data.expect("node_data").content,
        "parent updated via CLI"
    );

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Children(commands::node::ChildrenArgs {
            id: parent_id.clone(),
        }),
        true,
    )
    .await
    .expect("list children");

    let children = raw_client
        .get_children(nodespace_daemon::nodespace::GetChildrenRequest {
            node_id: parent_id.clone(),
        })
        .await
        .expect("children fetch")
        .into_inner();
    assert_eq!(
        children.count, 1,
        "expected exactly one child seeded via CLI"
    );
    assert_eq!(children.nodes.len(), 1, "nodes len must match count");
    assert_eq!(children.nodes[0].content, "child via CLI");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Delete(commands::node::DeleteArgs {
            id: parent_id.clone(),
        }),
        false,
    )
    .await
    .expect("delete parent");

    let err = raw_client
        .get_node(GetNodeRequest { node_id: parent_id })
        .await
        .expect_err("expected not_found after delete");
    assert_eq!(err.code(), Code::NotFound);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn get_missing_node_surfaces_not_found() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    let err = commands::node::run(
        &mut client,
        commands::node::NodeAction::Get(commands::node::GetArgs {
            id: "does-not-exist".into(),
        }),
        false,
    )
    .await
    .expect_err("expected error");

    let status = err
        .chain()
        .find_map(|e| e.downcast_ref::<tonic::Status>())
        .expect("expected tonic::Status in error chain");
    assert_eq!(status.code(), Code::NotFound);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn search_without_embedding_service_reports_unavailable() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    let err = commands::search::run(
        &mut client,
        commands::search::SearchArgs {
            query: "anything".into(),
            node_types: vec![],
            collection: None,
            collection_id: None,
            filters: None,
            threshold: 0.0,
            limit: 0,
        },
        true,
    )
    .await
    .expect_err("expected unavailable");

    let status = err
        .chain()
        .find_map(|e| e.downcast_ref::<tonic::Status>())
        .expect("expected tonic::Status in error chain");
    assert_eq!(status.code(), Code::Unavailable);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn diagnostics_collect_reports_counts_and_recency() {
    let (sock, shutdown, _tempdir) = spawn_routing_daemon().await;
    let mut node = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect node");
    let mut seed = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect seed");
    let mut db = connect_database(&sock).await.expect("connect database");

    let baseline = commands::diagnostics::collect(&mut node, &mut db, None).await;
    assert!(
        baseline.errors.is_empty(),
        "baseline collect must not produce errors: {:?}",
        baseline.errors
    );
    // The registry has exactly the seeded default, and it is the target when no
    // database is selected.
    assert_eq!(baseline.databases.len(), 1, "one registered database");
    assert!(baseline.databases[0].is_default);
    assert_eq!(baseline.targeted_database_id, baseline.databases[0].id);

    let root = seed
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "root".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed root")
        .into_inner();

    let mut last_child_id = String::new();
    for label in ["child-1", "child-2"] {
        tokio::time::sleep(Duration::from_millis(20)).await;
        last_child_id = seed
            .create_node(CreateNodeRequest {
                node_type: "text".into(),
                content: label.into(),
                parent_id: Some(root.node_id.clone()),
                properties: String::new(),
                collection: None,
                lifecycle_status: None,
                id: None,
                position: None,
            })
            .await
            .unwrap_or_else(|e| panic!("seed {label}: {e}"))
            .into_inner()
            .node_id;
    }

    let report = commands::diagnostics::collect(&mut node, &mut db, None).await;
    assert_eq!(
        report.total_node_count,
        baseline.total_node_count + 3,
        "expected three additional nodes vs baseline"
    );
    assert_eq!(
        report.root_node_count,
        baseline.root_node_count + 1,
        "expected one additional root node vs baseline"
    );
    assert!(
        report.database_size_bytes.unwrap_or(0) > 0,
        "targeted database file should have a nonzero size after writes"
    );
    assert_eq!(report.recent_node_ids[0], last_child_id);
    assert!(
        report.errors.is_empty(),
        "happy-path collect must not surface errors: {:?}",
        report.errors
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn connect_refused_returns_friendly_error() {
    let err = connect(
        std::path::Path::new("/tmp/nodespace-no-such-daemon.sock"),
        DatabaseIdInterceptor::none(),
    )
    .await
    .expect_err("expected refusal");

    let msg = format!("{}", err);
    assert!(
        msg.contains("Could not connect to nodespaced"),
        "expected friendly error, got: {msg}"
    );
    assert!(
        msg.contains("Is the daemon running?"),
        "expected remediation hint, got: {msg}"
    );
}

#[tokio::test]
async fn node_query_by_type() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    raw.create_node(CreateNodeRequest {
        node_type: "task".into(),
        content: "do the thing".into(),
        parent_id: None,
        properties: String::new(),
        collection: None,
        lifecycle_status: None,
        id: None,
        position: None,
    })
    .await
    .expect("seed task");

    raw.create_node(CreateNodeRequest {
        node_type: "text".into(),
        content: "some text".into(),
        parent_id: None,
        properties: String::new(),
        collection: None,
        lifecycle_status: None,
        id: None,
        position: None,
    })
    .await
    .expect("seed text");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Query(commands::node::QueryArgs {
            id: None,
            mentioned_by: None,
            content_contains: None,
            title_contains: None,
            node_type: Some("task".into()),
            limit: 0,
            offset: 0,
        }),
        true,
    )
    .await
    .expect("query by type");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_export_markdown() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let root = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "# Root Document".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed root")
        .into_inner();

    raw.create_node(CreateNodeRequest {
        node_type: "text".into(),
        content: "Child paragraph".into(),
        parent_id: Some(root.node_id.clone()),
        properties: String::new(),
        collection: None,
        lifecycle_status: None,
        id: None,
        position: None,
    })
    .await
    .expect("seed child");

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Export(commands::node::ExportArgs {
            id: root.node_id.clone(),
            children: true,
            max_depth: 0,
            node_ids: false,
        }),
        true,
    )
    .await
    .expect("export markdown");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_batch_get_and_update() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let a = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "node-a".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed a")
        .into_inner()
        .node_id;

    let b = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "node-b".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed b")
        .into_inner()
        .node_id;

    // batch-get: both found, one missing
    commands::node::run(
        &mut client,
        commands::node::NodeAction::BatchGet(commands::node::BatchGetArgs {
            ids: vec![a.clone(), b.clone(), "does-not-exist".into()],
        }),
        true,
    )
    .await
    .expect("batch-get");

    // batch-update (auto-version)
    let updates_json = serde_json::json!([
        {"node_id": a, "content": "node-a updated"},
        {"node_id": b, "content": "node-b updated"},
    ])
    .to_string();

    commands::node::run(
        &mut client,
        commands::node::NodeAction::BatchUpdate(commands::node::BatchUpdateArgs {
            updates: updates_json,
        }),
        true,
    )
    .await
    .expect("batch-update");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn mention_create_query_delete() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let source = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "source node".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed source")
        .into_inner()
        .node_id;

    let target = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "target node".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed target")
        .into_inner()
        .node_id;

    commands::mention::run(
        &mut client,
        commands::mention::MentionAction::Create(commands::mention::CreateMentionArgs {
            from: source.clone(),
            to: target.clone(),
        }),
        true,
    )
    .await
    .expect("create mention");

    commands::mention::run(
        &mut client,
        commands::mention::MentionAction::Outgoing(commands::mention::MentionQueryArgs {
            id: source.clone(),
        }),
        true,
    )
    .await
    .expect("outgoing mentions");

    commands::mention::run(
        &mut client,
        commands::mention::MentionAction::Incoming(commands::mention::MentionQueryArgs {
            id: target.clone(),
        }),
        true,
    )
    .await
    .expect("incoming mentions");

    commands::mention::run(
        &mut client,
        commands::mention::MentionAction::Delete(commands::mention::DeleteMentionArgs {
            from: source.clone(),
            to: target.clone(),
        }),
        true,
    )
    .await
    .expect("delete mention");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn schema_list_and_get() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    // The test daemon has no custom schemas; list should return an empty result without error.
    commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::List(commands::schema::SchemaListArgs {}),
        true,
    )
    .await
    .expect("schema list");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn schema_create_and_update_round_trip() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::Create(commands::schema::SchemaParamsArgs {
            params: Some(
                serde_json::json!({
                    "name": "Invoice",
                    "fields": [
                        {"name": "amount", "type": "number"}
                    ]
                })
                .to_string(),
            ),
            params_file: None,
        }),
        true,
    )
    .await
    .expect("schema create");

    // Fetch the created schema back via the existing read path to confirm it landed.
    commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::Get(commands::schema::SchemaGetArgs {
            id: "invoice".into(),
        }),
        true,
    )
    .await
    .expect("schema get after create");

    commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::Update(commands::schema::SchemaParamsArgs {
            params: Some(
                serde_json::json!({
                    "schema_id": "invoice",
                    "add_fields": [
                        {"name": "currency", "type": "string"}
                    ]
                })
                .to_string(),
            ),
            params_file: None,
        }),
        true,
    )
    .await
    .expect("schema update");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn schema_create_rejects_malformed_params() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    let err = commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::Create(commands::schema::SchemaParamsArgs {
            params: Some("not json".into()),
            params_file: None,
        }),
        true,
    )
    .await
    .expect_err("malformed params_json should error");
    let status = err
        .chain()
        .find_map(|e| e.downcast_ref::<tonic::Status>())
        .expect("expected tonic::Status in error chain");
    assert_eq!(status.code(), Code::InvalidArgument);
    assert!(
        status.message().contains("params_json"),
        "expected status message to name the offending field, got: {}",
        status.message()
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn execute_query_filters_by_property() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    raw.create_node(CreateNodeRequest {
        node_type: "task".into(),
        content: "task one".into(),
        parent_id: None,
        properties: serde_json::json!({"status": "open"}).to_string(),
        collection: None,
        lifecycle_status: None,
        id: None,
        position: None,
    })
    .await
    .expect("seed open task");

    raw.create_node(CreateNodeRequest {
        node_type: "task".into(),
        content: "task two".into(),
        parent_id: None,
        properties: serde_json::json!({"status": "done"}).to_string(),
        collection: None,
        lifecycle_status: None,
        id: None,
        position: None,
    })
    .await
    .expect("seed done task");

    commands::query::run(
        &mut client,
        commands::query::QueryArgs {
            target_type: "task".into(),
            filters: Some(
                serde_json::json!([
                    {"type": "property", "operator": "equals", "property": "status", "value": "open"}
                ])
                .to_string(),
            ),
            sorting: None,
            limit: 0,
        },
        true,
    )
    .await
    .expect("execute query");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn relationship_create_and_get() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    // Relationships must be schema-defined (find-then-edit guidance: the
    // relationship name must already exist on the source node's schema).
    // Create a "ticket" schema with a "blocks" -> "ticket" relationship, then
    // two ticket instances to relate.
    commands::schema::run(
        &mut client,
        commands::schema::SchemaAction::Create(commands::schema::SchemaParamsArgs {
            params: Some(
                serde_json::json!({
                    "name": "Ticket",
                    "fields": [{"name": "title", "type": "string"}],
                    "relationships": [
                        {"name": "blocks", "target_type": "ticket", "direction": "out", "cardinality": "many"}
                    ]
                })
                .to_string(),
            ),
            params_file: None,
        }),
        true,
    )
    .await
    .expect("create ticket schema");

    let source = raw
        .create_node(CreateNodeRequest {
            node_type: "ticket".into(),
            content: "source".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed source")
        .into_inner()
        .node_id;

    let target = raw
        .create_node(CreateNodeRequest {
            node_type: "ticket".into(),
            content: "target".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed target")
        .into_inner()
        .node_id;

    commands::relationship::run(
        &mut client,
        commands::relationship::RelationshipAction::Create(commands::relationship::CreateArgs {
            from: source.clone(),
            relationship_name: "blocks".into(),
            to: target.clone(),
            edge_data: None,
        }),
        true,
    )
    .await
    .expect("create relationship");

    commands::relationship::run(
        &mut client,
        commands::relationship::RelationshipAction::Get(commands::relationship::GetArgs {
            id: source.clone(),
            relationship_name: "blocks".into(),
            direction: commands::relationship::Direction::Out,
        }),
        true,
    )
    .await
    .expect("get related nodes");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn relationship_create_rejects_relationship_undefined_on_source_schema() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    // Plain "text" nodes have no schema-defined relationships at all, so
    // any non-built-in relationship name must be rejected (find-then-edit
    // guidance: the relationship must already exist on the source's schema).
    let source = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "source".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed source")
        .into_inner()
        .node_id;

    let target = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "target".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed target")
        .into_inner()
        .node_id;

    let err = commands::relationship::run(
        &mut client,
        commands::relationship::RelationshipAction::Create(commands::relationship::CreateArgs {
            from: source,
            relationship_name: "not_a_defined_relationship".into(),
            to: target,
            edge_data: None,
        }),
        true,
    )
    .await
    .expect_err("relationship not defined on schema should error");
    let status = err
        .chain()
        .find_map(|e| e.downcast_ref::<tonic::Status>())
        .expect("expected tonic::Status in error chain");
    assert!(
        status.message().contains("not_a_defined_relationship")
            || status.message().contains("not defined"),
        "expected error to name the undefined relationship, got: {}",
        status.message()
    );

    let _ = shutdown.send(());
}

// `nodespace relationship get --direction` is a clap ValueEnum (only "out"/"in"
// are constructible), so an invalid direction can no longer reach the CLI's
// GetArgs at all — clap itself rejects it before this test's code would run.
// The daemon-side validation this used to exercise is still real (any other
// gRPC client, not just this CLI, can send an arbitrary string on the wire),
// so this drives GetRelatedNodesRequest directly against the raw client.
#[tokio::test]
async fn get_related_nodes_rpc_rejects_invalid_direction() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let status = raw
        .get_related_nodes(GetRelatedNodesRequest {
            node_id: "some-id".into(),
            relationship_name: "blocks".into(),
            direction: "sideways".into(),
        })
        .await
        .expect_err("invalid direction should error");
    assert_eq!(status.code(), Code::InvalidArgument);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_update_sets_properties_and_preserves_content() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let id = raw
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "original content".into(),
            parent_id: None,
            properties: serde_json::json!({"existing": "keep-me"}).to_string(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed node")
        .into_inner()
        .node_id;

    commands::node::run(
        &mut client,
        commands::node::NodeAction::Update(commands::node::UpdateArgs {
            id: id.clone(),
            content: None,
            properties: vec![("added".into(), serde_json::json!("value"))],
        }),
        true,
    )
    .await
    .expect("update properties only");

    let node = raw
        .get_node(GetNodeRequest {
            node_id: id.clone(),
        })
        .await
        .expect("get node")
        .into_inner()
        .node_data
        .expect("node_data");
    assert_eq!(
        node.content, "original content",
        "content must be preserved when only properties are set"
    );
    let props: serde_json::Value =
        serde_json::from_str(&node.properties).expect("parse properties");
    // Typed properties are namespaced under the node's type key on the wire
    // (properties.<node_type>.<field>), per the typed-value shape produced by
    // crate::models::node_to_typed_value.
    assert_eq!(props["text"]["added"], "value");
    assert_eq!(
        props["text"]["existing"], "keep-me",
        "existing properties must be deep-merged, not replaced"
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_update_rejects_empty_args() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    let err = commands::node::run(
        &mut client,
        commands::node::NodeAction::Update(commands::node::UpdateArgs {
            id: "irrelevant".into(),
            content: None,
            properties: vec![],
        }),
        true,
    )
    .await
    .expect_err("update with no content and no properties should error");
    assert!(err.to_string().contains("--content"));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_set_status_updates_status_property() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");
    let mut raw = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("raw connect");

    let id = raw
        .create_node(CreateNodeRequest {
            node_type: "task".into(),
            content: "a task".into(),
            parent_id: None,
            properties: serde_json::json!({"status": "open"}).to_string(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("seed task")
        .into_inner()
        .node_id;

    commands::node::run(
        &mut client,
        commands::node::NodeAction::SetStatus(commands::node::SetStatusArgs {
            id: id.clone(),
            status: "done".into(),
        }),
        true,
    )
    .await
    .expect("set status");

    let node = raw
        .get_node(GetNodeRequest {
            node_id: id.clone(),
        })
        .await
        .expect("get node")
        .into_inner()
        .node_data
        .expect("node_data");
    let props: serde_json::Value =
        serde_json::from_str(&node.properties).expect("parse properties");
    assert_eq!(props["task"]["status"], "done");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn node_set_status_rejects_invalid_status() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect");

    let err = commands::node::run(
        &mut client,
        commands::node::NodeAction::SetStatus(commands::node::SetStatusArgs {
            id: "irrelevant".into(),
            status: "not-a-real-status".into(),
        }),
        true,
    )
    .await
    .expect_err("invalid status should error");
    assert!(err.to_string().contains("invalid status"));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn database_registry_round_trip() {
    let (sock, shutdown, tempdir) = spawn_routing_daemon().await;
    let mut db = connect_database(&sock).await.expect("connect database");

    // list: succeeds and reports the seeded default.
    commands::database::run(&mut db, commands::database::DatabaseAction::List, true)
        .await
        .expect("database list");
    let listed = db
        .list(ListDatabasesRequest {})
        .await
        .expect("raw list")
        .into_inner();
    assert_eq!(listed.databases.len(), 1);
    let default_id = listed.default_database_id.clone();
    assert!(!default_id.is_empty());

    // create: registers a second database.
    let second_path = tempdir.path().join("second.db").display().to_string();
    commands::database::run(
        &mut db,
        commands::database::DatabaseAction::Create(commands::database::CreateArgs {
            name: "Second".into(),
            path: Some(second_path),
        }),
        true,
    )
    .await
    .expect("database create");
    let listed = db
        .list(ListDatabasesRequest {})
        .await
        .expect("raw list")
        .into_inner();
    assert_eq!(listed.databases.len(), 2);
    let second_id = listed
        .databases
        .iter()
        .find(|d| d.name == "Second")
        .expect("second registered")
        .id
        .clone();

    // Open the second database by routing a write to it, so its file exists on
    // disk — this lets the remove assertion below prove the file is preserved
    // rather than merely never created (a freshly created DB is "missing" until
    // first opened).
    let mut node_second = node_client_for(&sock, &second_id).await;
    node_second
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "open the second database".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("open second database");

    // rename: resolves by name and relabels.
    commands::database::run(
        &mut db,
        commands::database::DatabaseAction::Rename(commands::database::RenameArgs {
            database: "Second".into(),
            new_name: "Renamed".into(),
        }),
        true,
    )
    .await
    .expect("database rename");
    let listed = db
        .list(ListDatabasesRequest {})
        .await
        .expect("raw list")
        .into_inner();
    let renamed = listed
        .databases
        .iter()
        .find(|d| d.id == second_id)
        .expect("still registered by id");
    assert_eq!(renamed.name, "Renamed");

    // use: sets the daemon-wide default to the renamed database.
    commands::database::run(
        &mut db,
        commands::database::DatabaseAction::Use(commands::database::UseArgs {
            database: "Renamed".into(),
        }),
        true,
    )
    .await
    .expect("database use");
    let listed = db
        .list(ListDatabasesRequest {})
        .await
        .expect("raw list")
        .into_inner();
    assert_eq!(listed.default_database_id, second_id);

    // Put the default back so removing the (now non-default) original succeeds.
    commands::database::run(
        &mut db,
        commands::database::DatabaseAction::Use(commands::database::UseArgs {
            database: default_id.clone(),
        }),
        true,
    )
    .await
    .expect("database use back to default");

    // remove: unregisters by id without deleting the file.
    let second_file = tempdir.path().join("second.db");
    commands::database::run(
        &mut db,
        commands::database::DatabaseAction::Remove(commands::database::RemoveArgs {
            database: second_id.clone(),
        }),
        true,
    )
    .await
    .expect("database remove");
    let listed = db
        .list(ListDatabasesRequest {})
        .await
        .expect("raw list")
        .into_inner();
    assert_eq!(listed.databases.len(), 1);
    assert!(
        listed.databases.iter().all(|d| d.id != second_id),
        "removed database must be gone from the registry"
    );
    assert!(
        second_file.exists(),
        "remove must not delete the underlying database file"
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn select_database_by_name_resolves_to_id() {
    let (sock, shutdown, tempdir) = spawn_routing_daemon().await;
    let mut db = connect_database(&sock).await.expect("connect database");

    let created = db
        .create(CreateDatabaseRequest {
            name: "Workspace".into(),
            path: Some(tempdir.path().join("workspace.db").display().to_string()),
        })
        .await
        .expect("create database")
        .into_inner();

    // A name resolves to the matching id...
    let resolved = commands::database::resolve_database_id_by_selection(&mut db, "Workspace")
        .await
        .expect("resolve by name");
    assert_eq!(resolved, created.id);

    // ...and the id resolves to itself.
    let resolved_by_id = commands::database::resolve_database_id_by_selection(&mut db, &created.id)
        .await
        .expect("resolve by id");
    assert_eq!(resolved_by_id, created.id);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn unregistered_database_selection_errors() {
    let (sock, shutdown, _tempdir) = spawn_routing_daemon().await;
    let mut db = connect_database(&sock).await.expect("connect database");

    let err = commands::database::resolve_database_id_by_selection(&mut db, "no-such-database")
        .await
        .expect_err("unregistered selection must error");
    assert!(
        err.to_string().contains("no database named or with id"),
        "expected a clear not-registered error, got: {err}"
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn ambiguous_name_selection_errors_but_id_still_resolves() {
    // The daemon does not enforce unique names, so two databases can share a
    // name. Selecting by that name must fail with an ambiguity error; selecting
    // by id must still resolve (id match wins over any name match).
    let (sock, shutdown, tempdir) = spawn_routing_daemon().await;
    let mut db = connect_database(&sock).await.expect("connect database");

    let first = db
        .create(CreateDatabaseRequest {
            name: "work".into(),
            path: Some(tempdir.path().join("work-a.db").display().to_string()),
        })
        .await
        .expect("create first work")
        .into_inner();
    db.create(CreateDatabaseRequest {
        name: "work".into(),
        path: Some(tempdir.path().join("work-b.db").display().to_string()),
    })
    .await
    .expect("create second work");

    let err = commands::database::resolve_database_id_by_selection(&mut db, "work")
        .await
        .expect_err("a name shared by two databases must be ambiguous");
    let msg = err.to_string();
    assert!(
        msg.contains("ambiguous"),
        "expected an ambiguity error, got: {msg}"
    );
    // Both colliding ids should be listed so the user can disambiguate.
    assert!(
        msg.contains(&first.id),
        "ambiguity error should list the colliding ids, got: {msg}"
    );

    // Selecting by id sidesteps the ambiguity.
    let resolved = commands::database::resolve_database_id_by_selection(&mut db, &first.id)
        .await
        .expect("id resolves unambiguously even with a duplicate name");
    assert_eq!(resolved, first.id);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn database_routing_isolates_writes() {
    let (sock, shutdown, tempdir) = spawn_routing_daemon().await;
    let mut db = connect_database(&sock).await.expect("connect database");

    // Register a second database.
    db.create(CreateDatabaseRequest {
        name: "Second".into(),
        path: Some(tempdir.path().join("second.db").display().to_string()),
    })
    .await
    .expect("create second")
    .into_inner();

    // Resolve its id by name and route a NodeService client to it.
    let second_id = commands::database::resolve_database_id_by_selection(&mut db, "Second")
        .await
        .expect("resolve second");
    let mut node_second = node_client_for(&sock, &second_id).await;

    // Create a node routed to the second database via the node command handler.
    commands::node::run(
        &mut node_second,
        commands::node::NodeAction::Create(commands::node::CreateArgs {
            node_type: "text".into(),
            content: "isolated-to-second".into(),
            parent: None,
        }),
        true,
    )
    .await
    .expect("create in second");

    let query = QueryNodesSimpleRequest {
        id: None,
        mentioned_by: None,
        content_contains: Some("isolated-to-second".into()),
        title_contains: None,
        node_type: None,
        limit: 0,
        offset: 0,
    };

    // Visible from the second database...
    let in_second = node_second
        .query_nodes_simple(query.clone())
        .await
        .expect("query second")
        .into_inner();
    assert_eq!(
        in_second.nodes.len(),
        1,
        "the write must be visible in the database it was routed to"
    );

    // ...and invisible from the default (header-less) database — no cross-database bleed.
    let mut node_default = connect(&sock, DatabaseIdInterceptor::none())
        .await
        .expect("connect default");
    let in_default = node_default
        .query_nodes_simple(query)
        .await
        .expect("query default")
        .into_inner();
    assert!(
        in_default.nodes.is_empty(),
        "the write must not leak into the default database"
    );

    let _ = shutdown.send(());
}
