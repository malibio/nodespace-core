//! Integration tests for schema creation and updates
//!
//! Tests exercise handle_create_schema and handle_update_schema end-to-end
//! against a real NodeService / SqliteStore, covering title_template
//! validation including the field-removal-while-template-exists edge case.

use super::*;
use crate::db::SqliteStore;
use crate::services::NodeService;
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

async fn create_test_service() -> (Arc<NodeService>, TempDir) {
    let temp_dir = TempDir::new().expect("tempdir creation failed");
    let db_path = temp_dir.path().join("test.db");
    // NodeService::new takes &mut Arc<SqliteStore> to allow internal Arc replacement during init
    let mut store = Arc::new(
        SqliteStore::new(db_path)
            .await
            .expect("SqliteStore init failed"),
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
    // NB: not "Person" — that now collides with the core `person` schema (ADR-037).
    let schema_id = create_base_schema(&svc, "Employee", &["first_name", "last_name"]).await;

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
        position: crate::services::InsertPositionOwned::End,
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

// ============================================================================
// Description subtree tests
// ============================================================================

#[tokio::test]
async fn test_create_schema_stores_description_as_child_subtree() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Invoice",
            "description": "Tracks money owed by clients for goods or services rendered.",
            "fields": []
        }),
    )
    .await
    .expect("create_schema should succeed");

    let schema_id = result["schemaId"].as_str().expect("schemaId missing");
    assert_eq!(schema_id, "invoice");

    // Description must NOT be stored in properties
    let node = svc
        .get_node(schema_id)
        .await
        .expect("get_node failed")
        .expect("schema node not found");
    assert!(
        node.properties.get("description").is_none(),
        "description should not be in properties after #1351"
    );

    // Description must be stored as child text node(s)
    let children = svc
        .get_children(schema_id)
        .await
        .expect("get_children failed");
    assert!(
        !children.is_empty(),
        "Schema should have child description nodes"
    );

    // The child content should contain the description text
    let combined: String = children
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        combined.contains("money owed") || combined.contains("clients"),
        "Description text should appear in children: {:?}",
        combined
    );
}

#[tokio::test]
async fn test_create_schema_description_not_in_properties() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Campaign",
            "description": "Organizes outreach into phases and channels.",
            "fields": []
        }),
    )
    .await
    .expect("create_schema should succeed");

    let node = svc
        .get_node("campaign")
        .await
        .expect("get_node failed")
        .expect("schema node not found");

    // properties.description must be absent
    assert!(
        node.properties.get("description").is_none(),
        "description must not be written to properties (Issue #1351)"
    );
}

#[tokio::test]
async fn test_update_schema_replaces_description_subtree() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Contact",
            "description": "Original description.",
            "fields": []
        }),
    )
    .await
    .expect("create_schema should succeed");

    let initial_children = svc
        .get_children("contact")
        .await
        .expect("get_children failed");
    assert!(
        !initial_children.is_empty(),
        "Schema should have initial description children"
    );
    let initial_ids: std::collections::HashSet<String> =
        initial_children.iter().map(|c| c.id.clone()).collect();

    // Update description
    handle_update_schema(
        &svc,
        json!({
            "schema_id": "contact",
            "description": "Updated description with new content."
        }),
    )
    .await
    .expect("update_schema should succeed");

    let updated_children = svc
        .get_children("contact")
        .await
        .expect("get_children failed");
    assert!(
        !updated_children.is_empty(),
        "Schema should still have description children after update"
    );

    // None of the original child IDs should survive — replace_description_subtree must
    // delete the full old subtree before creating a new one.
    for child in &updated_children {
        assert!(
            !initial_ids.contains(&child.id),
            "Stale child node {:?} should have been deleted by replace_description_subtree",
            child.id
        );
    }

    // The combined text should reflect the new description
    let combined: String = updated_children
        .iter()
        .map(|c| c.content.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    assert!(
        combined.contains("Updated description") || combined.contains("new content"),
        "Updated description should appear in children: {:?}",
        combined
    );
}

