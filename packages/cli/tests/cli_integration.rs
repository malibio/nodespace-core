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

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use nodespace_cli::{commands, connect};
use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_daemon::nodespace::{CreateNodeRequest, GetNodeRequest};
use nodespace_daemon::{NodeServiceImpl, NodeServiceServer};
use tempfile::TempDir;
use tokio::net::UnixListener;
use tokio::sync::oneshot;
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
    let service = NodeServiceImpl::new(node_service, Arc::new(tokio::sync::RwLock::new(None)));

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
        if connect(&sock_path).await.is_ok() {
            return (sock_path, shutdown_tx, tempdir);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!(
        "daemon did not start accepting connections on {}",
        sock_path.display()
    );
}

#[tokio::test]
async fn create_get_update_children_delete_round_trip() {
    let (sock, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock).await.expect("connect");

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

    let mut raw_client = connect(&sock).await.expect("raw client connect");

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
            content: "parent updated via CLI".into(),
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
    assert_eq!(
        children.nodes[0].parent_id.as_deref(),
        Some(parent_id.as_str())
    );

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
    let mut client = connect(&sock).await.expect("connect");

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
    let mut client = connect(&sock).await.expect("connect");

    let err = commands::search::run(
        &mut client,
        commands::search::SearchArgs {
            query: "anything".into(),
            node_types: vec![],
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
    let (sock, shutdown, tempdir) = spawn_test_daemon().await;
    let mut client = connect(&sock).await.expect("connect");
    let mut raw_client = connect(&sock).await.expect("raw client connect");

    let db_path = tempdir.path().join("daemon-db");
    let baseline = commands::diagnostics::collect(&mut client, &db_path).await;
    assert!(
        baseline.errors.is_empty(),
        "baseline collect must not produce errors: {:?}",
        baseline.errors
    );

    let root = raw_client
        .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
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
        last_child_id = raw_client
            .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
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

    let report = commands::diagnostics::collect(&mut client, &db_path).await;
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
    assert!(report.database_exists);
    assert!(report.database_size_bytes.unwrap_or(0) > 0);
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
    let err = connect(std::path::Path::new("/tmp/nodespace-no-such-daemon.sock"))
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
    let mut client = connect(&sock).await.expect("connect");
    let mut raw = connect(&sock).await.expect("raw connect");

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
    let mut client = connect(&sock).await.expect("connect");
    let mut raw = connect(&sock).await.expect("raw connect");

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
    let mut client = connect(&sock).await.expect("connect");
    let mut raw = connect(&sock).await.expect("raw connect");

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
    let mut client = connect(&sock).await.expect("connect");
    let mut raw = connect(&sock).await.expect("raw connect");

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
    let mut client = connect(&sock).await.expect("connect");

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
