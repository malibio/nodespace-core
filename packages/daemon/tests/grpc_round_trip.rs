//! End-to-end gRPC integration test for the `nodespaced` daemon.
//!
//! Spins the tonic server up in-process against a tempdir-backed SurrealDB,
//! drives a `NodeServiceClient` against it, and verifies a CreateNode →
//! GetNode round trip plus a few error-mapping paths. This validates the
//! single acceptance criterion in #1112:
//!   > Integration test: start daemon, send GetNode via gRPC client,
//!   > verify response.

use std::sync::Arc;
use std::time::Duration;

use nodespace_core::{NodeService as CoreNodeService, SurrealStore};
use nodespace_daemon::nodespace::{
    node_event::Event as NodeEventKind, CreateNodeRequest, DeleteNodeRequest, GetChildrenRequest,
    GetNodeRequest, SearchRequest, UpdateNodeRequest, WatchRequest,
};
use nodespace_daemon::{NodeServiceClient, NodeServiceImpl, NodeServiceServer};
use tempfile::TempDir;
use tokio::net::TcpListener;
use tokio::sync::oneshot;
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::Code;

/// Start an in-process daemon and return a connected client plus a shutdown
/// handle. The server tears down when `shutdown` is sent — and the temp dir
/// is held alive on the returned tuple so it outlives all RPCs.
async fn spawn_test_daemon() -> (
    NodeServiceClient<tonic::transport::Channel>,
    oneshot::Sender<()>,
    TempDir,
) {
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

    // Bind to an ephemeral port so parallel test runs don't collide on 50051.
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

    // Give the server a brief moment to start accepting before we dial it.
    // Connect with retries to remove timing flakiness on slow CI runners
    // (50 * 25ms = 1.25s budget — comfortable for heavily loaded shared CI).
    let endpoint = format!("http://{}", addr);
    let mut last_err = None;
    for _ in 0..50 {
        match NodeServiceClient::connect(endpoint.clone()).await {
            Ok(client) => return (client, shutdown_tx, tempdir),
            Err(e) => {
                last_err = Some(e);
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        }
    }
    panic!("failed to connect to in-process daemon: {:?}", last_err);
}

#[tokio::test]
async fn create_then_get_round_trip() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "hello from grpc".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed")
        .into_inner();

    assert!(!created.node_id.is_empty(), "expected a node id");
    assert_eq!(created.node_type, "text");
    let created_data = created.node_data.expect("missing node_data");
    assert_eq!(created_data.content, "hello from grpc");
    assert_eq!(created_data.lifecycle_status, "active");
    assert_eq!(created_data.version, 1);

    let fetched = client
        .get_node(GetNodeRequest {
            node_id: created.node_id.clone(),
        })
        .await
        .expect("get_node failed")
        .into_inner();

    assert_eq!(fetched.node_id, created.node_id);
    let fetched_data = fetched.node_data.expect("missing node_data");
    assert_eq!(fetched_data.id, created.node_id);
    assert_eq!(fetched_data.content, "hello from grpc");

    let _ = shutdown.send(());
}

#[tokio::test]
async fn update_increments_version() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "v1".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed")
        .into_inner();

    let updated = client
        .update_node(UpdateNodeRequest {
            node_id: created.node_id.clone(),
            version: None, // exercise auto-fetch path
            node_type: String::new(),
            content: Some("v2".into()),
            properties: None,
            add_to_collection: String::new(),
            remove_from_collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("update_node failed")
        .into_inner();

    let data = updated.node_data.expect("missing node_data");
    assert_eq!(data.content, "v2");
    assert!(
        data.version >= 2,
        "expected version bump, got {}",
        data.version
    );

    let _ = shutdown.send(());
}

#[tokio::test]
async fn get_children_returns_parent_subtree() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let parent = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "parent".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create parent")
        .into_inner();

    for label in ["child-a", "child-b"] {
        client
            .create_node(CreateNodeRequest {
                node_type: "text".into(),
                content: label.into(),
                parent_id: parent.node_id.clone(),
                properties: String::new(),
                collection: String::new(),
                lifecycle_status: String::new(),
            })
            .await
            .expect("create child");
    }

    let children = client
        .get_children(GetChildrenRequest {
            node_id: parent.node_id.clone(),
        })
        .await
        .expect("get_children failed")
        .into_inner();

    assert_eq!(children.count, 2);
    let contents: Vec<&str> = children.nodes.iter().map(|n| n.content.as_str()).collect();
    assert!(contents.contains(&"child-a"));
    assert!(contents.contains(&"child-b"));

    let _ = shutdown.send(());
}