#[tokio::test]
async fn test_schema_behavior_can_have_children_is_true() {
    use crate::behaviors::{NodeBehavior, SchemaNodeBehavior};

    let behavior = SchemaNodeBehavior;
    assert!(
        behavior.can_have_children(),
        "SchemaNodeBehavior should allow children (description subtree)"
    );
}

#[tokio::test]
async fn test_schema_get_aggregated_content_returns_description_text() {
    use crate::behaviors::{NodeBehavior, SchemaNodeBehavior};

    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Billing",
            "description": "Tracks invoices and payment records for clients.",
            "fields": []
        }),
    )
    .await
    .expect("create_schema should succeed");

    let schema_node = svc
        .get_node("billing")
        .await
        .expect("get_node failed")
        .expect("schema node not found");

    let behavior = SchemaNodeBehavior;
    let aggregated = behavior.get_aggregated_content(&schema_node, &*svc).await;

    assert!(
        aggregated.is_some(),
        "get_aggregated_content should return Some when description subtree exists"
    );
    let text = aggregated.unwrap();
    assert!(
        text.contains("invoices") || text.contains("payment"),
        "Aggregated content should include description text for semantic search: {:?}",
        text
    );
}

// ============================================================================
// Malformed `fields` entries — error must LOCATE the bad entry
// ============================================================================
//
// An LLM is the primary caller of create_schema, and it repairs a rejected call
// from the error text alone. Serde's whole-payload error names only the absent
// key ("missing field `type`") with no position, so the model cannot tell which
// array element to fix; observed behaviour is that it mutates an element that
// was already correct and degrades the arguments on every retry. These tests
// pin the properties that make the error repairable: which entry, what is
// missing, and an instruction not to disturb the rest.

#[tokio::test]
async fn test_create_schema_missing_field_type_names_the_offending_entry() {
    let (svc, _tmp) = create_test_service().await;

    // Second entry lacks "type" — the exact shape observed in the agent logs.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Project",
            "fields": [
                { "name": "project_name", "type": "text", "required": true },
                { "name": "status", "required": true }
            ]
        }),
    )
    .await;

    let err = result.expect_err("field missing 'type' must be rejected");
    let msg = err.to_string();

    assert!(
        msg.contains("fields[1]"),
        "error must identify WHICH entry is malformed, not just the absent key: {msg}"
    );
    assert!(
        msg.contains("status"),
        "error should name the offending entry so the caller can match it: {msg}"
    );
    assert!(
        msg.contains("\"type\""),
        "error must say which key is missing: {msg}"
    );
    assert!(
        !msg.contains("fields[0]"),
        "the well-formed entry must not be implicated — blaming it is what drives \
         the caller to corrupt a correct field on retry: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_malformed_fields_error_preserves_correct_entries() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Project",
            "fields": [
                { "name": "project_name", "type": "text" },
                { "type": "text" }
            ]
        }),
    )
    .await;

    let msg = result
        .expect_err("field missing 'name' must be rejected")
        .to_string();

    // Without this instruction the observed failure mode is a retry that drops
    // `name` from the entry that already had one.
    assert!(
        msg.contains("leave every other field exactly as it was"),
        "error must tell the caller to correct only the listed entries: {msg}"
    );
    assert!(
        msg.contains("fields[1]"),
        "error must locate the entry missing 'name': {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_reports_every_malformed_entry_at_once() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Project",
            "fields": [
                { "name": "a" },
                { "name": "b", "type": "text" },
                { "type": "number" }
            ]
        }),
    )
    .await;

    let msg = result
        .expect_err("malformed entries must be rejected")
        .to_string();

    // Reporting one problem per round-trip would take as many retries as there
    // are bad entries, and each retry is an opportunity to corrupt a good one.
    assert!(
        msg.contains("fields[0]") && msg.contains("fields[2]"),
        "every malformed entry must be reported in a single error: {msg}"
    );
    assert!(
        !msg.contains("fields[1]"),
        "the valid entry must not be reported: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_rejects_blank_field_name_not_just_absent_key() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Project",
            "fields": [{ "name": "   ", "type": "text" }]
        }),
    )
    .await;

    let msg = result
        .expect_err("a whitespace-only field name is not a usable name")
        .to_string();
    assert!(
        msg.contains("fields[0]") && msg.contains("\"name\""),
        "blank name must be reported like an absent one: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_reports_non_object_field_entry() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Project",
            "fields": ["status"]
        }),
    )
    .await;

    let msg = result
        .expect_err("a bare string is not a field definition")
        .to_string();
    assert!(
        msg.contains("fields[0]") && msg.contains("not an object"),
        "error must explain that the entry is the wrong shape entirely: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_snake_case_core_values_rejected() {
    let (svc, _tmp) = create_test_service().await;

    // The tool schema declares "coreValues" (camelCase). A caller that sends
    // the snake_case Rust field name instead must be rejected with an error
    // naming the unknown key, not silently produce an enum with no values.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Invoice",
            "fields": [
                {
                    "name": "status",
                    "type": "enum",
                    "core_values": [
                        { "value": "pending", "label": "Pending" },
                        { "value": "paid", "label": "Paid" }
                    ]
                }
            ]
        }),
    )
    .await;

    let msg = result
        .expect_err("core_values is not a recognized key on a schema field")
        .to_string();
    assert!(
        msg.contains("core_values"),
        "error must name the unknown field so the caller can correct it: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_enum_field_with_core_values_succeeds() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Invoice",
            "fields": [
                {
                    "name": "status",
                    "type": "enum",
                    "coreValues": [
                        { "value": "pending", "label": "Pending" },
                        { "value": "paid", "label": "Paid" }
                    ]
                }
            ]
        }),
    )
    .await;

    let val = result.expect("enum field with coreValues should succeed end to end");
    assert_eq!(val["schemaId"], "invoice");
    let fields = val["fields"].as_array().expect("fields array");
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0]["name"], "status");
    let core_values = fields[0]["coreValues"]
        .as_array()
        .expect("coreValues array");
    assert_eq!(core_values.len(), 2);
    assert_eq!(core_values[0]["value"], "pending");
}

