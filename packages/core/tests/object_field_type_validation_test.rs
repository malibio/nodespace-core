//! `field_type` is declared but was never enforced except for `enum`: a field
//! declared `object` (or an `array` whose `itemType` is `object`) accepted any
//! JSON shape and stored it as-is, with no error and no warning.
//!
//! Scope, deliberately narrow: this closes the `object`-shape gap only —
//! structural validation that a field declared `object` holds a JSON object,
//! and that an `array` field declared `itemType: "object"` holds an array of
//! JSON objects. It does NOT validate every declared `field_type` (string,
//! number, boolean, date), and it does NOT recurse into a declared `object`
//! field's own `fields`/`item_fields` sub-schema — both are out of scope for
//! this fix; see the doc comment on the validation itself in
//! `packages/core/src/services/node_service/crud.rs` for the full reasoning.
//!
//! A survey of every writer of a declared `object`/`array<object>` field in
//! `core_schemas.rs` (`ai-chat.messages`, `query.filters`, `query.sorting`)
//! found no contradicting writer — every production write already sends a
//! correctly-shaped array of objects — so enabling this check is safe with no
//! accompanying writer fix needed, unlike the `ai-chat.status` precedent.

use anyhow::Result;
use nodespace_core::{
    db::SqliteStore,
    ops::node_ops,
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

async fn seed_widget_schema(svc: &Arc<NodeService>) -> Result<()> {
    handle_create_schema(
        svc,
        json!({
            "name": "Widget",
            "fields": [
                { "name": "address", "type": "object", "protection": "user", "indexed": false },
                {
                    "name": "notes",
                    "type": "array",
                    "itemType": "object",
                    "protection": "user",
                    "indexed": false
                }
            ]
        }),
    )
    .await
    .map_err(|e| anyhow::anyhow!("widget schema: {e}"))?;
    Ok(())
}

async fn create_widget(svc: &Arc<NodeService>, properties: serde_json::Value) -> Result<String> {
    let output = node_ops::create_node(
        svc,
        node_ops::CreateNodeInput {
            id: None,
            node_type: "widget".to_string(),
            content: "Sprocket".to_string(),
            parent_id: None,
            position: InsertPositionOwned::End,
            properties,
            collection: None,
            lifecycle_status: None,
        },
    )
    .await?;
    Ok(output.node_id)
}

#[tokio::test]
async fn object_field_rejects_a_string_value() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let err = create_widget(&svc, json!({ "address": "123 Main St" }))
        .await
        .expect_err("a string must not satisfy a field declared type object");

    let msg = err.to_string();
    assert!(
        msg.contains("address"),
        "error must name the field, got: {msg}"
    );
    assert!(
        msg.contains("object"),
        "error must name the declared type, got: {msg}"
    );
    assert!(
        msg.contains("string"),
        "error must say what was received, got: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn object_field_rejects_a_number_value() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let err = create_widget(&svc, json!({ "address": 42 }))
        .await
        .expect_err("a number must not satisfy a field declared type object");
    assert!(err.to_string().contains("address"));
    Ok(())
}

#[tokio::test]
async fn object_field_accepts_an_object_value() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let id = create_widget(&svc, json!({ "address": { "city": "Berlin" } })).await?;

    let node = node_ops::get_node(&svc, node_ops::GetNodeInput { node_id: id }).await?;
    assert_eq!(
        node.get("properties").and_then(|p| p.get("address")),
        Some(&json!({ "city": "Berlin" })),
        "a correctly-shaped object must be accepted and persisted"
    );
    Ok(())
}

#[tokio::test]
async fn object_field_accepts_an_explicit_null() -> Result<()> {
    // Mirrors the existing enum-field behavior: an explicit `null` on a
    // non-required field is a legal "no value", not a type violation.
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    create_widget(&svc, json!({ "address": null })).await?;
    Ok(())
}

#[tokio::test]
async fn array_object_field_rejects_a_non_array_value() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let err = create_widget(&svc, json!({ "notes": "not an array" }))
        .await
        .expect_err("a string must not satisfy a field declared array<object>");

    let msg = err.to_string();
    assert!(
        msg.contains("notes"),
        "error must name the field, got: {msg}"
    );
    assert!(
        msg.contains("array"),
        "error must name the declared type, got: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn array_object_field_rejects_a_non_object_item() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let err = create_widget(&svc, json!({ "notes": [{ "text": "ok" }, 5] }))
        .await
        .expect_err("a non-object array element must be rejected");

    let msg = err.to_string();
    assert!(
        msg.contains("notes"),
        "error must name the field, got: {msg}"
    );
    assert!(
        msg.contains('1'),
        "error should identify which item failed (index 1), got: {msg}"
    );
    Ok(())
}

#[tokio::test]
async fn array_object_field_accepts_an_array_of_objects() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;

    let notes = json!([{ "text": "first" }, { "text": "second" }]);
    let id = create_widget(&svc, json!({ "notes": notes.clone() })).await?;

    let node = node_ops::get_node(&svc, node_ops::GetNodeInput { node_id: id }).await?;
    assert_eq!(
        node.get("properties").and_then(|p| p.get("notes")),
        Some(&notes),
        "a correctly-shaped array of objects must be accepted and persisted"
    );
    Ok(())
}

/// The update path validates through a different call site
/// (`validate_node_against_schema`) than create — this pins that it enforces
/// the same rule rather than only the create path.
#[tokio::test]
async fn object_field_rejects_a_bad_value_on_update() -> Result<()> {
    let (svc, _tmp) = create_test_service().await?;
    seed_widget_schema(&svc).await?;
    let id = create_widget(&svc, json!({ "address": { "city": "Berlin" } })).await?;

    let err = node_ops::update_node(
        &svc,
        node_ops::UpdateNodeInput {
            node_id: id,
            version: None,
            node_type: None,
            content: None,
            properties: Some(json!({ "address": "not an object anymore" })),
            add_to_collection: None,
            remove_from_collection: None,
            lifecycle_status: None,
        },
    )
    .await
    .expect_err("an update writing a wrongly-typed value must be rejected too");

    assert!(err.to_string().contains("address"));
    Ok(())
}