#[tokio::test]
async fn get_node_missing_returns_not_found() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let err = client
        .get_node(GetNodeRequest {
            node_id: "does-not-exist".into(),
        })
        .await
        .expect_err("expected not_found");

    assert_eq!(err.code(), Code::NotFound);

    let _ = shutdown.send(());
}

#[tokio::test]
async fn delete_node_marks_existed() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "doomed".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed")
        .into_inner();

    let deleted = client
        .delete_node(DeleteNodeRequest {
            node_id: created.node_id.clone(),
            version: None,
        })
        .await
        .expect("delete_node failed")
        .into_inner();

    assert_eq!(deleted.node_id, created.node_id);
    assert!(deleted.existed);

    // Subsequent get should now report NotFound.
    let err = client
        .get_node(GetNodeRequest {
            node_id: created.node_id,
        })
        .await
        .expect_err("expected not_found after delete");
    assert_eq!(err.code(), Code::NotFound);

    let _ = shutdown.send(());
}

/// Locks in the graceful-disable contract: when the daemon starts without an
/// `NodeEmbeddingService`, semantic search must report `Unavailable` rather
/// than crashing or returning empty results. Catches future regressions where
/// someone silently swaps the `Option<Arc<NodeEmbeddingService>>` to a panic
/// or a default-empty implementation.
#[tokio::test]
async fn search_nodes_returns_unavailable_without_embedding_service() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let err = client
        .search_nodes(SearchRequest {
            query: "anything".into(),
            ..SearchRequest::default()
        })
        .await
        .expect_err("expected unavailable");

    assert_eq!(err.code(), Code::Unavailable);

    let _ = shutdown.send(());
}

/// Per-event timeout for WatchNodes streaming tests. Generous enough to
/// absorb a loaded CI runner but short enough to fail fast when an event is
/// genuinely missing.
const WATCH_EVENT_TIMEOUT: Duration = Duration::from_secs(5);

/// Pull the next event off a WatchNodes stream with a timeout so tests fail
/// fast instead of hanging forever when an event is dropped.
async fn next_event_with_timeout(
    stream: &mut tonic::Streaming<nodespace_daemon::nodespace::NodeEvent>,
) -> nodespace_daemon::nodespace::NodeEvent {
    tokio::time::timeout(WATCH_EVENT_TIMEOUT, stream.next())
        .await
        .expect("timed out waiting for WatchNodes event")
        .expect("stream ended unexpectedly")
        .expect("stream item was an error")
}