#[tokio::test]
async fn test_create_schema_with_well_formed_fields_is_unaffected() {
    let (svc, _tmp) = create_test_service().await;

    // The locating pass must only ever turn an error into a better error — it
    // must not reject a payload that previously succeeded.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Equipment",
            "fields": [
                { "name": "replacement_cost", "type": "number" },
                { "name": "checked_out_on", "type": "date" }
            ]
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "well-formed fields must still create the schema: {:?}",
        result
    );
}

#[tokio::test]
async fn test_create_schema_without_fields_key_is_unaffected() {
    let (svc, _tmp) = create_test_service().await;

    // Description-only creation infers its own fields; the locating pass must
    // not intercept a payload that has no `fields` array at all.
    //
    // This originally asserted only that the locating pass stayed out of the way,
    // guarded behind `if let Err`, because description-only creation was broken
    // at the time: namespaced field names contained ':' and the field-name
    // validator rejected them. That is now fixed, so the guard would make this
    // test vacuous — it would pass without asserting anything at all.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Venue",
            "description": "contact email and capacity number"
        }),
    )
    .await;

    let output = result.expect("description-only create_schema must succeed");
    assert_eq!(output["schemaId"], "venue");
}

#[tokio::test]
async fn test_update_schema_locates_malformed_add_fields_entry() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Album",
            "fields": [{ "name": "artist", "type": "text" }]
        }),
    )
    .await
    .expect("setup create_schema failed");

    let result = handle_update_schema(
        &svc,
        json!({
            "schema_id": "album",
            "add_fields": [{ "name": "released" }]
        }),
    )
    .await;

    let msg = result
        .expect_err("add_fields entry missing 'type' must be rejected")
        .to_string();
    assert!(
        msg.contains("add_fields[0]"),
        "update_schema must locate the bad entry using its own key name: {msg}"
    );
    assert!(
        msg.contains("released"),
        "error should name the offending entry: {msg}"
    );
}

