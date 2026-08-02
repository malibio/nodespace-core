//! Schema creation and updates.
//!
//! Provides the `create_schema` tool for creating custom schemas with explicit
//! field and relationship definitions.

use crate::behaviors::SchemaNodeBehavior;
use crate::markdown::MarkdownError;
use crate::models::schema::SchemaField;
use crate::models::{Node, NodeUpdate, SchemaNode};
use crate::services::{CreateNodeParams, NodeService};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// Reserved core property names that conflict with system properties
const RESERVED_CORE_PROPERTIES: &[&str] = &[
    "id",
    "node_type",
    "content",
    "parent_id",
    "root_id",
    "created_at",
    "modified_at",
    "status",
    "priority",
    "due_date",
    "due",
];

/// Report malformed entries in a `create_schema`/`update_schema` `fields` array
/// with enough context for the caller to repair the exact element at fault.
///
/// Serde's own error for this payload names only the absent key — for a field
/// array it cannot say which element lacked it. That is actionable for a human
/// reading a stack trace and useless to an LLM constructing the call: with no
/// position, the model edits an element that was already correct and each retry
/// loses more information than the last.
///
/// Returns `Ok(())` when `fields` is absent or every entry has both `name` and
/// `type` — the payload then proceeds to normal deserialization, so this
/// function only ever converts an error into a better-located error.
fn describe_malformed_fields(params: &Value, key: &str) -> Result<(), MarkdownError> {
    let Some(fields) = params.get(key).and_then(Value::as_array) else {
        return Ok(());
    };

    let mut problems: Vec<String> = Vec::new();
    for (idx, field) in fields.iter().enumerate() {
        let Some(obj) = field.as_object() else {
            problems.push(format!(
                "{key}[{idx}] is {}, not an object",
                json_type_name(field)
            ));
            continue;
        };
        // Identify the element by name where one exists — a name is far easier
        // for the caller to match against what it sent than a bare index.
        let label = obj
            .get("name")
            .and_then(Value::as_str)
            .filter(|n| !n.trim().is_empty())
            .map(|n| format!("{key}[{idx}] (\"{n}\")"))
            .unwrap_or_else(|| format!("{key}[{idx}]"));

        // Not `key` — that names the array being checked, and shadowing it here
        // with a per-entry property name gives one identifier two meanings in a
        // function whose whole job is naming things precisely.
        for required_key in ["name", "type"] {
            let missing = match obj.get(required_key) {
                None => true,
                Some(Value::String(s)) => s.trim().is_empty(),
                Some(_) => false,
            };
            if missing {
                problems.push(format!("{label} is missing \"{required_key}\""));
            }
        }
    }

    if problems.is_empty() {
        return Ok(());
    }

    Err(MarkdownError::invalid_params(format!(
        // No "Invalid parameters:" prefix — Display for InvalidParams already
        // writes "invalid params: ", and the model reads the doubled preamble first.
        "{}. Every entry in \"{key}\" needs both \"name\" and \"type\", \
         e.g. {{\"name\":\"amount\",\"type\":\"number\"}}. Re-send the call with only the \
         listed entries corrected — leave every other field exactly as it was.",
        problems.join("; ")
    )))
}

/// Whether a field name carries a `<namespace>:` prefix (`custom:capacity`).
///
/// Requires exactly one `:` with a non-empty segment on each side, so neither a
/// bare name (`capacity`) nor a malformed one (`:capacity`, `custom:`,
/// `a:b:c`) counts as prefixed. `validate_schema_field_name` rejects the
/// malformed forms outright, but this does not lean on that: a check that
/// silently depends on validation order in another module is one refactor away
/// from admitting the names it exists to exclude.
fn has_namespace_prefix(name: &str) -> bool {
    let mut parts = name.split(':');
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some(namespace), Some(bare), None) if !namespace.is_empty() && !bare.is_empty()
    )
}

/// Warn on explicit field names that shadow a reserved core property.
///
/// A name colliding with a reserved core property is reported as a warning
/// rather than rejected, leaving the choice with the caller — this mirrors how
/// the description-inference route treated the same collision before it was
/// deleted (ADR-063). `fields` is otherwise stored verbatim.
fn warn_reserved_property_names(fields: Vec<SchemaField>) -> (Vec<SchemaField>, Vec<String>) {
    let mut warnings = Vec::new();
    for field in &fields {
        if RESERVED_CORE_PROPERTIES.contains(&field.name.as_str()) {
            warnings.push(format!(
                "Field name '{}' matches a reserved core property and may be shadowed by it. \
                 Consider a more specific name.",
                field.name
            ));
        }
    }
    (fields, warnings)
}

