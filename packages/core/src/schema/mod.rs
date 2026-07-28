//! Schema creation and updates.
//!
//! Provides the `create_schema` tool for creating custom schemas with fields and relationships.
//! Supports both explicit field/relationship definitions and natural language descriptions
//! with intelligent type inference.

use crate::behaviors::SchemaNodeBehavior;
use crate::markdown::MarkdownError;
use crate::models::schema::{EnumValue, SchemaField, SchemaProtectionLevel};
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
pub struct CreateSchemaParams {
    /// Schema name (e.g., "Invoice", "Customer")
    pub name: String,
    /// Natural language description of entity fields (optional if fields provided directly)
    #[serde(default)]
    pub description: Option<String>,
    /// Optional explicit field definitions (takes precedence over description parsing)
    #[serde(default)]
    pub fields: Option<Vec<SchemaField>>,
    /// Optional relationship definitions
    #[serde(default)]
    pub relationships: Option<Vec<crate::models::schema::SchemaRelationship>>,
    /// Optional additional constraints for explicit type hints (used with description)
    #[serde(default)]
    pub additional_constraints: Option<AdditionalConstraints>,
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

/// Additional constraints for schema creation
#[derive(Debug, Deserialize)]
pub struct AdditionalConstraints {
    /// List of field names that are required
    #[serde(default)]
    pub required_fields: Option<Vec<String>>,
    /// Default values for specific fields
    #[serde(default)]
    pub default_values: Option<std::collections::HashMap<String, Value>>,
    /// Enum values for specific fields
    #[serde(default)]
    pub enum_values: Option<std::collections::HashMap<String, Vec<String>>>,
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
    /// Optional warnings about ambiguous descriptions
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warnings: Option<Vec<String>>,
}

/// Inferred field information from natural language
#[derive(Debug, Clone)]
struct InferredField {
    name: String,
    field_type: String,
    required: bool,
    enum_values: Option<Vec<String>>,
    warnings: Vec<String>,
}

/// Create a custom schema with fields and relationships
///
/// # Tool: create_schema
///
/// Creates a new schema definition with optional fields and relationships.
/// Fields can be provided explicitly or inferred from a natural language description.
/// Both routes store field names as given (bare, not namespace-prefixed), so the
/// same intent yields the same stored keys whichever route created the schema.
/// A name colliding with a reserved core property is reported in `warnings`.
///
/// # Parameters
/// - `name`: Schema name (e.g., "Invoice", "Customer")
/// - `description`: Optional natural language description of fields
/// - `fields`: Optional explicit field definitions (takes precedence over description)
/// - `relationships`: Optional relationship definitions to other schemas
/// - `additional_constraints`: Optional type hints for description parsing
///
/// # Returns
/// - `schema_id`: Generated schema ID (snake_case)
/// - `fields`: List of created fields
/// - `relationships`: List of created relationships
/// - `warnings`: Any ambiguities or assumptions made
///
/// # Errors
/// - `INVALID_PARAMS`: If name is empty or both description and fields are missing
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

    // Determine fields: explicit fields take precedence, otherwise parse description
    let (namespaced_fields, warnings) = if let Some(explicit_fields) = params.fields {
        // Use explicit fields directly (already properly typed by caller)
        (explicit_fields, Vec::new())
    } else if let Some(ref description) = params.description {
        if description.trim().is_empty() {
            return Err(MarkdownError::invalid_params(
                "Either 'fields' or non-empty 'description' must be provided".to_string(),
            ));
        }
        // Parse natural language and infer fields
        let inferred_fields = parse_field_descriptions(description);
        let fields = apply_constraints(inferred_fields, params.additional_constraints);
        // Collect warnings AFTER normalization: it appends the reserved-core-property
        // warning, which is dropped entirely if the snapshot is taken beforehand.
        let (normalized, warnings) = normalize_inferred_fields(fields);
        (normalized, warnings)
    } else {
        // No fields and no description - create schema with empty fields
        (Vec::new(), Vec::new())
    };