// ============================================================================
// title_template resolution is owned by validate_template_tokens
// ============================================================================
//
// There is deliberately no second, pre-deserialization check for this rule.
// A hand-rolled scanner disagreed with the authority on real inputs — it waved
// through "{a} {b" (unclosed) and prescribed a fix for "{} {b}" that still
// failed on the empty placeholder — handing the caller a confident but wrong
// repair instruction, which is worse than a terse correct one.

#[tokio::test]
async fn test_create_schema_title_template_all_placeholders_defined_succeeds() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Equipment",
            "fields": [
                { "name": "equipment", "type": "text" },
                { "name": "status", "type": "text" }
            ],
            "title_template": "{equipment} ({status})"
        }),
    )
    .await;

    assert!(
        result.is_ok(),
        "a template whose placeholders all resolve must create: {:?}",
        result
    );
}

#[tokio::test]
async fn test_create_schema_title_template_unclosed_placeholder_rejected() {
    let (svc, _tmp) = create_test_service().await;

    // The case a hand-rolled pre-check silently passed: scanning for '{' and
    // taking the next '}' finds none, skips the token, and reports nothing.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Equipment",
            "fields": [{ "name": "equipment", "type": "text" }],
            "title_template": "{equipment} {status"
        }),
    )
    .await;

    let msg = result
        .expect_err("an unclosed placeholder must be rejected")
        .to_string();
    // Assert the AUTHORITY's exact phrasing, not just the word "unclosed".
    //
    // On this input a pre-check and the authority agree on the OUTCOME — both
    // reject — so an outcome assertion cannot tell which one spoke, and cannot
    // detect a second validator being reintroduced. Pinning the exact wording
    // at least catches a reintroduction that rewords the error, which is the
    // most this input can guarantee. The empty-placeholder test below is the
    // one that genuinely fails on a reintroduced pre-check.
    assert!(
        msg.contains("contains an unclosed '{' placeholder"),
        "the authority's own message must be what reaches the caller: {msg}"
    );
}

#[tokio::test]
async fn test_create_schema_title_template_empty_placeholder_rejected() {
    let (svc, _tmp) = create_test_service().await;

    // This is the ratchet: it fails if a second, hand-rolled title_template
    // validator is ever reintroduced ahead of the authority.
    //
    // `fields` MUST stay empty. A pre-check that scans for placeholders and
    // prescribes the missing ones would report `status` here and return its own
    // message, which does not contain the authority's "empty" wording — so this
    // assertion fails and the reintroduction is caught. Define `status` in
    // `fields` and the placeholder resolves, the pre-check falls silent, the
    // authority runs anyway, and the test passes either way — blind to the
    // very thing it exists to catch.
    let result = handle_create_schema(
        &svc,
        json!({
            "name": "Equipment",
            "fields": [],
            "title_template": "{} {status}"
        }),
    )
    .await;

    let msg = result
        .expect_err("an empty placeholder must be rejected")
        .to_string();
    assert!(
        msg.contains("contains an empty '{}' placeholder"),
        "the authority must reject on the empty placeholder, and its own message \
         must be what reaches the caller — a different message here means a \
         second validator spoke first: {msg}"
    );
}

#[tokio::test]
async fn test_update_schema_title_template_may_reference_preexisting_fields() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Gear",
            "fields": [{ "name": "label", "type": "text" }]
        }),
    )
    .await
    .expect("setup create_schema failed");

    // update_schema deliberately does NOT get the title_template pre-check: the
    // template may reference fields already on the stored schema, which the
    // payload alone cannot see. Pre-checking here would reject a valid call.
    let result = handle_update_schema(
        &svc,
        json!({ "schema_id": "gear", "title_template": "{label}" }),
    )
    .await;

    assert!(
        result.is_ok(),
        "a template referencing an existing stored field must be accepted: {:?}",
        result
    );
}