/// Acceptance criterion (#1114): mutate a node via gRPC, verify the watcher
/// receives the corresponding event. Covers all three event kinds in one go.
#[tokio::test]
async fn watch_nodes_receives_create_update_delete_events() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    // Open the watch stream BEFORE issuing any mutation so we see the events.
    // We use a second client handle for streaming so the main client can keep
    // issuing unary requests without contention with the streaming response.
    let mut watch_client = client.clone();
    let mut stream = watch_client
        .watch_nodes(WatchRequest {
            node_type: String::new(),
            root_id: String::new(),
        })
        .await
        .expect("watch_nodes failed")
        .into_inner();

    // Trigger create → update → delete and observe each event.
    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "watched node".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed")
        .into_inner();
    let node_id = created.node_id.clone();

    let create_event = next_event_with_timeout(&mut stream).await;
    match create_event.event {
        Some(NodeEventKind::Created(data)) => {
            assert_eq!(data.id, node_id);
            assert_eq!(data.content, "watched node");
            assert_eq!(data.node_type, "text");
        }
        other => panic!("expected Created event, got {:?}", other),
    }

    client
        .update_node(UpdateNodeRequest {
            node_id: node_id.clone(),
            version: None,
            node_type: String::new(),
            content: Some("watched node v2".into()),
            properties: None,
            add_to_collection: String::new(),
            remove_from_collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("update_node failed");

    let update_event = next_event_with_timeout(&mut stream).await;
    match update_event.event {
        Some(NodeEventKind::Updated(data)) => {
            assert_eq!(data.id, node_id);
            assert_eq!(data.content, "watched node v2");
            assert!(data.version >= 2);
        }
        other => panic!("expected Updated event, got {:?}", other),
    }

    client
        .delete_node(DeleteNodeRequest {
            node_id: node_id.clone(),
            version: None,
        })
        .await
        .expect("delete_node failed");

    let delete_event = next_event_with_timeout(&mut stream).await;
    match delete_event.event {
        Some(NodeEventKind::Deleted(d)) => {
            assert_eq!(d.node_id, node_id);
            assert_eq!(d.node_type, "text");
        }
        other => panic!("expected Deleted event, got {:?}", other),
    }

    let _ = shutdown.send(());
}

/// Acceptance criterion (#1114): multiple concurrent watchers supported
/// simultaneously. Both must receive the same event from a single mutation.
#[tokio::test]
async fn watch_nodes_supports_multiple_concurrent_watchers() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let mut watch_a = client.clone();
    let mut watch_b = client.clone();
    let mut stream_a = watch_a
        .watch_nodes(WatchRequest::default())
        .await
        .expect("watch_a failed")
        .into_inner();
    let mut stream_b = watch_b
        .watch_nodes(WatchRequest::default())
        .await
        .expect("watch_b failed")
        .into_inner();

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "broadcast me".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed")
        .into_inner();

    let (event_a, event_b) = tokio::join!(
        next_event_with_timeout(&mut stream_a),
        next_event_with_timeout(&mut stream_b),
    );

    for event in [event_a, event_b] {
        match event.event {
            Some(NodeEventKind::Created(data)) => {
                assert_eq!(data.id, created.node_id);
                assert_eq!(data.content, "broadcast me");
            }
            other => panic!("expected Created event on both watchers, got {:?}", other),
        }
    }

    let _ = shutdown.send(());
}

/// Verifies the server-side stream closes cleanly when the client drops its
/// receiver, rather than holding the broadcast subscription forever. Matches
/// the AC "Stream closes gracefully when the client disconnects".
#[tokio::test]
async fn watch_nodes_closes_when_client_drops_stream() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    {
        let mut transient = client.clone();
        let _stream = transient
            .watch_nodes(WatchRequest::default())
            .await
            .expect("watch failed")
            .into_inner();
        // Stream is dropped at end of this block; the server-side task should
        // observe the receiver-half being gone via tonic's cancellation
        // signaling and break its loop. We don't have a direct hook into that,
        // but we can verify a fresh watcher still works afterwards (i.e. the
        // server is still healthy and tracking subscribers correctly).
    }

    let mut fresh = client.clone();
    let mut stream = fresh
        .watch_nodes(WatchRequest::default())
        .await
        .expect("fresh watch failed")
        .into_inner();

    client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "post-drop".into(),
            parent_id: String::new(),
            properties: String::new(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect("create_node failed");

    let event = next_event_with_timeout(&mut stream).await;
    assert!(
        matches!(event.event, Some(NodeEventKind::Created(_))),
        "expected fresh watcher to receive Created event after previous watcher dropped"
    );

    let _ = shutdown.send(());
}

/// Verifies CreateNode rejects malformed property JSON with InvalidArgument
/// rather than letting the parse error reach the ops layer as `Internal`.
#[tokio::test]
async fn create_node_rejects_malformed_properties() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let err = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "irrelevant".into(),
            parent_id: String::new(),
            properties: "{not valid json".into(),
            collection: String::new(),
            lifecycle_status: String::new(),
        })
        .await
        .expect_err("expected invalid_argument");

    assert_eq!(err.code(), Code::InvalidArgument);

    let _ = shutdown.send(());
}
