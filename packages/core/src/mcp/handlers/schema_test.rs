//! Integration tests for MCP schema handlers
//!
//! Tests exercise handle_create_schema and handle_update_schema end-to-end
//! against a real NodeService / SurrealStore, covering title_template
//! validation including the field-removal-while-template-exists edge case.

use super::*;
use crate::db::SurrealStore;
use crate::services::NodeService;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_service() -> (Arc<NodeService>, TempDir) {
    let temp_dir = TempDir::new().expect("tempdir creation failed");
    let db_path = temp_dir.path().join("test.db");
    // NodeService::new takes &mut Arc<SurrealStore> to allow internal Arc replacement during init
    let mut store = Arc::new(
        SurrealStore::new(db_path)
            .await
            .expect("SurrealStore init failed"),
    );
    let node_service = Arc::new(
        NodeService::new(&mut store)
            .await
            .expect("NodeService init failed"),
    );
    (node_service, temp_dir)
}

// ============================================================================
// create_schema + title_template
// ============================================================================

#[tokio::test]
async fn test_create_schema_with_valid_title_template() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Customer",
            "fields": [
                { "name": "first_name", "type": "string", "protection": "user", "indexed": false },
                { "name": "last_name",  "type": "string", "protection": "user", "indexed": false }
            ],
            "title_template": "{first_name} {last_name}"
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Valid title_template should succeed: {:?}",
        result
    );
    let val = result.expect("valid create_schema should return Ok");
    assert_eq!(val["schemaId"], "customer");
}

#[tokio::test]
async fn test_create_schema_title_template_undefined_field_rejected() {
    let (svc, _tmp) = create_test_service().await;

    // title_template references "nonexistent" which is not in fields
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Customer",
            "fields": [
                { "name": "first_name", "type": "string", "protection": "user", "indexed": false }
            ],
            "title_template": "{nonexistent}"
        }),
    )
    .await;

    assert!(
        result.is_err(),
        "title_template referencing undefined field should fail"
    );
    let err = result.unwrap_err();
    let msg = format!("{:?}", err);
    assert!(
        msg.contains("nonexistent"),
        "Error should name the bad field: {}",
        msg
    );
}

#[tokio::test]
async fn test_create_schema_without_title_template_succeeds() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Invoice",
            "fields": [
                { "name": "amount", "type": "number", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Schema without title_template should succeed: {:?}",
        result
    );
}

// ============================================================================
// update_schema + title_template
// ============================================================================

/// Helper: create a schema with the given fields (no title_template)
async fn create_base_schema(svc: &Arc<NodeService>, name: &str, field_names: &[&str]) -> String {
    let fields: Vec<_> = field_names
        .iter()
        .map(|n| json!({ "name": n, "type": "string", "protection": "user", "indexed": false }))
        .collect();

    let result = handle_create_schema(svc, json!({ "name": name, "fields": fields }))
        .await
        .expect("create_base_schema: schema creation failed");

    result["schemaId"]
        .as_str()
        .expect("create_base_schema: schemaId missing in response")
        .to_string()
}

#[tokio::test]
async fn test_update_schema_add_valid_title_template() {
    let (svc, _tmp) = create_test_service().await;
    let schema_id = create_base_schema(&svc, "Person", &["first_name", "last_name"]).await;

    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "title_template": "{first_name} {last_name}"
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Adding valid title_template should succeed: {:?}",
        result
    );
}

#[tokio::test]
async fn test_update_schema_title_template_undefined_field_rejected() {
    let (svc, _tmp) = create_test_service().await;
    let schema_id = create_base_schema(&svc, "Contact", &["email"]).await;

    // Template references "name" which doesn't exist in this schema
    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "title_template": "{name}"
        }),
    )
    .await;

    assert!(
        result.is_err(),
        "title_template referencing undefined field should fail"
    );
    let msg = format!("{:?}", result.unwrap_err());
    assert!(
        msg.contains("name"),
        "Error should name the bad field: {}",
        msg
    );
}