// ============================================================================
// create_schema from a description (no explicit fields)
// ============================================================================

#[tokio::test]
async fn test_create_schema_from_description_only() {
    let (svc, _tmp) = create_test_service().await;

    // No `fields` array: fields are inferred from the description and stored
    // under bare names, matching what the explicit-fields path stores.
    let result = handle_create_schema(
        &svc,
        json!({ "name": "Venue", "description": "contact email and capacity number" }),
    )
    .await;

    assert!(
        result.is_ok(),
        "Description-only create_schema should succeed: {:?}",
        result
    );

    let val = result.expect("description-only create_schema should return Ok");
    assert_eq!(val["schemaId"], "venue");

    let field_names: Vec<&str> = val["fields"]
        .as_array()
        .expect("fields should be an array")
        .iter()
        .map(|f| f["name"].as_str().expect("field name should be a string"))
        .collect();

    assert!(
        !field_names.is_empty(),
        "Description should infer at least one field"
    );
    assert!(
        field_names.iter().all(|n| !n.contains(':')),
        "Inferred fields should be stored under bare names: {:?}",
        field_names
    );
    assert!(
        field_names.contains(&"contact_email"),
        "Expected 'contact_email' among {:?}",
        field_names
    );

    // The schema must be readable back from storage, not merely returned.
    let stored = svc
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed");
    assert!(
        stored.is_some(),
        "Schema should be persisted and retrievable"
    );
}

/// The two `create_schema` routes must apply the same *namespacing* convention
/// to stored field names. They previously diverged — the description path
/// applied a `custom:` prefix during inference while the explicit-fields path
/// stored caller-supplied names verbatim — so a schema's stored keys depended on
/// which call shape created it. Stored names are user-visible (title-template
/// tokens, CEL selectors, query filters, frontend lookups), so this asserts on
/// the stored node rather than the return value.
///
/// The description here is worded so field-name *inference* is unambiguous, keeping this test
/// focused on the namespace convention. The companion test below covers the harder case, where
/// the description carries a type keyword the inference has to read as a type rather than a name.
#[tokio::test]
async fn test_create_schema_paths_agree_on_stored_field_names() {
    let field_names_of = |schema: &crate::models::SchemaNode| -> Vec<String> {
        let mut names: Vec<String> = schema.fields.iter().map(|f| f.name.clone()).collect();
        names.sort();
        names
    };

    // Path A: fields inferred from a natural-language description.
    let (svc_described, _tmp_a) = create_test_service().await;
    handle_create_schema(
        &svc_described,
        json!({ "name": "Venue", "description": "capacity" }),
    )
    .await
    .expect("description-path create_schema should succeed");
    let described = svc_described
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed")
        .expect("description-path schema should be persisted");

    // Path B: the same intent expressed as an explicit field definition.
    let (svc_explicit, _tmp_b) = create_test_service().await;
    handle_create_schema(
        &svc_explicit,
        json!({
            "name": "Venue",
            "fields": [{ "name": "capacity", "type": "string" }]
        }),
    )
    .await
    .expect("explicit-path create_schema should succeed");
    let explicit = svc_explicit
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed")
        .expect("explicit-path schema should be persisted");

    assert_eq!(
        field_names_of(&described),
        field_names_of(&explicit),
        "Both create_schema paths must store identical field names"
    );
    assert_eq!(
        field_names_of(&explicit),
        vec!["capacity".to_string()],
        "Stored field names are bare, not namespace-prefixed"
    );
}

