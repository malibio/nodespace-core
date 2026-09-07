//! Object-valued properties must round-trip on every type, not just `task`.
//!
//! Storage nests a node's own fields under its type key
//! (`{"venue": {"capacity": 250}}`), and every read path flattens that key back
//! out. A field whose value happens to be a JSON object is indistinguishable by
//! shape from a *sibling namespace* — another type's dormant key left beside the
//! active one — so a normalizer that classified on the value's type had to guess,
//! and guessed "namespace". The field landed outside the type key, where the
//! flattener never looks, and disappeared from output with no error and no null:
//! a documented input shape (`--property address='{"city":"Berlin"}'`) that
//! silently lost data.
//!
//! The classification now keys off the property name's `_` prefix instead, which
//! is unambiguous: `_`-prefixed keys are internal bookkeeping that belongs at the
//! top level, everything else is a field of this type whatever its value.
//!
//! These tests pin the round-trip end to end for a user-defined type, through
//! each of the three read surfaces that flatten independently — `get_node`,
//! `query_nodes`, and `get_related_nodes` — because they reach the flattener by
//! different routes and could drift apart.

use anyhow::Result;
use nodespace_core::{
    db::SqliteStore,
    models::Node,
    ops::{node_ops, rel_ops},
    schema::handle_create_schema,
    services::{InsertPositionOwned, NodeService},
};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_service() -> Result<(Arc<NodeService>, TempDir)> {
    let temp_dir = TempDir::new()?;
    let db_path = temp_dir.path().join("test.db");
    let mut store = Arc::new(SqliteStore::new(db_path).await?);
    let node_service = Arc::new(NodeService::new(&mut store).await?);
    Ok((node_service, temp_dir))
}