/// Whether `schema_id` names a schema NodeSpace ships, rather than a
/// user-defined one. A missing schema is reported as non-core; the update path
/// below surfaces the not-found error with better context.
async fn schema_is_core(
    node_service: &Arc<NodeService>,
    schema_id: &str,
) -> Result<bool, MarkdownError> {
    let schema = node_service
        .get_schema_node(schema_id)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?;
    Ok(schema.is_some_and(|s| s.is_core))
}

/// Reject relationships whose `targetType` does not name an existing schema.
///
/// `TARGET_TYPE_MUST_EXIST` (`skill_rules.rs`) already tells the model this in
/// prose, but nothing previously enforced it: `handle_create_schema`,
/// `handle_update_schema`, and `handle_add_schema_relationship` all persisted
/// a relationship's `targetType` verbatim with no existence check, so a model
/// that ignored the prose rule (or split one request into two schemas and
/// referenced the second before it existed) got a silent success and a
/// dangling reference instead of an actionable error. `targetType: None` is
/// left unvalidated — omitting the target entirely is the documented escape
/// hatch for "the type doesn't exist yet."
async fn validate_relationship_targets_exist(
    node_service: &Arc<NodeService>,
    relationships: &[crate::models::schema::SchemaRelationship],
) -> Result<(), MarkdownError> {
    for rel in relationships {
        let Some(target_type) = rel.target_type.as_deref() else {
            continue;
        };
        let exists = node_service
            .get_schema_node(target_type)
            .await
            .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?
            .is_some();
        if !exists {
            return Err(MarkdownError::invalid_params(format!(
                "Relationship '{}' targets '{}', which is not an existing schema. \
                 targetType must name a schema that already exists — omit the relationship \
                 entirely if the target type doesn't exist yet, rather than inventing one.",
                rel.name, target_type
            )));
        }
    }
    Ok(())
}

/// Name of a JSON value's type, for error messages.
fn json_type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "a string",
        Value::Array(_) => "an array",
        Value::Object(_) => "an object",
    }
}

/// Input parameters for create_schema
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateSchemaParams {
    /// Schema name (e.g., "Invoice", "Customer")
    pub name: String,
    /// Brief prose summary of what this entity type represents. Stored as a
    /// child subtree for semantic discovery; not parsed into fields.
    #[serde(default)]
    pub description: Option<String>,
    /// Explicit field definitions
    #[serde(default)]
    pub fields: Option<Vec<SchemaField>>,
    /// Optional relationship definitions
    #[serde(default)]
    pub relationships: Option<Vec<crate::models::schema::SchemaRelationship>>,
    /// Optional template for computing display title from field values.
    /// Use `{field_name}` tokens that reference fields defined in `fields`.
    /// Example: `"{first_name} {last_name}"` for a customer schema.
    #[serde(default)]
    pub title_template: Option<String>,
    /// Optional template for rendering a compact property summary inline below the node title.
    /// Uses the same `{field_name}` syntax. Evaluated client-side only.
    /// Example: `"{status} · {company}"` → `"Active · Acme Corp"`.
    #[serde(default)]
    pub properties_header_summary_template: Option<String>,
}