/// The two routes must also agree when the description carries a type keyword. "capacity number"
/// once stored `capacity_number` while the explicit-fields route stored `capacity` — the keyword
/// was consumed for type inference *and* kept in the name, so a schema's user-visible keys
/// depended on which call shape created it. Asserts on the stored node, not the return value.
#[tokio::test]
async fn test_create_schema_paths_agree_on_stored_field_names_with_type_keyword() {
    let field_names_of = |schema: &crate::models::SchemaNode| -> Vec<String> {
        let mut names: Vec<String> = schema.fields.iter().map(|f| f.name.clone()).collect();
        names.sort();
        names
    };

    // Path A: the type keyword sits in a natural-language description.
    let (svc_described, _tmp_a) = create_test_service().await;
    handle_create_schema(
        &svc_described,
        json!({ "name": "Venue", "description": "capacity number" }),
    )
    .await
    .expect("description-path create_schema should succeed");
    let described = svc_described
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed")
        .expect("description-path schema should be persisted");

    // Path B: the same intent, with the type given explicitly instead of in prose.
    let (svc_explicit, _tmp_b) = create_test_service().await;
    handle_create_schema(
        &svc_explicit,
        json!({
            "name": "Venue",
            "fields": [{ "name": "capacity", "type": "number" }]
        }),
    )
    .await
    .expect("explicit-path create_schema should succeed");
    let explicit = svc_explicit
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed")
        .expect("explicit-path schema should be persisted");

    assert_eq!(
        field_names_of(&described),
        field_names_of(&explicit),
        "Both create_schema paths must store identical field names when the description \
         carries a type keyword"
    );
    assert_eq!(
        field_names_of(&explicit),
        vec!["capacity".to_string()],
        "The type keyword is read as the type, not folded into the stored name"
    );

    // The keyword was read as the type, not merely discarded.
    let capacity = described
        .fields
        .iter()
        .find(|f| f.name == "capacity")
        .expect("described schema should define 'capacity'");
    assert_eq!(
        capacity.field_type, "number",
        "'capacity number' should still infer a number type"
    );
}

/// Names where the type keyword is intrinsic must survive the description route unchanged —
/// stripping it would turn `invoice_number` into `invoice`, a different wrong stored key.
#[tokio::test]
async fn test_create_schema_description_keeps_intrinsic_type_keyword() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({ "name": "Bill", "description": "invoice number, capacity number" }),
    )
    .await
    .expect("description-path create_schema should succeed");

    let stored = svc
        .get_schema_node("bill")
        .await
        .expect("get_schema_node should succeed")
        .expect("schema should be persisted");

    let mut names: Vec<String> = stored.fields.iter().map(|f| f.name.clone()).collect();
    names.sort();
    assert_eq!(
        names,
        vec!["capacity".to_string(), "invoice_number".to_string()],
        "'invoice number' keeps its keyword while 'capacity number' drops it"
    );
}

/// The reserved-core-property warning has to survive all the way into the
/// response JSON. It previously did not: warnings were snapshotted before the
/// normalization step that appends them, so every collision was dropped
/// silently. A unit test on the normalization function alone would not have
/// caught that, because the defect was in the caller.
#[tokio::test]
async fn test_create_schema_reserved_property_warning_reaches_response() {
    let (svc, _tmp) = create_test_service().await;

    let result = handle_create_schema(&svc, json!({ "name": "Ticket", "description": "status" }))
        .await
        .expect("create_schema should succeed");

    let warnings = result["warnings"]
        .as_array()
        .expect("warnings should be present in the response when a field collides");
    assert!(
        warnings.iter().any(|w| {
            let w = w.as_str().unwrap_or_default();
            w.contains("status") && w.contains("reserved core property")
        }),
        "Expected a reserved-core-property warning for 'status', got {warnings:?}"
    );
}

/// Bare names are the convention for user-defined types, but a type NodeSpace
/// owns is different: a bare name added to a core schema can be claimed by a
/// core property in a future release, and the user's field would collide with
/// it. The prefix requirement is kept exactly where that hazard exists.
#[tokio::test]
async fn test_update_schema_core_type_requires_namespace_prefix() {
    let (svc, _tmp) = create_test_service().await;

    let unprefixed = handle_update_schema(
        &svc,
        json!({
            "schema_id": "task",
            "add_fields": [{ "name": "effort", "type": "number" }]
        }),
    )
    .await;

    let err = unprefixed
        .expect_err("adding an unprefixed field to a core schema should be rejected")
        .to_string();
    assert!(
        err.contains("effort") && err.contains("namespace prefix"),
        "Error should name the field and the requirement, got: {err}"
    );

    // The same field is accepted once it carries a prefix.
    handle_update_schema(
        &svc,
        json!({
            "schema_id": "task",
            "add_fields": [{ "name": "custom:effort", "type": "number" }]
        }),
    )
    .await
    .expect("adding a namespaced field to a core schema should succeed");

    let stored = svc
        .get_schema_node("task")
        .await
        .expect("get_schema_node should succeed")
        .expect("core task schema should exist");
    assert!(
        stored.fields.iter().any(|f| f.name == "custom:effort"),
        "Namespaced field should be persisted on the core schema"
    );
}