/// A user-defined type with an `object` field — the shape with no built-in
/// equivalent, and the one that used to lose its value.
async fn seed_venue_schema(svc: &Arc<NodeService>) -> Result<()> {
    handle_create_schema(
        svc,
        json!({
            "name": "Venue",
            "fields": [
                { "name": "address", "type": "object", "protection": "user", "indexed": false },
                { "name": "capacity", "type": "number", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("venue schema: {e}"))?;
    Ok(())
}

const ADDRESS: fn() -> serde_json::Value =
    || json!({ "city": "Berlin", "street": "Torstrasse 1", "postal": { "code": "10119" } });

async fn create_venue(svc: &Arc<NodeService>) -> Result<String> {
    let output = node_ops::create_node(
        svc,
        node_ops::CreateNodeInput {
            id: None,
            node_type: "venue".to_string(),
            content: "Berghain".to_string(),
            parent_id: None,
            position: InsertPositionOwned::End,
            properties: json!({ "address": ADDRESS(), "capacity": 1500 }),
            collection: None,
            lifecycle_status: None,
        },
    )
    .await?;
    Ok(output.node_id)
}

fn properties_of(node: &serde_json::Value) -> &serde_json::Value {
    node.get("properties")
        .expect("read output must carry properties")
}

#[tokio::test]
async fn object_property_round_trips_through_get_node() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_venue_schema(&svc).await?;
    let id = create_venue(&svc).await?;

    let node = node_ops::get_node(
        &svc,
        node_ops::GetNodeInput {
            node_id: id.clone(),
        },
    )
    .await?;

    let props = properties_of(&node);
    assert_eq!(
        props.get("address"),
        Some(&ADDRESS()),
        "the object-valued field must survive the read, nested value included"
    );
    assert_eq!(
        props.get("capacity"),
        Some(&json!(1500)),
        "a scalar sibling must be unaffected"
    );
    assert!(
        props.get("venue").is_none(),
        "storage namespacing must not be observable on the read surface"
    );
    Ok(())
}

#[tokio::test]
async fn object_property_round_trips_through_query() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_venue_schema(&svc).await?;
    create_venue(&svc).await?;

    let out = node_ops::query_nodes(
        &svc,
        node_ops::QueryNodesInput {
            node_type: Some("venue".to_string()),
            parent_id: None,
            root_id: None,
            limit: None,
            offset: None,
            collection_id: None,
            collection: None,
            filters: None,
        },
    )
    .await?;

    assert_eq!(out.count, 1, "the venue must be queryable");
    assert_eq!(
        properties_of(&out.nodes[0]).get("address"),
        Some(&ADDRESS()),
        "query must expose the object-valued field like every other read path"
    );
    Ok(())
}

#[tokio::test]
async fn object_property_round_trips_through_relationship_get() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_venue_schema(&svc).await?;

    handle_create_schema(
        &svc,
        json!({
            "name": "Concert",
            "fields": [{ "name": "billing", "type": "string", "protection": "user", "indexed": false }],
            "relationships": [{
                "name": "held_at",
                "targetType": "venue",
                "direction": "out",
                "cardinality": "one"
            }]
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("concert schema: {e}"))?;

    let venue_id = create_venue(&svc).await?;
    let concert_id = uuid::Uuid::new_v4().to_string();
    svc.create_node(Node::new_with_id(
        concert_id.clone(),
        "concert".to_string(),
        "Klubnacht".to_string(),
        json!({ "billing": "headline" }),
    ))
    .await?;

    svc.create_relationship(&concert_id, "held_at", &venue_id, json!({}))
        .await?;

    let out = rel_ops::get_related_nodes(
        &svc,
        rel_ops::GetRelatedInput {
            node_id: concert_id,
            relationship_name: "held_at".to_string(),
            direction: "out".to_string(),
        },
    )
    .await?;

    assert_eq!(out.related_nodes.len(), 1, "the venue must be reachable");
    assert_eq!(
        properties_of(&out.related_nodes[0]).get("address"),
        Some(&ADDRESS()),
        "relationship traversal must expose the object-valued field too"
    );
    Ok(())
}

/// An update must not lose the field either — the update path normalizes
/// separately from create, and used to pass no schema at all.
#[tokio::test]
async fn object_property_survives_an_update() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_venue_schema(&svc).await?;
    let id = create_venue(&svc).await?;

    let moved = json!({ "city": "Leipzig", "street": "Karl-Liebknecht-Str 2" });
    node_ops::update_node(
        &svc,
        node_ops::UpdateNodeInput {
            node_id: id.clone(),
            version: None,
            node_type: None,
            content: None,
            properties: Some(json!({ "address": moved })),
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        },
    )
    .await?;

    let node = node_ops::get_node(&svc, node_ops::GetNodeInput { node_id: id }).await?;
    let props = properties_of(&node);
    assert_eq!(
        props.get("address").and_then(|a| a.get("city")),
        Some(&json!("Leipzig")),
        "the updated object value must be readable back"
    );
    assert_eq!(
        props.get("capacity"),
        Some(&json!(1500)),
        "an untouched sibling must survive the deep merge"
    );
    Ok(())
}

/// `_`-prefixed keys are the one thing that stays outside the type namespace:
/// seed bookkeeping must land at a fixed, type-independent path rather than
/// being swept in with the type's own fields.
#[tokio::test]
async fn underscore_prefixed_keys_stay_out_of_the_type_namespace() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_venue_schema(&svc).await?;

    let id = uuid::Uuid::new_v4().to_string();
    svc.create_node(Node::new_with_id(
        id.clone(),
        "venue".to_string(),
        "Tresor".to_string(),
        json!({
            "address": ADDRESS(),
            "_seed": { "key": "Tresor", "tier": "system" }
        }),
    ))
    .await?;

    let stored = svc.get_node(&id).await?.expect("venue must exist");
    assert_eq!(
        stored.properties["venue"]["address"],
        ADDRESS(),
        "the field belongs inside the type namespace"
    );
    assert_eq!(
        stored.properties["_seed"]["key"], "Tresor",
        "_seed belongs at the top level, not inside the type namespace"
    );
    assert!(
        stored.properties["venue"].get("_seed").is_none(),
        "_seed must not be swept into the type namespace"
    );
    Ok(())
}