/// Output from schema creation
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateSchemaOutput {
    /// ID for the generated schema (snake_case of name)
    pub schema_id: String,
    /// Whether this is a core schema
    pub is_core: bool,
    /// Schema version
    pub version: u32,
    /// Schema description
    pub description: String,
    /// List of created fields
    pub fields: Vec<SchemaField>,
    /// List of created relationships
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relationships: Vec<crate::models::schema::SchemaRelationship>,
    /// Optional warnings, e.g. a field name shadowing a reserved core property
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// Create a custom schema with fields and relationships
///
/// # Tool: create_schema
///
/// Creates a new schema definition from explicit field and relationship
/// definitions. Field names are stored as given (bare, not namespace-prefixed).
/// A name colliding with a reserved core property is reported in `warnings`.
///
/// # Parameters
/// - `name`: Schema name (e.g., "Invoice", "Customer")
/// - `description`: Optional prose summary, stored for semantic discovery only
/// - `fields`: Explicit field definitions
/// - `relationships`: Optional relationship definitions to other schemas
///
/// # Returns
/// - `schema_id`: Generated schema ID (snake_case)
/// - `fields`: List of created fields
/// - `relationships`: List of created relationships
/// - `warnings`: e.g. a field name shadowing a reserved core property
///
/// # Errors
/// - `INVALID_PARAMS`: If name is empty or `fields` is missing
/// - `INTERNAL_ERROR`: If schema creation fails
pub async fn handle_create_schema(
    node_service: &Arc<NodeService>,
    params: Value,
) -> Result<Value, MarkdownError> {
    // Locate malformed field entries before serde sees them. Deserializing the
    // whole payload at once reports only the missing key ("missing field
    // `type`") with no indication of WHICH array element is at fault, which
    // leaves an LLM caller unable to repair the call: it re-sends with a
    // different part mutated and degrades the arguments further on each retry.
    describe_malformed_fields(&params, "fields")?;

    let params: CreateSchemaParams = serde_json::from_value(params)
        .map_err(|e| MarkdownError::invalid_params(format!("{e}")))?;

    if params.name.trim().is_empty() {
        return Err(MarkdownError::invalid_params(
            "name cannot be empty".to_string(),
        ));
    }

    // `fields` absent from the request (`None`) is always an error — the caller
    // never defined what the type holds. `fields: []` (`Some(vec![])`) is always
    // valid: an explicit, deliberate choice such as a relationship-only schema.
    let Some(explicit_fields) = params.fields else {
        return Err(MarkdownError::invalid_params(
            "\"fields\" is required. List every field explicitly, e.g. \
             [{\"name\":\"amount\",\"type\":\"number\"}]. \"description\" is a prose \
             summary only and is not parsed into fields."
                .to_string(),
        ));
    };

    let (stored_fields, warnings) = warn_reserved_property_names(explicit_fields);

    // Get relationships (default to empty)
    let relationships = params.relationships.unwrap_or_default();

    // Reject a relationship whose targetType doesn't exist yet, before
    // creating the node — see validate_relationship_targets_exist.
    validate_relationship_targets_exist(node_service, &relationships).await?;

    // Generate schema ID
    let schema_id = crate::services::node_service::normalize_schema_id(&params.name);

    // Check if schema already exists — return a clear error so the agent knows
    // to use create_node instead of retrying create_schema.
    if matches!(node_service.get_schema_node(&schema_id).await, Ok(Some(_))) {
        return Err(MarkdownError::invalid_params(format!(
            "Schema '{}' already exists. Use create_node with node_type='{}' to create instances.",
            params.name, schema_id
        )));
    }

    // Schema properties (description is stored as child node subtree, not in properties)
    let description_text = params
        .description
        .clone()
        .unwrap_or_else(|| format!("Schema for {}", params.name));
    let mut properties = serde_json::json!({
        "isCore": false,
        "schemaVersion": 1,
        "fields": &stored_fields,
        "relationships": &relationships
    });
    if let Some(ref template) = params.title_template {
        properties["titleTemplate"] = serde_json::Value::String(template.clone());
    }
    if let Some(ref template) = params.properties_header_summary_template {
        properties["propertiesHeaderSummaryTemplate"] = serde_json::Value::String(template.clone());
    }

    // Create schema node params — no explicit ID; create_node_with_parent derives it from content
    let schema_node_params = CreateNodeParams {
        id: None,
        node_type: "schema".to_string(),
        content: params.name.clone(),
        parent_id: None,
        position: crate::services::InsertPositionOwned::End,
        properties,
    };

    // Store the schema node
    let created_schema_id = node_service
        .create_node_with_parent(schema_node_params)
        .await
        .map_err(|e| {
            MarkdownError::internal_error(format!(
                "Failed to create schema node for '{}': {}",
                schema_id, e
            ))
        })?;

    // Store the description as a child node subtree so it is included in the
    // schema's embedding and enables synonym-based semantic discovery.
    create_description_subtree(node_service, &created_schema_id, &description_text).await?;

    let output = CreateSchemaOutput {
        schema_id: schema_id.clone(),
        is_core: false,
        version: 1,
        description: description_text,
        fields: stored_fields,
        relationships,
        warnings: if warnings.is_empty() {
            None
        } else {
            Some(warnings)
        },
    };

    serde_json::to_value(&output)
        .map_err(|e| MarkdownError::internal_error(format!("Failed to serialize output: {}", e)))
}

// ============================================================================
// Schema Relationship Operations
// ============================================================================

/// Parameters for add_schema_relationship.
///
/// Not currently wired to any agent tool (no `add_schema_relationship` entry
/// in `Tool::ALL`) — `deny_unknown_fields` is added anyway since this sits on
/// the same `Value`-deserialization boundary as the tool-reachable structs in
/// this module, and stays a no-op until/unless a tool is wired to it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddSchemaRelationshipParams {
    /// Schema ID to add the relationship to
    pub schema_id: String,
    /// Relationship definition to add
    pub relationship: crate::models::schema::SchemaRelationship,
}