#[tokio::test]
async fn test_update_schema_remove_field_referenced_by_existing_template_rejected() {
    let (svc, _tmp) = create_test_service().await;

    // Create schema with both fields and a title_template
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Employee",
            "fields": [
                { "name": "first_name", "type": "string", "protection": "user", "indexed": false },
                { "name": "last_name",  "type": "string", "protection": "user", "indexed": false }
            ],
            "title_template": "{first_name} {last_name}"
        }),
    )
    .await
    .expect("Employee schema creation failed");
    let schema_id = result["schemaId"].as_str().expect("schemaId missing");

    // Now try to remove first_name — template still references it
    let update_result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "remove_fields": ["first_name"]
        }),
    )
    .await;

    assert!(
        update_result.is_err(),
        "Removing a field still referenced by title_template should be rejected"
    );
    let msg = format!("{:?}", update_result.unwrap_err());
    assert!(
        msg.contains("first_name"),
        "Error should identify the dangling field: {}",
        msg
    );
}

#[tokio::test]
async fn test_update_schema_remove_field_and_clear_template_succeeds() {
    let (svc, _tmp) = create_test_service().await;

    // Create schema with title_template
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Widget",
            "fields": [
                { "name": "sku",   "type": "string", "protection": "user", "indexed": false },
                { "name": "color", "type": "string", "protection": "user", "indexed": false }
            ],
            "title_template": "{sku}"
        }),
    )
    .await
    .expect("Widget schema creation failed");
    let schema_id = result["schemaId"].as_str().expect("schemaId missing");

    // Clearing the template (empty string) while removing sku should succeed:
    // the empty template has no tokens so validation passes.
    // Note: we pass an empty string because Option<String> with serde default
    // can't distinguish "omit" from "clear" — this tests the case where
    // the caller explicitly sets an empty template to clear it.
    let update_result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "remove_fields": ["sku"],
            "title_template": ""
        }),
    )
    .await;

    assert!(
        update_result.is_ok(),
        "Removing field after clearing template should succeed: {:?}",
        update_result
    );
}

#[tokio::test]
async fn test_update_schema_remove_unrelated_field_with_template_succeeds() {
    let (svc, _tmp) = create_test_service().await;

    // Create schema with three fields; template only uses two
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Product",
            "fields": [
                { "name": "name",  "type": "string", "protection": "user", "indexed": false },
                { "name": "sku",   "type": "string", "protection": "user", "indexed": false },
                { "name": "notes", "type": "string", "protection": "user", "indexed": false }
            ],
            "title_template": "{name} ({sku})"
        }),
    )
    .await
    .expect("Product schema creation failed");
    let schema_id = result["schemaId"].as_str().expect("schemaId missing");

    // Remove "notes" — not in the template — should succeed
    let update_result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "remove_fields": ["notes"]
        }),
    )
    .await;

    assert!(
        update_result.is_ok(),
        "Removing a field not referenced by title_template should succeed: {:?}",
        update_result
    );
}

// ============================================================================
// rename_fields
// ============================================================================

