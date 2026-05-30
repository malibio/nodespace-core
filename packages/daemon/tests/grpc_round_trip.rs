//! End-to-end gRPC integration test for the `nodespaced` daemon.
//!
//! Spins the tonic server up in-process against a tempdir-backed SQLite database,
//! drives a `NodeServiceClient` against it, and verifies a CreateNode →
//! GetNode round trip plus a few error-mapping paths. This validates the
//! single acceptance criterion in #1112:
//!   > Integration test: start daemon, send GetNode via gRPC client,
//!   > verify response.

use std::sync::Arc;
use std::time::Duration;

use nodespace_core::{NodeService as CoreNodeService, SqliteStore};
use nodespace_daemon::nodespace::{
    create_node_request::Position as CreatePos, node_event::Event as NodeEventKind,
    reorder_node_request::Position as ReorderPos, CreateCollectionRequest, CreateNodeRequest,
    DeleteNodeRequest, GetChildrenRequest, GetNodeRequest, NodeCollectionsRequest,
    ReorderNodeRequest, SearchRequest, UpdateNodeRequest, WatchRequest,
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
        SqliteStore::new(tempdir.path().join("daemon-db"))
            .await
            .expect("failed to open SqliteStore"),
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create_node failed")
        .into_inner();

    let updated = client
        .update_node(UpdateNodeRequest {
            node_id: created.node_id.clone(),
            version: None, // exercise auto-fetch path
            node_type: None,
            content: Some("v2".into()),
            properties: None,
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create parent")
        .into_inner();

    for label in ["child-a", "child-b"] {
        client
            .create_node(CreateNodeRequest {
                node_type: "text".into(),
                content: label.into(),
                parent_id: Some(parent.node_id.clone()),
                properties: String::new(),
                collection: None,
                lifecycle_status: None,
                id: None,
                position: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
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
            node_type: None,
            content: Some("watched node v2".into()),
            properties: None,
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
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
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
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
            parent_id: None,
            properties: "{not valid json".into(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect_err("expected invalid_argument");

    assert_eq!(err.code(), Code::InvalidArgument);

    let _ = shutdown.send(());
}

/// Verify that `position: Beginning` places a second node before the first.
#[tokio::test]
async fn test_insert_position_beginning_and_after_proto_decoding() {
    let (mut client, shutdown, _dir) = spawn_test_daemon().await;

    // Create parent
    let parent_resp = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "Parent".into(),
            parent_id: None,
            properties: "{}".into(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create parent")
        .into_inner();
    let parent_id = parent_resp.node_id;

    // Child A — appended via default (End)
    let child_a_resp = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "Child A".into(),
            parent_id: Some(parent_id.clone()),
            properties: "{}".into(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: Some(CreatePos::End(true)),
        })
        .await
        .expect("create child A")
        .into_inner();
    let child_a_id = child_a_resp.node_id;

    // Child B — inserted at Beginning, should become first
    client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "Child B".into(),
            parent_id: Some(parent_id.clone()),
            properties: "{}".into(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: Some(CreatePos::Beginning(true)),
        })
        .await
        .expect("create child B");

    let children = client
        .get_children(GetChildrenRequest {
            node_id: parent_id.clone(),
        })
        .await
        .expect("get children")
        .into_inner();

    let nodes = children.nodes;
    assert_eq!(nodes.len(), 2, "parent should have 2 children");
    assert_eq!(
        nodes[0].content, "Child B",
        "Beginning: Child B should be first"
    );
    assert_eq!(nodes[1].content, "Child A", "End: Child A should be second");

    // Now reorder Child A to Beginning via ReorderNode
    client
        .reorder_node(ReorderNodeRequest {
            node_id: child_a_id.clone(),
            version: 1,
            position: Some(ReorderPos::Beginning(true)),
        })
        .await
        .expect("reorder child A to beginning");

    let children2 = client
        .get_children(GetChildrenRequest {
            node_id: parent_id.clone(),
        })
        .await
        .expect("get children after reorder")
        .into_inner();

    let nodes2 = children2.nodes;
    assert_eq!(
        nodes2[0].content, "Child A",
        "After reorder, Child A should be first"
    );
    assert_eq!(
        nodes2[1].content, "Child B",
        "After reorder, Child B should be second"
    );

    // Reorder Child B to After(Child A) — B should end up second again
    client
        .reorder_node(ReorderNodeRequest {
            node_id: nodes2[1].id.clone(), // Child B
            version: 1,
            position: Some(ReorderPos::After(child_a_id.clone())),
        })
        .await
        .expect("reorder child B after child A");

    let children3 = client
        .get_children(GetChildrenRequest {
            node_id: parent_id.clone(),
        })
        .await
        .expect("get children after After reorder")
        .into_inner();

    assert_eq!(children3.nodes[0].content, "Child A");
    assert_eq!(children3.nodes[1].content, "Child B");

    let _ = shutdown.send(());
}

/// Parity test (AC #1241): create a node with a collection path and a non-default
/// lifecycle status via the gRPC RPC. Verifies the daemon delegates to node_ops
/// correctly — the node lands in the resolved collection and has the requested
/// lifecycle status.
#[tokio::test]
async fn create_node_with_collection_and_lifecycle_status() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    // Create the collection node first so the path resolves.
    let _coll = client
        .create_collection(CreateCollectionRequest {
            name: "test-collection".into(),
            description: String::new(),
        })
        .await
        .expect("create_collection failed");

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "parity test node".into(),
            parent_id: None,
            properties: String::new(),
            collection: Some("test-collection".into()),
            lifecycle_status: Some("archived".into()),
            id: None,
            position: None,
        })
        .await
        .expect("create_node failed")
        .into_inner();

    let data = created.node_data.expect("missing node_data");
    assert_eq!(
        data.lifecycle_status, "archived",
        "lifecycle_status must be set"
    );
    assert!(
        !created.collection_id.is_empty(),
        "collection_id should be set"
    );

    // Verify the node is actually a member of the collection via a separate RPC.
    let memberships = client
        .get_node_collections(NodeCollectionsRequest {
            node_id: created.node_id.clone(),
        })
        .await
        .expect("get_node_collections failed")
        .into_inner();
    assert!(
        !memberships.collection_ids.is_empty(),
        "node should belong to at least one collection"
    );

    let _ = shutdown.send(());
}

/// Parity test (AC #1241): update a node without supplying version — the
/// daemon delegates to node_ops, which auto-fetches the current version.
#[tokio::test]
async fn update_node_auto_fetches_version_when_omitted() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let created = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "before".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create_node failed")
        .into_inner();

    // Omit version — node_ops auto-fetches it; no VersionConflict expected.
    let updated = client
        .update_node(UpdateNodeRequest {
            node_id: created.node_id.clone(),
            version: None,
            node_type: None,
            content: Some("after".into()),
            properties: None,
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        })
        .await
        .expect("update_node without version failed")
        .into_inner();

    let data = updated.node_data.expect("missing node_data");
    assert_eq!(data.content, "after");
    assert!(data.version >= 2, "version must be bumped after update");

    let _ = shutdown.send(());
}

/// Parity test (AC #1241): add then remove a collection membership via
/// update_node's add_to_collection / remove_from_collection fields.
#[tokio::test]
async fn update_node_add_then_remove_collection_membership() {
    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    let _coll = client
        .create_collection(CreateCollectionRequest {
            name: "membership-test".into(),
            description: String::new(),
        })
        .await
        .expect("create_collection failed")
        .into_inner();
    let collection_id = _coll.collection_id;

    let node = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "membership node".into(),
            parent_id: None,
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create_node failed")
        .into_inner();
    let node_id = node.node_id.clone();

    // Add to collection via update_node.
    client
        .update_node(UpdateNodeRequest {
            node_id: node_id.clone(),
            version: None,
            node_type: None,
            content: None,
            properties: None,
            add_to_collection: Some("membership-test".into()),
            remove_from_collection: None,
            lifecycle_status: None,
        })
        .await
        .expect("add_to_collection failed");

    let after_add = client
        .get_node_collections(NodeCollectionsRequest {
            node_id: node_id.clone(),
        })
        .await
        .expect("get_node_collections failed")
        .into_inner();
    assert!(
        after_add.collection_ids.contains(&collection_id),
        "node should be in collection after add"
    );

    // Remove from collection via update_node.
    client
        .update_node(UpdateNodeRequest {
            node_id: node_id.clone(),
            version: None,
            node_type: None,
            content: None,
            properties: None,
            add_to_collection: None,
            remove_from_collection: Some(collection_id.clone()),
            lifecycle_status: None,
        })
        .await
        .expect("remove_from_collection failed");

    let after_remove = client
        .get_node_collections(NodeCollectionsRequest {
            node_id: node_id.clone(),
        })
        .await
        .expect("get_node_collections failed")
        .into_inner();
    assert!(
        !after_remove.collection_ids.contains(&collection_id),
        "node should not be in collection after remove"
    );

    let _ = shutdown.send(());
}

/// B5 AC: move_node with new_parent_id unset (None) must move to root.
/// Verifies the optional field semantics: unset = root, present = reparent.
#[tokio::test]
async fn move_node_to_root_when_new_parent_id_unset() {
    use nodespace_daemon::nodespace::{GetRootsRequest, MoveNodeRequest};

    let (mut client, shutdown, _tempdir) = spawn_test_daemon().await;

    // Create a parent node and a child.
    let parent = client
        .create_node(CreateNodeRequest {
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
        .expect("create parent")
        .into_inner();

    let child = client
        .create_node(CreateNodeRequest {
            node_type: "text".into(),
            content: "child".into(),
            parent_id: Some(parent.node_id.clone()),
            properties: String::new(),
            collection: None,
            lifecycle_status: None,
            id: None,
            position: None,
        })
        .await
        .expect("create child")
        .into_inner();

    // Move child to root — new_parent_id = None (unset).
    client
        .move_node(MoveNodeRequest {
            node_id: child.node_id.clone(),
            version: 1,
            new_parent_id: None,
            position: None,
        })
        .await
        .expect("move_node to root failed");

    // Child should now appear in roots.
    let roots = client
        .get_roots(GetRootsRequest {
            limit: 100,
            offset: 0,
        })
        .await
        .expect("get_roots failed")
        .into_inner();
    let root_ids: Vec<&str> = roots.nodes.iter().map(|n| n.id.as_str()).collect();
    assert!(
        root_ids.contains(&child.node_id.as_str()),
        "child should be a root node after move with new_parent_id=None"
    );

    // Original parent should have no children.
    let children_of_parent = client
        .get_children(GetChildrenRequest {
            node_id: parent.node_id.clone(),
        })
        .await
        .expect("get_children failed")
        .into_inner();
    assert_eq!(
        children_of_parent.count, 0,
        "original parent should have no children after move"
    );

    let _ = shutdown.send(());
}