/// Parameters for remove_schema_relationship. Not currently tool-reachable —
/// see [`AddSchemaRelationshipParams`].
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoveSchemaRelationshipParams {
    /// Schema ID to remove the relationship from
    pub schema_id: String,
    /// Name of the relationship to remove
    pub relationship_name: String,
}

/// A single field rename operation within update_schema
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FieldRename {
    /// Current field name
    pub from: String,
    /// New field name
    pub to: String,
}

/// Parameters for update_schema (batch operations)
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateSchemaParams {
    /// Schema ID to update
    pub schema_id: String,
    /// Fields to add
    #[serde(default)]
    pub add_fields: Option<Vec<SchemaField>>,
    /// Field names to remove
    #[serde(default)]
    pub remove_fields: Option<Vec<String>>,
    /// Field renames — rekeys property data on all existing nodes of this type
    /// and updates the schema definition atomically.
    #[serde(default)]
    pub rename_fields: Option<Vec<FieldRename>>,
    /// Relationships to add
    #[serde(default)]
    pub add_relationships: Option<Vec<crate::models::schema::SchemaRelationship>>,
    /// Relationship names to remove (soft-delete: edge table preserved)
    #[serde(default)]
    pub remove_relationships: Option<Vec<String>>,
    /// New description (optional)
    #[serde(default)]
    pub description: Option<String>,
    /// Set or update the title template. Pass `null` (absent) to leave unchanged.
    /// Use `{field_name}` tokens referencing fields defined in the schema.
    /// Example: `"{first_name} {last_name}"`
    #[serde(default)]
    pub title_template: Option<String>,
    /// Set or update the properties header summary template. Pass `null` (absent) to leave unchanged.
    /// Uses the same `{field_name}` syntax. Evaluated client-side only.
    /// Example: `"{status} · {company}"`
    #[serde(default)]
    pub properties_header_summary_template: Option<String>,
    /// If true, proceed with the schema update even if active playbooks would be
    /// affected. If false (default), return an error listing the affected playbooks.
    #[serde(default)]
    pub force: bool,
}

/// Output for schema update operations
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SchemaUpdateOutput {
    pub schema_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields_added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields_removed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fields_renamed: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationships_added: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relationships_removed: Option<usize>,
    /// Playbooks affected by this schema change (present when force=true and playbooks were affected)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_playbooks: Option<Vec<String>>,
}

/// Add a relationship definition to an existing schema
///
/// # Tool: add_schema_relationship
///
/// Adds a new relationship type to a schema. This creates the edge table DDL
/// but doesn't create any actual edges - use `create_relationship` for that.
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify
/// - `relationship`: The relationship definition to add
pub async fn handle_add_schema_relationship(
    node_service: &Arc<NodeService>,
    params: Value,
) -> Result<Value, MarkdownError> {
    let params: AddSchemaRelationshipParams = serde_json::from_value(params)
        .map_err(|e| MarkdownError::invalid_params(format!("{e}")))?;

    // Get existing schema
    let schema = node_service
        .get_schema_node(&params.schema_id)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?
        .ok_or_else(|| {
            MarkdownError::invalid_params(format!("Schema '{}' not found", params.schema_id))
        })?;

    // Check if relationship already exists
    if schema
        .relationships
        .iter()
        .any(|r| r.name == params.relationship.name)
    {
        return Err(MarkdownError::invalid_params(format!(
            "Relationship '{}' already exists in schema '{}'",
            params.relationship.name, params.schema_id
        )));
    }

    // Reject a targetType that doesn't exist yet — see
    // validate_relationship_targets_exist.
    validate_relationship_targets_exist(node_service, std::slice::from_ref(&params.relationship))
        .await?;

    // Build updated relationships
    let mut relationships = schema.relationships.clone();
    relationships.push(params.relationship.clone());

    // Update schema node
    let properties = serde_json::json!({
        "isCore": schema.is_core,
        "version": schema.schema_version,
        "fields": schema.fields,
        "relationships": relationships
    });

    let update = NodeUpdate {
        properties: Some(properties),
        ..Default::default()
    };

    node_service
        .update_node_unchecked(&params.schema_id, update)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to update schema: {}", e)))?;

    Ok(serde_json::json!({
        "success": true,
        "schemaId": params.schema_id,
        "relationshipAdded": params.relationship.name
    }))
}