#[tokio::test]
async fn test_rename_field_updates_schema_definition() {
    let (svc, _tmp) = create_test_service().await;

    // Create a schema with a field to rename
    let create_result = handle_create_schema(
        &svc,
        json!({
            "name": "RenameTest",
            "fields": [
                { "name": "old_name", "type": "string", "protection": "user", "indexed": false },
                { "name": "keep_me",  "type": "string", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await
    .expect("Schema creation failed");
    let schema_id = create_result["schemaId"]
        .as_str()
        .expect("schemaId missing");

    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "rename_fields": [{ "from": "old_name", "to": "new_name" }]
        }),
    )
    .await;

    assert!(result.is_ok(), "rename_fields should succeed: {:?}", result);
    let output = result.unwrap();
    assert_eq!(output["fieldsRenamed"], serde_json::json!(1));

    // Schema definition should reflect the rename
    let schema = svc
        .get_schema_node(schema_id)
        .await
        .expect("get_schema_node failed")
        .expect("schema not found");

    let field_names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
    assert!(
        field_names.contains(&"new_name"),
        "new_name should exist in schema fields: {:?}",
        field_names
    );
    assert!(
        !field_names.contains(&"old_name"),
        "old_name should no longer exist in schema fields: {:?}",
        field_names
    );
    assert!(
        field_names.contains(&"keep_me"),
        "keep_me should be unchanged: {:?}",
        field_names
    );
}

#[tokio::test]
async fn test_rename_field_not_found_returns_error() {
    let (svc, _tmp) = create_test_service().await;

    let create_result = handle_create_schema(
        &svc,
        json!({
            "name": "RenameErrorTest",
            "fields": [
                { "name": "existing", "type": "string", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await
    .expect("Schema creation failed");
    let schema_id = create_result["schemaId"]
        .as_str()
        .expect("schemaId missing");

    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "rename_fields": [{ "from": "does_not_exist", "to": "new_name" }]
        }),
    )
    .await;

    assert!(result.is_err(), "Renaming a nonexistent field should fail");
}

#[tokio::test]
async fn test_rename_field_to_existing_field_returns_error() {
    let (svc, _tmp) = create_test_service().await;

    let create_result = handle_create_schema(
        &svc,
        json!({
            "name": "RenameConflictTest",
            "fields": [
                { "name": "field_a", "type": "string", "protection": "user", "indexed": false },
                { "name": "field_b", "type": "string", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await
    .expect("Schema creation failed");
    let schema_id = create_result["schemaId"]
        .as_str()
        .expect("schemaId missing");

    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "rename_fields": [{ "from": "field_a", "to": "field_b" }]
        }),
    )
    .await;

    assert!(
        result.is_err(),
        "Renaming to an existing field name should fail"
    );
}

#[tokio::test]
async fn test_rename_field_migrates_node_data() {
    use crate::services::CreateNodeParams;

    let (svc, _tmp) = create_test_service().await;

    // Create a schema type
    let create_result = handle_create_schema(
        &svc,
        json!({
            "name": "DataMigrateTest",
            "fields": [
                { "name": "old_field", "type": "string", "protection": "user", "indexed": false }
            ]
        }),
    )
    .await
    .expect("Schema creation failed");
    let schema_id = create_result["schemaId"]
        .as_str()
        .expect("schemaId missing")
        .to_string();

    // Create a node instance with data in old_field
    let node_params = CreateNodeParams {
        id: None,
        node_type: schema_id.clone(),
        content: "test node".to_string(),
        parent_id: None,
        insert_after_node_id: None,
        properties: serde_json::json!({
            &schema_id: { "old_field": "my_value" }
        }),
    };
    let node_id = svc
        .create_node_with_parent(node_params)
        .await
        .expect("create_node_with_parent failed");

    // Rename the field
    handle_update_schema(
        &svc,
        json!({
            "schema_id": schema_id,
            "rename_fields": [{ "from": "old_field", "to": "new_field" }]
        }),
    )
    .await
    .expect("rename_fields should succeed");

    // Verify the node data was migrated
    let node = svc
        .get_node(&node_id)
        .await
        .expect("get_node failed")
        .expect("node not found");

    let ns_props = node.properties.get(&schema_id);
    assert!(
        ns_props.is_some(),
        "Namespaced properties should exist after rename"
    );
    let ns_props = ns_props.unwrap();
    assert_eq!(
        ns_props.get("new_field").and_then(|v| v.as_str()),
        Some("my_value"),
        "Value should be migrated to new_field"
    );
    assert!(
        ns_props.get("old_field").is_none(),
        "old_field should be removed after rename"
    );
}