    // Get relationships (default to empty)
    let relationships = params.relationships.unwrap_or_default();

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
        "fields": &namespaced_fields,
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
        fields: namespaced_fields,
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

/// Parameters for add_schema_relationship
#[derive(Debug, Deserialize)]
pub struct AddSchemaRelationshipParams {
    /// Schema ID to add the relationship to
    pub schema_id: String,
    /// Relationship definition to add
    pub relationship: crate::models::schema::SchemaRelationship,
}

/// Parameters for remove_schema_relationship
#[derive(Debug, Deserialize)]
pub struct RemoveSchemaRelationshipParams {
    /// Schema ID to remove the relationship from
    pub schema_id: String,
    /// Name of the relationship to remove
    pub relationship_name: String,
}

/// A single field rename operation within update_schema
#[derive(Debug, Deserialize)]
pub struct FieldRename {
    /// Current field name
    pub from: String,
    /// New field name
    pub to: String,
}

/// Parameters for update_schema (batch operations)
#[derive(Debug, Deserialize)]
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

        // Extending a type NodeSpace owns requires a namespace prefix. A bare
        // name here can be claimed by a core property in a future release, and
        // the user's field would then collide with it. User-defined types carry
        // no such risk — NodeSpace never adds core fields to them — so their
        // fields are stored under the caller's names.
        if schema.is_core {
            for field in add_fields {
                if !field.name.contains(':') {
                    return Err(MarkdownError::invalid_params(format!(
                        "Field '{}' added to core schema '{}' must carry a namespace prefix \
                         (e.g. 'custom:{}'). Unprefixed names on core types are reserved for \
                         core properties.",
                        field.name, params.schema_id, field.name
                    )));
                }
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

/// Parse natural language description and extract fields
fn parse_field_descriptions(description: &str) -> Vec<InferredField> {
    let mut fields = Vec::new();

    // Split by common delimiters: commas, "and", semicolons
    let field_descriptions = split_field_descriptions(description);

    for field_desc in field_descriptions {
        if let Some(inferred) = parse_single_field_description(&field_desc, description) {
            fields.push(inferred);
        }
    }

    fields
}

/// Split description into individual field descriptions
fn split_field_descriptions(description: &str) -> Vec<String> {
    // Split by comma, semicolon, or " and "
    let parts: Vec<&str> = description
        .split([',', ';'])
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    // Further split by " and " within each part
    let mut result = Vec::new();
    for part in parts {
        let subparts: Vec<&str> = part.split(" and ").map(|s| s.trim()).collect();
        for subpart in subparts {
            if !subpart.is_empty() {
                result.push(subpart.to_string());
            }
        }
    }

    result
}

/// Parse a single field description and infer its properties
fn parse_single_field_description(
    field_desc: &str,
    _full_description: &str,
) -> Option<InferredField> {
    let field_desc = field_desc.trim();
    if field_desc.is_empty() {
        return None;
    }

    // Extract field name (first word or phrase before parentheses/keywords)
    let field_name = extract_field_name(field_desc)?;

    // Infer field type
    let field_type = infer_field_type(field_desc, &field_name);

    // Extract enum values if present
    let enum_values = extract_enum_values(field_desc);

    // Check if field is required
    let required = is_field_required(field_desc);

    // Collect warnings
    let mut warnings = Vec::new();
    if field_type == "string" && contains_any(field_desc, &["amount", "price", "cost"]) {
        warnings.push(format!(
            "Field '{}' mentions monetary amount but inferred as string. Consider using number type.",
            field_name
        ));
    }

    Some(InferredField {
        name: field_name,
        field_type,
        required,
        enum_values,
        warnings,
    })
}

/// Extract field name from description
fn extract_field_name(field_desc: &str) -> Option<String> {
    // Pattern 1: "field_name (required)" or "field_name (something/else)"
    if let Some(name) = extract_before_paren(field_desc) {
        return Some(name);
    }

    // Pattern 2: "field_name in USD" or similar
    if let Some(name) = extract_before_keyword(field_desc, &["in ", "with ", "that "]) {
        return Some(name);
    }

    // Pattern 3: Just take the first few words until a keyword
    let words: Vec<&str> = field_desc.split_whitespace().take(3).collect();
    if !words.is_empty() {
        let combined = words.join("_").to_lowercase();
        if !combined.is_empty() && combined.chars().all(|c| c.is_alphanumeric() || c == '_') {
            return Some(combined);
        }
    }

    None
}

/// Extract text before first parenthesis
fn extract_before_paren(text: &str) -> Option<String> {
    if let Some(idx) = text.find('(') {
        let name = text[..idx].trim();
        if !name.is_empty() {
            return Some(normalize_field_name(name));
        }
    }
    None
}

/// Extract text before specific keyword
fn extract_before_keyword(text: &str, keywords: &[&str]) -> Option<String> {
    for keyword in keywords {
        if let Some(idx) = text.find(keyword) {
            let name = text[..idx].trim();
            if !name.is_empty() {
                return Some(normalize_field_name(name));
            }
        }
    }
    None
}

/// Normalize field name to snake_case
fn normalize_field_name(name: &str) -> String {
    name.to_lowercase()
        .replace([' ', '-'], "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>()
        .split('_')
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

/// Convert a value string to title case for display label
/// e.g., "in_progress" -> "In Progress", "DRAFT" -> "Draft"
fn to_title_case(s: &str) -> String {
    s.replace('_', " ")
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Infer field type from description
///
/// # Type Inference Priority (in order of precedence)
/// 1. **Date/Time** - Keywords: "date", "deadline", "due", "created", etc.
///    - Highest priority because date/time are specific and unambiguous
/// 2. **Boolean** - Keywords: "yes/no", "enabled/disabled", "active/inactive"
///    - High priority because boolean is explicit
/// 3. **Numeric** - Keywords: "amount", "price", "quantity", "count", "usd"
///    - Also checked in field name (e.g., "invoice_amount" → number)
/// 4. **Enum** - Pattern: "(option1/option2)" with parentheses and forward slashes
///    - High specificity, explicit syntax
/// 5. **Array** - Keywords: "list", "items", "tags", "array", "multiple"
/// 6. **String** - Default fallback for any ambiguous descriptions
///
/// Note: If a description mentions "enabled date", the date check will match first,
/// so it returns "date" (most specific keyword wins).
fn infer_field_type(field_desc: &str, field_name: &str) -> String {
    let lower = field_desc.to_lowercase();
    let name_lower = field_name.to_lowercase();

    // Priority 1: Check for date/time (most specific, highest priority)
    if contains_any(
        &lower,
        &[
            "date",
            "when",
            "time",
            "deadline",
            "due",
            "expires",
            "scheduled",
            "created",
        ],
    ) {
        return "date".to_string();
    }

    // Priority 2: Check for boolean (explicit yes/no values)
    if contains_any(
        &lower,
        &[
            "yes", "no", "enabled", "disabled", "active", "inactive", "true", "false",
        ],
    ) {
        return "boolean".to_string();
    }

    // Priority 3: Check for numeric values
    if contains_any(
        &lower,
        &[
            "amount",
            "price",
            "cost",
            "count",
            "number",
            "quantity",
            "value",
            "total",
            "sum",
            "average",
            "percentage",
            "rate",
            "usd",
            "dollars",
            "euros",
            "cents",
        ],
    ) || contains_any(
        &name_lower,
        &["amount", "price", "cost", "count", "quantity"],
    ) {
        return "number".to_string();
    }

    // Priority 4: Check for enum (explicit option syntax)
    if field_desc.contains('(') && field_desc.contains('/') {
        return "enum".to_string();
    }

    // Priority 5: Check for array/collection types
    if contains_any(&lower, &["list", "items", "tags", "array", "multiple"]) {
        return "array".to_string();
    }

    // Priority 6: Default to string for any ambiguous descriptions
    "string".to_string()
}

/// Extract enum values from "(option1/option2)" pattern
fn extract_enum_values(field_desc: &str) -> Option<Vec<String>> {
    // Match pattern: (value1/value2/value3)
    if let Some(start) = field_desc.find('(') {
        if let Some(end) = field_desc.find(')') {
            if start < end {
                let content = &field_desc[start + 1..end];
                if content.contains('/') {
                    let values: Vec<String> = content
                        .split('/')
                        .map(|s| s.trim().to_uppercase())
                        .filter(|s| !s.is_empty())
                        .collect();
                    if !values.is_empty() {
                        return Some(values);
                    }
                }
            }
        }
    }
    None
}

/// Check if field is marked as required
fn is_field_required(field_desc: &str) -> bool {
    let lower = field_desc.to_lowercase();
    contains_any(
        &lower,
        &["required", "must", "mandatory", "essential", "critical"],
    )
}

/// Check if any keyword is contained in text
fn contains_any(text: &str, keywords: &[&str]) -> bool {
    let lower = text.to_lowercase();
    keywords.iter().any(|kw| lower.contains(kw))
}

/// Apply additional constraints to inferred fields
fn apply_constraints(
    mut fields: Vec<InferredField>,
    constraints: Option<AdditionalConstraints>,
) -> Vec<InferredField> {
    let constraints = match constraints {
        Some(c) => c,
        None => return fields,
    };

    // Apply required field constraints
    if let Some(required_list) = constraints.required_fields {
        for field in &mut fields {
            if required_list.iter().any(|req| {
                req.to_lowercase().contains(&field.name.to_lowercase())
                    || field.name.to_lowercase().contains(&req.to_lowercase())
            }) {
                field.required = true;
            }
        }
    }

    // Apply enum value constraints
    if let Some(enum_map) = constraints.enum_values {
        for field in &mut fields {
            for (enum_field, values) in &enum_map {
                if enum_field
                    .to_lowercase()
                    .contains(&field.name.to_lowercase())
                    || field
                        .name
                        .to_lowercase()
                        .contains(&enum_field.to_lowercase())
                {
                    field.field_type = "enum".to_string();
                    field.enum_values = Some(values.iter().map(|v| v.to_uppercase()).collect());
                }
            }
        }
    }

    fields
}

/// Normalize inferred field names into stored field names.
///
/// Stored field names are bare (`capacity`), never namespace-prefixed. Both
/// `create_schema` paths must agree on this: the explicit-fields path stores
/// caller-supplied names verbatim, so inferring a `custom:` prefix here would
/// make the same user intent produce a different stored key depending on which
/// path created the schema. Stored names are user-visible keys — they appear in
/// `titleTemplate` tokens, CEL selectors, query filters and frontend lookups —
/// so the two paths disagreeing is a correctness problem, not a cosmetic one.
///
/// A name colliding with a reserved core property is reported as a warning
/// rather than silently rewritten, leaving the choice with the caller.
fn normalize_inferred_fields(
    inferred_fields: Vec<InferredField>,
) -> (Vec<SchemaField>, Vec<String>) {
    let mut warnings: Vec<String> = Vec::new();
    let fields = inferred_fields
        .into_iter()
        .map(|inferred| {
            let stored_name = normalize_field_name(&inferred.name);

            warnings.extend(inferred.warnings.iter().cloned());

            // Warn if field name matches a reserved core property
            if RESERVED_CORE_PROPERTIES.contains(&stored_name.as_str()) {
                warnings.push(format!(
                    "Field name '{}' matches a reserved core property and may be shadowed by it. Consider a more specific name.",
                    stored_name
                ));
            }

            // Convert string enum values to EnumValue with auto-generated labels
            let user_values = inferred.enum_values.as_ref().map(|values| {
                values.iter().map(|v| EnumValue {
                    value: v.clone(),
                    label: to_title_case(v),
                }).collect()
            });

            SchemaField {
                name: stored_name,
                field_type: inferred.field_type.clone(),
                protection: SchemaProtectionLevel::User,
                core_values: None,
                user_values,
                indexed: false, // Not indexed by default
                required: Some(inferred.required),
                extensible: Some(inferred.field_type == "enum"), // Enums are extensible by default
                default: None,
                description: None,
                item_type: if inferred.field_type == "array" {
                    Some("string".to_string())
                } else {
                    None
                },
                fields: None,
                item_fields: None,
            }
        })
        .collect();

    (fields, warnings)
}

#[cfg(test)]
#[path = "schema_test.rs"]
mod schema_test;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_field_name_with_parens() {
        let desc = "invoice number (required)";
        let name = extract_field_name(desc);
        assert_eq!(name, Some("invoice_number".to_string()));
    }

    #[test]
    fn test_extract_field_name_with_keyword() {
        let desc = "amount in USD";
        let name = extract_field_name(desc);
        assert_eq!(name, Some("amount".to_string()));
    }

    #[test]
    fn test_infer_number_type() {
        assert_eq!(infer_field_type("amount in USD", "amount"), "number");
        assert_eq!(infer_field_type("price", "price"), "number");
        assert_eq!(infer_field_type("total cost", "cost"), "number");
    }

    #[test]
    fn test_infer_date_type() {
        assert_eq!(infer_field_type("due date", "due_date"), "date");
        assert_eq!(infer_field_type("when is it due", "due"), "date");
        assert_eq!(infer_field_type("deadline", "deadline"), "date");
    }

    #[test]
    fn test_infer_enum_type() {
        assert_eq!(
            infer_field_type("status (draft/sent/paid)", "status"),
            "enum"
        );
    }

    #[test]
    fn test_infer_boolean_type() {
        // "yes/no" keywords match boolean type first (priority 2) before enum pattern (priority 4)
        assert_eq!(infer_field_type("enabled (yes/no)", "enabled"), "boolean");
        assert_eq!(infer_field_type("active or inactive", "active"), "boolean");
    }

    #[test]
    fn test_extract_enum_values() {
        let values = extract_enum_values("status (draft/sent/paid)");
        assert_eq!(
            values,
            Some(vec![
                "DRAFT".to_string(),
                "SENT".to_string(),
                "PAID".to_string()
            ])
        );
    }

    #[test]
    fn test_is_field_required() {
        assert!(is_field_required("invoice number (required)"));
        assert!(is_field_required("must have email"));
        assert!(!is_field_required("optional notes"));
    }

    #[test]
    fn test_normalize_field_name() {
        assert_eq!(normalize_field_name("Invoice Number"), "invoice_number");
        assert_eq!(normalize_field_name("first-name"), "first_name");
        assert_eq!(normalize_field_name("email address"), "email_address");
    }

    #[test]
    fn test_parse_field_descriptions_simple() {
        let desc = "invoice number, amount, status";
        let fields = parse_field_descriptions(desc);

        assert_eq!(fields.len(), 3);
        assert_eq!(fields[0].name, "invoice_number");
        assert_eq!(fields[1].name, "amount");
        assert_eq!(fields[2].name, "status");
    }

    #[test]
    fn test_parse_field_descriptions_with_types() {
        let desc = "invoice number (required), amount in USD, status (draft/sent/paid), due date";
        let fields = parse_field_descriptions(desc);

        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].field_type, "number"); // "number" keyword detected in "invoice number"
        assert_eq!(fields[1].field_type, "number"); // amount in USD
        assert_eq!(fields[2].field_type, "enum"); // "(options/separated/by/slashes)" pattern detected
        assert_eq!(fields[3].field_type, "date"); // due date
    }

    #[test]
    fn test_normalize_inferred_field_names() {
        let inferred = vec![
            InferredField {
                name: "invoice_number".to_string(),
                field_type: "string".to_string(),
                required: true,
                enum_values: None,
                warnings: vec![],
            },
            InferredField {
                name: "status".to_string(),
                field_type: "enum".to_string(),
                required: false,
                enum_values: Some(vec!["DRAFT".to_string(), "SENT".to_string()]),
                warnings: vec![],
            },
        ];

        let (fields, warnings) = normalize_inferred_fields(inferred);

        assert_eq!(fields[0].name, "invoice_number");
        assert_eq!(fields[1].name, "status");
        assert!(fields[0].required.unwrap());
        assert!(fields[1].extensible.unwrap()); // enum is extensible

        // "status" collides with a reserved core property, so the caller is warned
        // rather than having the name silently rewritten.
        assert!(
            warnings.iter().any(|w| w.contains("status")
                && w.contains("reserved core property")),
            "Expected a reserved-core-property warning for 'status', got {warnings:?}"
        );
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
    fn test_apply_constraints_required() {
        let fields = vec![InferredField {
            name: "email".to_string(),
            field_type: "string".to_string(),
            required: false,
            enum_values: None,
            warnings: vec![],
        }];

        let constraints = Some(AdditionalConstraints {
            required_fields: Some(vec!["email".to_string()]),
            default_values: None,
            enum_values: None,
        });

        let result = apply_constraints(fields, constraints);
        assert!(result[0].required);
    }

    #[test]
    fn test_apply_constraints_enum_values() {
        let fields = vec![InferredField {
            name: "status".to_string(),
            field_type: "string".to_string(),
            required: false,
            enum_values: None,
            warnings: vec![],
        }];

        let mut enum_map = std::collections::HashMap::new();
        enum_map.insert(
            "status".to_string(),
            vec!["active".to_string(), "inactive".to_string()],
        );

        let constraints = Some(AdditionalConstraints {
            required_fields: None,
            default_values: None,
            enum_values: Some(enum_map),
        });

        let result = apply_constraints(fields, constraints);
        assert_eq!(result[0].field_type, "enum");
        assert_eq!(
            result[0].enum_values,
            Some(vec!["ACTIVE".to_string(), "INACTIVE".to_string()])
        );
    }

    #[test]
    fn test_split_field_descriptions_comma() {
        let desc = "field1, field2, field3";
        let parts = split_field_descriptions(desc);
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_split_field_descriptions_and() {
        let desc = "field1 and field2 and field3";
        let parts = split_field_descriptions(desc);
        assert_eq!(parts.len(), 3);
    }

    #[test]
    fn test_split_field_descriptions_mixed() {
        let desc = "field1, field2 and field3; field4";
        let parts = split_field_descriptions(desc);
        assert_eq!(parts.len(), 4);
    }

    #[test]
    fn test_normalize_empty_field_name() {
        // Edge case: field name becomes empty after normalization
        let desc = "!@#$%^&*()";
        let normalized = normalize_field_name(desc);
        // Should return empty string for invalid input
        assert_eq!(normalized, "");
    }

    #[test]
    fn test_integration_full_schema_creation() {
        let desc = "invoice number (required), amount in USD, status (draft/sent/paid), due date";
        let fields = parse_field_descriptions(desc);
        let (normalized, _warnings) = normalize_inferred_fields(fields);

        // Stored names are bare — no namespace prefix is applied during inference
        assert!(normalized.iter().all(|f| !f.name.contains(':')));

        // Verify field names are present
        assert_eq!(normalized.len(), 4);
        assert!(normalized[0].name.contains("invoice"));
        assert!(normalized[1].name.contains("amount"));
        assert!(normalized[2].name.contains("status"));
        assert!(normalized[3].name.contains("due"));

        // Verify types are inferred correctly (following priority order)
        assert_eq!(normalized[0].field_type, "number"); // "number" keyword in "invoice number"
        assert_eq!(normalized[1].field_type, "number"); // amount in USD
        assert_eq!(normalized[2].field_type, "enum"); // "(options/separated/by/slashes)" pattern
        assert_eq!(normalized[3].field_type, "date"); // due date
    }

    #[test]
    fn test_integration_ambiguous_description() {
        let desc = "some field";
        let fields = parse_field_descriptions(desc);
        let (normalized, _warnings) = normalize_inferred_fields(fields);

        // Even with ambiguous description, should still create valid schema
        assert_eq!(normalized.len(), 1);
        assert_eq!(normalized[0].field_type, "string"); // Defaults to string
        assert_eq!(normalized[0].name, "some_field");
    }

    #[test]
    fn test_integration_edge_case_empty_enum_values() {
        let desc = "status ()";
        let fields = parse_field_descriptions(desc);

        // Should not create enum if no values found
        assert_eq!(fields[0].enum_values, None);
        assert_eq!(fields[0].field_type, "string");
    }

    #[test]
    fn test_integration_multiple_inferred_fields() {
        let desc = "customer name, total amount in USD, invoice date, is_paid (yes/no)";
        let fields = parse_field_descriptions(desc);

        assert_eq!(fields.len(), 4);
        assert_eq!(fields[0].field_type, "string");
        assert_eq!(fields[1].field_type, "number");
        assert_eq!(fields[2].field_type, "date");
        assert_eq!(fields[3].field_type, "boolean");
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

    #[test]
    fn test_integration_reserved_property_names() {
        // Fields matching core properties keep the caller's bare name and are
        // surfaced as warnings instead of being silently rewritten.
        let desc = "status, priority, due_date";
        let fields = parse_field_descriptions(desc);
        let (normalized, warnings) = normalize_inferred_fields(fields);

        assert_eq!(normalized[0].name, "status");
        assert_eq!(normalized[1].name, "priority");
        assert_eq!(normalized[2].name, "due_date");

        // Each collision is reported to the caller.
        for name in ["status", "priority", "due_date"] {
            assert!(
                warnings
                    .iter()
                    .any(|w| w.contains(name) && w.contains("reserved core property")),
                "Expected a reserved-core-property warning for '{name}', got {warnings:?}"
            );
        }
    }
}