/// Remove a relationship definition from a schema (soft-delete)
///
/// # Tool: remove_schema_relationship
///
/// Removes a relationship from the schema definition. The edge table and any
/// existing edges are preserved (soft-delete) - they're just hidden from the
/// active schema. Re-adding the relationship will restore access to existing data.
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify
/// - `relationship_name`: Name of the relationship to remove
pub async fn handle_remove_schema_relationship(
    node_service: &Arc<NodeService>,
    params: Value,
) -> Result<Value, MarkdownError> {
    let params: RemoveSchemaRelationshipParams = serde_json::from_value(params)
        .map_err(|e| MarkdownError::invalid_params(format!("{e}")))?;

    // Get existing schema
    let schema = node_service
        .get_schema_node(&params.schema_id)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?
        .ok_or_else(|| {
            MarkdownError::invalid_params(format!("Schema '{}' not found", params.schema_id))
        })?;

    // Check if relationship exists
    if !schema
        .relationships
        .iter()
        .any(|r| r.name == params.relationship_name)
    {
        return Err(MarkdownError::invalid_params(format!(
            "Relationship '{}' not found in schema '{}'",
            params.relationship_name, params.schema_id
        )));
    }

    // Build updated relationships (remove the one specified)
    let relationships: Vec<_> = schema
        .relationships
        .into_iter()
        .filter(|r| r.name != params.relationship_name)
        .collect();

    // Update schema node
    let properties = serde_json::json!({
        "isCore": schema.is_core,
        "version": schema.schema_version,
        "fields": schema.fields,
        "relationships": relationships
    });

    let update = NodeUpdate {
        properties: Some(properties),
        ..Default::default()
    };

    node_service
        .update_node_unchecked(&params.schema_id, update)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to update schema: {}", e)))?;

    Ok(serde_json::json!({
        "success": true,
        "schemaId": params.schema_id,
        "relationshipRemoved": params.relationship_name,
        "note": "Edge table and existing edges preserved (soft-delete)"
    }))
}