/// Renaming is the other way a bare name can land on a core type, and it is the
/// more damaging one: `rename_schema_field` migrates node property data and
/// rewrites the schema per rename as it goes, so a check that ran afterwards
/// would reject the call with the offending key already written across every
/// node instance. The requirement is therefore enforced before any rename runs.
#[tokio::test]
async fn test_update_schema_core_type_rename_cannot_drop_namespace_prefix() {
    let (svc, _tmp) = create_test_service().await;

    handle_update_schema(
        &svc,
        json!({
            "schema_id": "task",
            "add_fields": [{ "name": "custom:effort", "type": "number" }]
        }),
    )
    .await
    .expect("adding a namespaced field to a core schema should succeed");

    let renamed = handle_update_schema(
        &svc,
        json!({
            "schema_id": "task",
            "rename_fields": [{ "from": "custom:effort", "to": "effort" }]
        }),
    )
    .await;

    let err = renamed
        .expect_err("renaming a core-schema field to a bare name should be rejected")
        .to_string();
    assert!(
        err.contains("effort") && err.contains("namespace prefix"),
        "Error should name the field and the requirement, got: {err}"
    );

    // The rejected rename must not have been applied on the way out.
    let stored = svc
        .get_schema_node("task")
        .await
        .expect("get_schema_node should succeed")
        .expect("core task schema should exist");
    assert!(
        stored.fields.iter().any(|f| f.name == "custom:effort"),
        "Original namespaced field should survive a rejected rename: {:?}",
        stored.fields.iter().map(|f| &f.name).collect::<Vec<_>>()
    );
    assert!(
        !stored.fields.iter().any(|f| f.name == "effort"),
        "Bare name must not reach a core schema via rename: {:?}",
        stored.fields.iter().map(|f| &f.name).collect::<Vec<_>>()
    );

    // Renaming between two namespaced names stays allowed.
    handle_update_schema(
        &svc,
        json!({
            "schema_id": "task",
            "rename_fields": [{ "from": "custom:effort", "to": "custom:effort_points" }]
        }),
    )
    .await
    .expect("renaming to another namespaced name should succeed");
}

/// A user-defined type is unaffected by the core-type rule: bare names are the
/// convention there, so neither adding nor renaming to one is rejected.
#[tokio::test]
async fn test_update_schema_user_defined_type_allows_bare_names() {
    let (svc, _tmp) = create_test_service().await;

    handle_create_schema(
        &svc,
        json!({
            "name": "Venue",
            "fields": [{ "name": "capacity", "type": "number" }]
        }),
    )
    .await
    .expect("create_schema should succeed");

    handle_update_schema(
        &svc,
        json!({
            "schema_id": "venue",
            "add_fields": [{ "name": "address", "type": "string" }],
            "rename_fields": [{ "from": "capacity", "to": "seats" }]
        }),
    )
    .await
    .expect("bare names on a user-defined type should be accepted");

    let stored = svc
        .get_schema_node("venue")
        .await
        .expect("get_schema_node should succeed")
        .expect("venue schema should exist");
    let names: Vec<&str> = stored.fields.iter().map(|f| f.name.as_str()).collect();
    assert!(
        names.contains(&"seats") && names.contains(&"address"),
        "Expected bare 'seats' and 'address', got {names:?}"
    );
}
