//! End-to-end integration test for the `nodespace` CLI.
//!
//! Spins an in-process `nodespaced` gRPC server up against a tempdir-backed
//! SurrealDB, then drives the CLI's command handlers (via the library
//! surface) at it. This validates that the CLI's gRPC plumbing — connection,
//! request construction, response unwrapping, error mapping — works end to
//! end against the same service stack the real binary uses.
//!
//! We exercise the handlers directly rather than spawning the compiled
//! binary so test failures point at the code path under test rather than
//! at fork/exec or stdout-capture plumbing.

use std::sync::Arc;
use std::time::Duration;

use nodespace_cli::{commands, connect};
use nodespace_core::{NodeService as CoreNodeService, SurrealStore};
use nodespace_daemon::nodespace::GetNodeRequest;
use nodespace_daemon::{NodeServiceImpl, NodeServiceServer};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tonic::transport::Server;
use tonic::Code;

/// Spawn an in-process daemon and return the endpoint URL the CLI should
/// dial. Mirrors `packages/daemon/tests/grpc_round_trip.rs::spawn_test_daemon`
/// but exposes the endpoint string rather than a pre-built client because the
/// CLI owns its own client construction.
async fn spawn_test_daemon() -> (String, oneshot::Sender<()>, TempDir) {
    let tempdir = TempDir::new().expect("failed to create tempdir");

    let mut store = Arc::new(
        SurrealStore::new(tempdir.path().join("daemon-db"))
            .await
            .expect("failed to open SurrealStore"),
    );
    let node_service = Arc::new(
        CoreNodeService::new(&mut store)
            .await
            .expect("failed to build NodeService"),
    );
    let service = NodeServiceImpl::new(node_service, None);

    // Ephemeral port — parallel test runs must not collide on 50051.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("failed to bind ephemeral port");
    let addr = listener.local_addr().expect("local_addr");
    let incoming = tokio_stream::wrappers::TcpListenerStream::new(listener);

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

    let endpoint = format!("http://{}", addr);

    // Wait for the server to start accepting before handing back the endpoint.
    // The CLI's `connect` helper already retries via tonic, but giving the
    // listener a moment removes spurious connect errors on slow CI runners.
    for _ in 0..50 {
        if connect(&endpoint).await.is_ok() {
            return (endpoint, shutdown_tx, tempdir);
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("daemon did not start accepting connections on {}", endpoint);
}

#[tokio::test]
async fn create_get_update_children_delete_round_trip() {
    let (endpoint, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&endpoint).await.expect("connect");

    // create root node
    commands::node::run(
        &mut client,
        commands::node::NodeAction::Create(commands::node::CreateArgs {
            node_type: "text".into(),
            content: "root via CLI".into(),
            parent: None,
        }),
        true, // json mode keeps stdout machine-readable, but mostly we care that no errors propagate
    )
    .await
    .expect("create root");

    // grab an ID by listing the root's children (it has none) and then by
    // creating one via a direct gRPC call so we can run the rest of the
    // commands against a known id without parsing stdout.
    let mut raw_client = nodespace_daemon::NodeServiceClient::connect(endpoint.clone())
        .await
        .expect("raw client connect");

    let created = raw_client
        .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
            node_type: "text".into(),
            content: "parent".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
            id: String::new(),
            insert_after_node_id: String::new(),
        })
        .await
        .expect("seed parent")
        .into_inner();
    let parent_id = created.node_id;

    // create a child under that parent via the CLI handler
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

    // get the parent
    commands::node::run(
        &mut client,
        commands::node::NodeAction::Get(commands::node::GetArgs {
            id: parent_id.clone(),
        }),
        false,
    )
    .await
    .expect("get parent");

    // update the parent's content
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

    // verify update took effect via a direct gRPC fetch — the CLI handler
    // only prints, so we check the underlying state independently
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

    // list children — exercise the CLI handler, then verify via raw gRPC
    // that the response shape the handler renders is correct. The handler
    // itself only prints, so we cross-check the underlying state.
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
    assert_eq!(
        children.nodes.len(),
        1,
        "nodes len must match count for non-paginated results"
    );
    assert_eq!(
        children.nodes[0].content, "child via CLI",
        "child content should match what the CLI handler created"
    );
    assert_eq!(
        children.nodes[0].parent_id.as_deref(),
        Some(parent_id.as_str()),
        "child must link back to the seeded parent"
    );

    // delete the parent
    commands::node::run(
        &mut client,
        commands::node::NodeAction::Delete(commands::node::DeleteArgs {
            id: parent_id.clone(),
        }),
        false,
    )
    .await
    .expect("delete parent");

    // confirm gone
    let err = raw_client
        .get_node(GetNodeRequest { node_id: parent_id })
        .await
        .expect_err("expected not_found after delete");
    assert_eq!(err.code(), Code::NotFound);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn get_missing_node_surfaces_not_found() {
    let (endpoint, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&endpoint).await.expect("connect");

    let err = commands::node::run(
        &mut client,
        commands::node::NodeAction::Get(commands::node::GetArgs {
            id: "does-not-exist".into(),
        }),
        false,
    )
    .await
    .expect_err("expected error");

    // anyhow wraps the tonic Status; make sure the original code survives in
    // the chain so users can still distinguish missing-node from transport
    // failures when they inspect the cause.
    let status = err
        .chain()
        .find_map(|e| e.downcast_ref::<tonic::Status>())
        .expect("expected tonic::Status in error chain");
    assert_eq!(status.code(), Code::NotFound);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn search_without_embedding_service_reports_unavailable() {
    let (endpoint, shutdown, _tempdir) = spawn_test_daemon().await;
    let mut client = connect(&endpoint).await.expect("connect");

    let err = commands::search::run(
        &mut client,
        commands::search::SearchArgs {
            query: "anything".into(),
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
    let (endpoint, shutdown, tempdir) = spawn_test_daemon().await;
    let mut client = connect(&endpoint).await.expect("connect");
    let mut raw_client = nodespace_daemon::NodeServiceClient::connect(endpoint.clone())
        .await
        .expect("raw client connect");

    // Capture the pre-seed baseline. NodeService::new auto-seeds core
    // schema records on first open, so a fresh daemon already has nodes —
    // we assert deltas against this baseline rather than absolute totals.
    let db_path = tempdir.path().join("daemon-db");
    let baseline = commands::diagnostics::collect(&mut client, &db_path).await;
    assert!(
        baseline.errors.is_empty(),
        "baseline collect must not produce errors: {:?}",
        baseline.errors
    );

    // Seed one root and two children. The diagnostics report should reflect
    // +3 total and rank the newest-created child first.
    let root = raw_client
        .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
            node_type: "text".into(),
            content: "root".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
            id: String::new(),
            insert_after_node_id: String::new(),
        })
        .await
        .expect("seed root")
        .into_inner();

    let mut last_child_id = String::new();
    for label in ["child-1", "child-2"] {
        // Brief sleep so created_at timestamps differ — gives the
        // created_at DESC sort a stable, observable order.
        tokio::time::sleep(Duration::from_millis(20)).await;
        last_child_id = raw_client
            .create_node(nodespace_daemon::nodespace::CreateNodeRequest {
                node_type: "text".into(),
                content: label.into(),
                parent_id: root.node_id.clone(),
                properties: String::new(),
                collection: String::new(),
                lifecycle_status: String::new(),
                id: String::new(),
                insert_after_node_id: String::new(),
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
    assert!(
        report.database_exists,
        "daemon-db directory must exist after node creation"
    );
    assert!(
        report.database_size_bytes.unwrap_or(0) > 0,
        "non-empty database should report a non-zero size"
    );
    assert_eq!(
        report.recent_node_ids[0], last_child_id,
        "newest-created node must appear first under created_at DESC ordering"
    );
    assert!(
        report.errors.is_empty(),
        "happy-path collect must not surface errors: {:?}",
        report.errors
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn connect_refused_returns_friendly_error() {
    // A port reserved by IANA that no daemon should be on. The CLI's `connect`
    // helper must surface a friendly message rather than a raw transport error.
    let err = connect("http://127.0.0.1:1")
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