/// Update a schema with multiple changes
///
/// # Tool: update_schema
///
/// Batch update a schema's fields and relationships. Useful when making
/// multiple changes at once. For single operations, prefer the specific
/// `add_schema_relationship` or `remove_schema_relationship` tools.
///
/// # Parameters
/// - `schema_id`: ID of the schema to update
/// - `add_fields`: Fields to add
/// - `remove_fields`: Field names to remove
/// - `add_relationships`: Relationships to add
/// - `remove_relationships`: Relationship names to remove (soft-delete)
/// - `description`: New description (optional)
pub async fn handle_update_schema(
    node_service: &Arc<NodeService>,
    params: Value,
) -> Result<Value, MarkdownError> {
    // See `describe_malformed_fields` — locate a bad entry before serde reports
    // only the absent key with no position.
    describe_malformed_fields(&params, "add_fields")?;

    let params: UpdateSchemaParams = serde_json::from_value(params)
        .map_err(|e| MarkdownError::invalid_params(format!("{e}")))?;

    // --- Phase 0: Verify schema exists, validate renames, run playbook impact check ---
    // Schema existence is verified upfront so rename/playbook validation errors are reported
    // before any mutations execute.
    node_service
        .get_schema_node(&params.schema_id)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?
        .ok_or_else(|| {
            MarkdownError::invalid_params(format!("Schema '{}' not found", params.schema_id))
        })?;

    // Validate renames before executing any mutations (including the playbook guard below)
    if let Some(ref renames) = params.rename_fields {
        let mut seen_sources = std::collections::HashSet::new();
        let mut seen_destinations = std::collections::HashSet::new();
        for rename in renames {
            if rename.from.trim().is_empty() || rename.to.trim().is_empty() {
                return Err(MarkdownError::invalid_params(
                    "rename_fields entries must have non-empty 'from' and 'to'".to_string(),
                ));
            }
            if rename.from == rename.to {
                return Err(MarkdownError::invalid_params(format!(
                    "rename_fields: 'from' and 'to' are the same: '{}'",
                    rename.from
                )));
            }
            if !seen_sources.insert(&rename.from) {
                return Err(MarkdownError::invalid_params(format!(
                    "rename_fields: duplicate source field name '{}'",
                    rename.from
                )));
            }
            if !seen_destinations.insert(&rename.to) {
                return Err(MarkdownError::invalid_params(format!(
                    "rename_fields: duplicate destination field name '{}'",
                    rename.to
                )));
            }
        }
    }

    // Check if any active playbooks would be affected by this schema change.
    // Done before any mutations so a blocked rename doesn't partially execute.
    let affected =
        crate::playbook::validation::check_schema_change_impact(&params.schema_id, node_service)
            .await
            .map_err(|e| MarkdownError::internal_error(format!("Impact analysis failed: {}", e)))?;

    if !affected.is_empty() && !params.force {
        let names: Vec<String> = affected.iter().map(|a| a.to_string()).collect();
        return Err(MarkdownError::invalid_params(format!(
            "Schema change would affect {} active playbook(s): {}. Use force=true to proceed.",
            affected.len(),
            names.join("; ")
        )));
    }

    let affected_names: Option<Vec<String>> = if !affected.is_empty() {
        Some(
            affected
                .iter()
                .map(|a| format!("{} ({})", a.playbook_name, a.playbook_id))
                .collect(),
        )
    } else {
        None
    };

    // --- Phase 0: Reject unprefixed field names on a type NodeSpace owns ---
    //
    // A bare name on a core type can be claimed by a core property in a future
    // release, and the user's field would then collide with it. User-defined
    // types carry no such risk — NodeSpace never adds core fields to them — so
    // their fields are stored under the caller's names.
    //
    // This runs before Phase 1 rather than validating the final field list,
    // because `rename_schema_field` migrates node property data and rewrites the
    // schema per rename, committing each one as it goes. Validating afterwards
    // would return an error with the offending rename already persisted across
    // every node instance. Both routes that can introduce a name are checked
    // here: `add_fields` supplies one directly, and `rename_fields` can turn an
    // already-prefixed field into a bare one.
    if schema_is_core(node_service, &params.schema_id).await? {
        let reject = |name: &str, how: &str| {
            MarkdownError::invalid_params(format!(
                "Field '{name}' {how} core schema '{}' must carry a namespace prefix \
                 (e.g. 'custom:{name}'). Unprefixed names on core types are reserved \
                 for core properties.",
                params.schema_id
            ))
        };

        if let Some(ref add_fields) = params.add_fields {
            for field in add_fields {
                if !has_namespace_prefix(&field.name) {
                    return Err(reject(&field.name, "added to"));
                }
            }
        }

        if let Some(ref renames) = params.rename_fields {
            for rename in renames {
                if !has_namespace_prefix(&rename.to) {
                    return Err(reject(&rename.to, "renamed on"));
                }
            }
        }
    }

    // --- Phase 1: Process renames (each rename migrates data + updates schema definition) ---
    let mut fields_renamed = 0;
    if let Some(ref renames) = params.rename_fields {
        for rename in renames {
            node_service
                .rename_schema_field(&params.schema_id, &rename.from, &rename.to)
                .await
                .map_err(|e| {
                    MarkdownError::invalid_params(format!("Field rename failed: {}", e))
                })?;
            fields_renamed += 1;
        }
    }

    // --- Phase 2: Re-fetch schema (reflects any renames applied above) ---
    let schema = node_service
        .get_schema_node(&params.schema_id)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to get schema: {}", e)))?
        .ok_or_else(|| {
            MarkdownError::invalid_params(format!(
                "Schema '{}' not found after renames",
                params.schema_id
            ))
        })?;

    // Process fields
    let mut fields = schema.fields.clone();
    let mut fields_added = 0;
    let mut fields_removed = 0;

    if let Some(remove_names) = &params.remove_fields {
        let before = fields.len();
        fields.retain(|f| !remove_names.contains(&f.name));
        fields_removed = before - fields.len();
    }

    if let Some(ref add_fields) = params.add_fields {
        // Check for duplicates before adding
        for field in add_fields {
            if fields.iter().any(|f| f.name == field.name) {
                return Err(MarkdownError::invalid_params(format!(
                    "Field '{}' already exists in schema '{}'",
                    field.name, params.schema_id
                )));
            }
        }
        fields_added = add_fields.len();
        fields.extend(add_fields.clone());
    }

    // Process relationships
    let mut relationships = schema.relationships.clone();
    let mut relationships_added = 0;
    let mut relationships_removed = 0;

    if let Some(remove_names) = &params.remove_relationships {
        let before = relationships.len();
        relationships.retain(|r| !remove_names.contains(&r.name));
        relationships_removed = before - relationships.len();
    }

    if let Some(ref add_rels) = params.add_relationships {
        // Check for duplicates before adding
        for rel in add_rels {
            if relationships.iter().any(|r| r.name == rel.name) {
                return Err(MarkdownError::invalid_params(format!(
                    "Relationship '{}' already exists in schema '{}'",
                    rel.name, params.schema_id
                )));
            }
        }
        // Reject a targetType that doesn't exist yet — see
        // validate_relationship_targets_exist.
        validate_relationship_targets_exist(node_service, add_rels).await?;
        relationships_added = add_rels.len();
        relationships.extend(add_rels.clone());
    }

    // Resolve title_template: use new value if provided, otherwise keep existing
    let title_template = params.title_template.or(schema.title_template);

    // Resolve properties_header_summary_template: use new value if provided, otherwise keep existing
    let properties_header_summary_template = params
        .properties_header_summary_template
        .or(schema.properties_header_summary_template);

    // Build updated properties (description is stored as child subtree, not in properties)
    let mut properties = serde_json::json!({
        "isCore": schema.is_core,
        "schemaVersion": schema.schema_version,
        "fields": fields,
        "relationships": relationships
    });
    if let Some(ref template) = title_template {
        properties["titleTemplate"] = serde_json::Value::String(template.clone());
    }
    if let Some(ref template) = properties_header_summary_template {
        properties["propertiesHeaderSummaryTemplate"] = serde_json::Value::String(template.clone());
    }

    // Validate the updated schema before saving (update_node_unchecked bypasses the behavior
    // pipeline, so we run SchemaNodeBehavior validation explicitly here)
    let temp_node = Node::new(
        "schema".to_string(),
        schema.content.clone(),
        properties.clone(),
    );
    let updated_schema = SchemaNode::from_node(temp_node).map_err(|e| {
        MarkdownError::invalid_params(format!("Failed to build schema for validation: {}", e))
    })?;
    SchemaNodeBehavior
        .validate_schema_node(&updated_schema)
        .map_err(|e| MarkdownError::invalid_params(format!("Schema validation failed: {}", e)))?;

    let update = NodeUpdate {
        properties: Some(properties),
        ..Default::default()
    };

    node_service
        .update_node_unchecked(&params.schema_id, update)
        .await
        .map_err(|e| MarkdownError::internal_error(format!("Failed to update schema: {}", e)))?;

    // If a new description was provided, replace the description child subtree
    if let Some(ref new_description) = params.description {
        replace_description_subtree(node_service, &params.schema_id, new_description).await?;
    }

    let output = SchemaUpdateOutput {
        schema_id: params.schema_id,
        success: true,
        fields_added: if fields_added > 0 {
            Some(fields_added)
        } else {
            None
        },
        fields_removed: if fields_removed > 0 {
            Some(fields_removed)
        } else {
            None
        },
        fields_renamed: if fields_renamed > 0 {
            Some(fields_renamed)
        } else {
            None
        },
        relationships_added: if relationships_added > 0 {
            Some(relationships_added)
        } else {
            None
        },
        relationships_removed: if relationships_removed > 0 {
            Some(relationships_removed)
        } else {
            None
        },
        affected_playbooks: affected_names,
    };

    serde_json::to_value(&output)
        .map_err(|e| MarkdownError::internal_error(format!("Failed to serialize output: {}", e)))
}

// ============================================================================
// Description Subtree Helpers
// ============================================================================

/// Create a description child subtree under a schema node.
///
/// Parses the markdown description into node types (text, header, etc.) and
/// inserts them as children of the schema node via bulk insert. The subtree
/// is then included in the schema's embedding via `get_aggregated_content`,
/// enabling synonym-based semantic discovery of schemas.
async fn create_description_subtree(
    node_service: &Arc<NodeService>,
    schema_id: &str,
    description: &str,
) -> Result<(), MarkdownError> {
    use crate::markdown::prepare_nodes_from_markdown;

    if description.trim().is_empty() {
        return Ok(());
    }

    let prepared = prepare_nodes_from_markdown(description, Some(schema_id.to_string()))?;
    if prepared.is_empty() {
        return Ok(());
    }

    let bulk_nodes: Vec<(
        String,
        String,
        String,
        Option<String>,
        f64,
        serde_json::Value,
    )> = prepared
        .into_iter()
        .map(|n| {
            (
                n.id,
                n.node_type,
                n.content,
                n.parent_id,
                n.order,
                n.properties,
            )
        })
        .collect();

    node_service
        .bulk_create_hierarchy(bulk_nodes)
        .await
        .map_err(|e| {
            MarkdownError::internal_error(format!(
                "Failed to create description subtree for schema '{}': {}",
                schema_id, e
            ))
        })?;

    Ok(())
}

/// Replace the description child subtree of a schema node with new content.
///
/// Atomically deletes the entire existing subtree (all descendants, not just direct children),
/// then creates a fresh subtree from the new markdown description.
async fn replace_description_subtree(
    node_service: &Arc<NodeService>,
    schema_id: &str,
    new_description: &str,
) -> Result<(), MarkdownError> {
    // Delete the entire descendant subtree in one statement (recursive CTE).
    // This correctly handles nested markdown structures (e.g. header → text children).
    node_service
        .store()
        .delete_children_subtree_unchecked(schema_id)
        .await
        .map_err(|e| {
            MarkdownError::internal_error(format!(
                "Failed to delete description subtree for schema '{}': {}",
                schema_id, e
            ))
        })?;

    // Create new description subtree
    create_description_subtree(node_service, schema_id, new_description).await
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod schema_test;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schema::SchemaProtectionLevel;

    #[test]
    fn test_has_namespace_prefix() {
        assert!(has_namespace_prefix("custom:capacity"));
        assert!(has_namespace_prefix("org:region"));

        assert!(!has_namespace_prefix("capacity"));

        // Malformed forms are not prefixes. `validate_schema_field_name` also
        // rejects these, but this must not depend on that happening first.
        assert!(!has_namespace_prefix(":capacity"));
        assert!(!has_namespace_prefix("custom:"));
        assert!(!has_namespace_prefix("a:b:c"));
        assert!(!has_namespace_prefix(":"));
        assert!(!has_namespace_prefix(""));
    }

    #[test]
    fn test_normalize_schema_id() {
        use crate::services::node_service::normalize_schema_id;
        assert_eq!(normalize_schema_id("Invoice"), "invoice");
        assert_eq!(normalize_schema_id("Customer Profile"), "customer_profile");
        assert_eq!(normalize_schema_id("code_block"), "code_block");
        assert_eq!(normalize_schema_id("Project"), "project");
    }

    #[test]
    fn test_integration_schema_id_generation() {
        use crate::services::node_service::normalize_schema_id;
        let entity_name = "Customer Invoice";
        let schema_id = normalize_schema_id(entity_name);
        // normalize_schema_id joins on '_' (see its dedicated unit tests); the core
        // hardcoded schema ids that use hyphens (code-block, …) are not generated
        // through this path.
        assert_eq!(schema_id, "customer_invoice");
    }

    fn field(name: &str) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            field_type: "string".to_string(),
            local_only: false,
            protection: SchemaProtectionLevel::User,
            core_values: None,
            user_values: None,
            indexed: false,
            required: None,
            extensible: None,
            default: None,
            description: None,
            item_type: None,
            fields: None,
            item_fields: None,
            unique: None,
            unique_case_insensitive: None,
        }
    }

    #[test]
    fn test_warn_reserved_property_names_warns_on_collision() {
        let (fields, warnings) =
            warn_reserved_property_names(vec![field("status"), field("capacity")]);

        // Explicit fields are stored verbatim — never silently rewritten.
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0].name, "status");
        assert_eq!(fields[1].name, "capacity");

        assert!(
            warnings
                .iter()
                .any(|w| w.contains("status") && w.contains("reserved core property")),
            "Expected a reserved-core-property warning for 'status', got {warnings:?}"
        );
        assert!(
            !warnings.iter().any(|w| w.contains("capacity")),
            "Unreserved field name should not warn, got {warnings:?}"
        );
    }

    #[test]
    fn test_warn_reserved_property_names_no_collision_no_warnings() {
        let (fields, warnings) = warn_reserved_property_names(vec![field("email")]);
        assert_eq!(fields.len(), 1);
        assert!(warnings.is_empty());
    }
}
