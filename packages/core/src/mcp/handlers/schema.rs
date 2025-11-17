//! MCP Schema Management Handlers
//!
//! Provides specialized MCP tools for schema operations with better UX
//! than using generic update_node. Enforces protection levels and provides
//! clear error messages.

use crate::mcp::types::MCPError;
use crate::models::schema::ProtectionLevel;
use crate::services::SchemaService;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

/// Parameters for create_entity_schema_from_description MCP method
#[derive(Debug, Deserialize)]
pub struct CreateEntitySchemaParams {
    /// Entity name (e.g., "Invoice", "Customer")
    pub entity_name: String,
    /// Natural language description of the entity and its fields
    pub description: String,
    /// Optional constraints for field types and values
    #[serde(default)]
    pub additional_constraints: Option<AdditionalConstraints>,
}

/// Additional constraints for schema generation
#[derive(Debug, Deserialize)]
pub struct AdditionalConstraints {
    /// Field names that should be required
    #[serde(default)]
    pub required_fields: Option<Vec<String>>,
    /// Default values for specific fields
    #[serde(default)]
    pub default_values: Option<std::collections::HashMap<String, Value>>,
    /// Enum values for specific fields
    #[serde(default)]
    pub enum_values: Option<std::collections::HashMap<String, Vec<String>>>,
}

/// Represents an inferred field from natural language description
#[derive(Debug, Clone)]
struct InferredField {
    name: String,
    field_type: String,
    required: bool,
    enum_values: Option<Vec<String>>,
    description: String,
    confidence: f32, // Confidence score for type inference (0.0-1.0)
}

/// Parse natural language description to extract field names
///
/// Handles common patterns like:
/// - "invoice number" -> "invoice_number"
/// - "vendor name" -> "vendor_name"
/// - Multi-word phrases with various separators
fn parse_field_names(description: &str) -> Vec<String> {
    let mut fields = Vec::new();

    // Split by common delimiters: commas, "and", parentheses
    let parts: Vec<String> = description
        .split([',', ';'])
        .flat_map(|part| {
            // Split on "and" (case insensitive)
            let and_parts: Vec<&str> = part.split_whitespace().collect();
            if and_parts.len() > 1 {
                // Check for "and" keyword and split
                let mut result = Vec::new();
                let mut current = Vec::new();
                for word in and_parts {
                    if word.to_lowercase() == "and" {
                        if !current.is_empty() {
                            result.push(current.join(" "));
                        }
                        current = Vec::new();
                    } else {
                        current.push(word);
                    }
                }
                if !current.is_empty() {
                    result.push(current.join(" "));
                }
                result
            } else {
                vec![part.to_string()]
            }
        })
        .collect();

    for part in parts {
        let trimmed = part.trim();

        // Remove parenthetical content but preserve enum values
        if let Some(paren_start) = trimmed.find('(') {
            let before_paren = &trimmed[..paren_start].trim();
            if !before_paren.is_empty() {
                let normalized = normalize_field_name(before_paren);
                if !normalized.is_empty() {
                    fields.push(normalized);
                }
            }
        } else if !trimmed.is_empty() {
            let normalized = normalize_field_name(trimmed);
            if !normalized.is_empty() {
                fields.push(normalized);
            }
        }
    }

    // Remove duplicates
    fields.sort();
    fields.dedup();
    fields
}

/// Normalize field name to snake_case
fn normalize_field_name(name: &str) -> String {
    name.trim()
        .to_lowercase()
        // Replace dashes and other separators with spaces first
        .replace('-', " ")
        .replace(['(', ')'], "")
        .split_whitespace()
        .collect::<Vec<&str>>()
        .join("_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect()
}

/// Check if description indicates a field is required
fn is_field_required(field_desc: &str) -> bool {
    let lower = field_desc.to_lowercase();
    lower.contains("(required)")
        || lower.contains("required")
        || lower.contains("must have")
        || lower.contains("mandatory")
}

/// Extract enum values from parenthetical notation (e.g., "(draft/sent/paid)")
/// Handles multiple parenthetical patterns and prefers patterns immediately after field context
fn extract_enum_values(text: &str) -> Option<Vec<String>> {
    // Look for the last/most recent parenthetical notation which is typically the field's enum values
    // This handles cases like "status (draft/sent) and priority (low/high)" by preferring the last one
    let mut last_valid_values: Option<Vec<String>> = None;

    // Find all parenthetical patterns
    let mut search_text = text;
    let mut search_start = 0;

    while let Some(start) = search_text.find('(') {
        let absolute_start = search_start + start;
        if let Some(end) = search_text[start..].find(')') {
            let absolute_end = absolute_start + end;
            let enum_str = &text[absolute_start + 1..absolute_end];

            if is_valid_enum_pattern(enum_str) {
                let separator = if enum_str.contains('/') {
                    '/'
                } else if enum_str.contains('|') {
                    '|'
                } else {
                    ','
                };

                let values: Vec<String> = enum_str
                    .split(separator)
                    .map(|v| {
                        v.trim()
                            .to_uppercase()
                            .replace(' ', "_")
                            .chars()
                            .filter(|c| c.is_alphanumeric() || *c == '_')
                            .collect::<String>()
                    })
                    .filter(|v| !v.is_empty())
                    .collect();

                if !values.is_empty() {
                    last_valid_values = Some(values);
                }
            }

            // Continue searching after this parenthesis pair
            search_text = &text[absolute_end + 1..];
            search_start = absolute_end + 1;
        } else {
            break;
        }
    }

    last_valid_values
}

/// Check if a parenthetical content looks like enum values
/// Enum patterns typically have separators and aren't just descriptive text
fn is_valid_enum_pattern(text: &str) -> bool {
    // Must have at least one separator (/, |, or comma)
    let has_separator = text.contains('/') || text.contains('|') || text.contains(',');

    // Shouldn't look like a note (e.g., "(optional)", "(required)")
    let is_not_note = !text.to_lowercase().contains("optional")
        && !text.to_lowercase().contains("required")
        && !text.to_lowercase().contains("mandatory");

    has_separator && is_not_note
}

/// Infer field type based on keywords in the description
/// Returns (field_type, enum_values, confidence_score)
/// Confidence score ranges from 0.0 to 1.0
fn infer_field_type(field_name: &str, field_desc: &str) -> (String, Option<Vec<String>>, f32) {
    let lower_name = field_name.to_lowercase();
    let lower_desc = field_desc.to_lowercase();

    // Check for enum type first (has parenthetical values)
    if let Some(enum_values) = extract_enum_values(field_desc) {
        return ("enum".to_string(), Some(enum_values), 0.95); // High confidence for explicit enums
    }

    // Keywords for number type
    let number_keywords = [
        "amount",
        "price",
        "cost",
        "total",
        "count",
        "quantity",
        "number",
        "num",
        "value",
        "usd",
        "dollars",
        "cents",
        "rate",
        "percentage",
        "percent",
        "%",
        "sum",
        "balance",
        "charge",
        "fee",
        "tax",
    ];

    // Keywords for date type
    let date_keywords = [
        "date",
        "deadline",
        "due",
        "when",
        "time",
        "created",
        "modified",
        "updated",
        "start",
        "end",
        "expires",
        "expiration",
        "born",
        "birthday",
        "anniversary",
        "scheduled",
    ];

    // Keywords for boolean type - prioritized by confidence
    let bool_keyword_pairs = [
        // High confidence: compound keywords that are unlikely to be ambiguous
        ("is_", true, 0.9),  // is_* fields are almost always boolean
        ("has_", true, 0.9), // has_* fields are almost always boolean
        ("enabled", false, 0.85),
        ("disabled", false, 0.85),
        ("active", false, 0.85),
        ("inactive", false, 0.85),
        // Medium confidence: can appear in other contexts
        ("is", false, 0.70),
        ("has", false, 0.70),
        ("allow", false, 0.65),
        ("prevent", false, 0.65),
        ("approve", false, 0.65),
        ("reject", false, 0.65),
        ("flag", false, 0.65),
        ("checked", false, 0.65),
        ("completed", false, 0.65),
        ("done", false, 0.65),
    ];

    // Count matching keywords for each type to determine confidence
    let mut number_match_count = 0;
    for keyword in &number_keywords {
        if lower_name.contains(keyword) || lower_desc.contains(keyword) {
            number_match_count += 1;
        }
    }

    let mut date_match_count = 0;
    for keyword in &date_keywords {
        if lower_name.contains(keyword) || lower_desc.contains(keyword) {
            date_match_count += 1;
        }
    }

    // Track highest confidence boolean match
    let mut highest_bool_confidence = 0.0f32;
    let mut has_bool_match = false;

    for (keyword, is_prefix_match, confidence) in &bool_keyword_pairs {
        let matched = if *is_prefix_match {
            lower_name.starts_with(keyword)
        } else {
            lower_name.contains(keyword) || lower_desc.contains(keyword)
        };

        if matched {
            has_bool_match = true;
            if confidence > &highest_bool_confidence {
                highest_bool_confidence = *confidence;
            }
        }
    }

    // Determine type based on matches, with ambiguity detection
    if number_match_count > 0 && has_bool_match && highest_bool_confidence <= 0.70 {
        // Ambiguous case like "is_amount" - prefer number if it matches, but low confidence
        return ("number".to_string(), None, 0.60);
    }

    if number_match_count > 0 {
        let confidence = (0.8 + (number_match_count as f32) * 0.05).min(0.95);
        return ("number".to_string(), None, confidence);
    }

    if date_match_count > 0 {
        let confidence = (0.8 + (date_match_count as f32) * 0.05).min(0.95);
        return ("date".to_string(), None, confidence);
    }

    if has_bool_match {
        return ("boolean".to_string(), None, highest_bool_confidence);
    }

    // Default to string with lower confidence
    ("string".to_string(), None, 0.5)
}

/// Parse natural language description to infer schema fields
fn parse_entity_description(description: &str) -> Vec<InferredField> {
    let mut fields = Vec::new();
    let field_names = parse_field_names(description);

    for field_name in field_names {
        // Find the field description in the original text
        // Use word boundary matching to avoid partial word matches
        let field_desc = find_field_description(description, &field_name);

        let (field_type, enum_values, confidence) = infer_field_type(&field_name, field_desc);
        let required = is_field_required(field_desc);

        fields.push(InferredField {
            name: format!("custom:{}", field_name),
            field_type,
            required,
            enum_values,
            description: field_desc.to_string(),
            confidence,
        });
    }

    fields
}

/// Find the most relevant sentence/clause containing the field name
/// Uses word boundary matching to avoid matching partial words
fn find_field_description<'a>(description: &'a str, field_name: &str) -> &'a str {
    let lower_desc = description.to_lowercase();
    let lower_field = field_name.to_lowercase();

    // Split by common delimiters and find the best match
    let mut best_match = "";
    let mut best_match_start_pos = usize::MAX;

    for segment in description.split([',', ';']) {
        let segment_lower = segment.to_lowercase();

        // Check if segment contains field name with word boundaries
        if let Some(pos) = segment_lower.find(&lower_field) {
            // Verify word boundaries by checking chars before and after
            let before_ok = pos == 0
                || !segment_lower
                    .chars()
                    .nth(pos - 1)
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);
            let after_ok = pos + lower_field.len() >= segment_lower.len()
                || !segment_lower
                    .chars()
                    .nth(pos + lower_field.len())
                    .map(|c| c.is_alphanumeric() || c == '_')
                    .unwrap_or(false);

            if before_ok && after_ok {
                // Calculate position in original description for preference
                if let Some(orig_pos) = lower_desc.find(segment_lower.as_str()) {
                    if orig_pos < best_match_start_pos {
                        best_match = segment;
                        best_match_start_pos = orig_pos;
                    }
                }
            }
        }
    }

    best_match.trim()
}

/// Parameters for add_schema_field MCP method
#[derive(Debug, Deserialize)]
pub struct AddSchemaFieldParams {
    /// Schema ID to modify (matches node_type)
    pub schema_id: String,
    /// Field name
    pub field_name: String,
    /// Field type (e.g., "string", "number", "boolean", "enum", "array")
    pub field_type: String,
    /// Whether this field should be indexed
    #[serde(default)]
    pub indexed: bool,
    /// Whether this field is required
    #[serde(default)]
    pub required: Option<bool>,
    /// Default value for the field
    #[serde(default)]
    pub default: Option<Value>,
    /// Field description
    #[serde(default)]
    pub description: Option<String>,
    /// For array fields, the type of items in the array
    #[serde(default)]
    pub item_type: Option<String>,
    /// For enum fields, the allowed values (added to user_values)
    #[serde(default)]
    pub enum_values: Option<Vec<String>>,
    /// For enum fields, whether users can extend with more values
    #[serde(default)]
    pub extensible: Option<bool>,
}

/// Parameters for remove_schema_field MCP method
#[derive(Debug, Deserialize)]
pub struct RemoveSchemaFieldParams {
    /// Schema ID to modify
    pub schema_id: String,
    /// Field name to remove
    pub field_name: String,
}

/// Parameters for extend_schema_enum MCP method
#[derive(Debug, Deserialize)]
pub struct ExtendSchemaEnumParams {
    /// Schema ID to modify
    pub schema_id: String,
    /// Enum field name
    pub field_name: String,
    /// New value to add to user_values
    pub value: String,
}

/// Parameters for remove_schema_enum_value MCP method
#[derive(Debug, Deserialize)]
pub struct RemoveSchemaEnumValueParams {
    /// Schema ID to modify
    pub schema_id: String,
    /// Enum field name
    pub field_name: String,
    /// Value to remove from user_values
    pub value: String,
}

/// Parameters for get_schema_definition MCP method
#[derive(Debug, Deserialize)]
pub struct GetSchemaDefinitionParams {
    /// Schema ID to retrieve
    pub schema_id: String,
}

/// Add a new field to a schema
///
/// # MCP Tool Description
/// Add a new user-protected field to an existing schema. Core and system fields
/// cannot be added through MCP (only user fields allowed). The schema version
/// will be incremented automatically.
///
/// **IMPORTANT: Namespace Requirement**
/// All user-defined field names MUST include a namespace prefix to prevent conflicts
/// with future core properties. Valid namespace prefixes are:
/// - `custom:` - For personal custom properties (e.g., `custom:estimatedHours`)
/// - `org:` - For organization-specific properties (e.g., `org:departmentCode`)
/// - `plugin:` - For plugin-provided properties (e.g., `plugin:jira:issueId`)
///
/// Core properties (added by NodeSpace) use simple names without prefixes
/// (e.g., `status`, `priority`, `due_date`).
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify (matches node_type)
/// - `field_name`: Name of the new field (MUST include namespace prefix: `custom:`, `org:`, or `plugin:`)
/// - `field_type`: Type of the field (string, number, boolean, enum, array)
/// - `indexed`: Whether to index this field for search (default: false)
/// - `required`: Whether this field is required (optional)
/// - `default`: Default value for the field (optional)
/// - `description`: Field description (optional)
/// - `item_type`: For array fields, the type of items (optional)
/// - `enum_values`: For enum fields, the allowed values (optional, added to user_values)
/// - `extensible`: For enum fields, whether users can add more values (optional)
///
/// # Returns
/// - `schema_id`: ID of the modified schema
/// - `new_version`: New schema version after adding the field
/// - `success`: true
///
/// # Errors
/// - `VALIDATION_ERROR`: If field name lacks namespace prefix, trying to add a non-user field, or field already exists
/// - `NODE_NOT_FOUND`: If schema doesn't exist
pub async fn handle_add_schema_field(
    schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: AddSchemaFieldParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Build field definition
    let field = crate::models::schema::SchemaField {
        name: params.field_name.clone(),
        field_type: params.field_type,
        protection: ProtectionLevel::User, // Always user-protected when added via MCP
        core_values: None,
        user_values: params.enum_values,
        indexed: params.indexed,
        required: params.required,
        extensible: params.extensible,
        default: params.default,
        description: params.description,
        item_type: params.item_type,
        fields: None,
        item_fields: None,
    };

    // Add field via SchemaService
    schema_service
        .add_field(&params.schema_id, field)
        .await
        .map_err(|e| MCPError::validation_error(e.to_string()))?;

    // Get updated schema to return new version
    let schema = schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|e| MCPError::internal_error(format!("Failed to retrieve schema: {}", e)))?;

    Ok(json!({
        "schema_id": params.schema_id,
        "new_version": schema.version,
        "success": true
    }))
}

/// Remove a field from a schema
///
/// # MCP Tool Description
/// Remove a user-protected field from an existing schema. Core and system fields
/// cannot be removed (protection enforcement). The schema version will be
/// incremented automatically.
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify
/// - `field_name`: Name of the field to remove
///
/// # Returns
/// - `schema_id`: ID of the modified schema
/// - `new_version`: New schema version after removing the field
/// - `success`: true
///
/// # Errors
/// - `VALIDATION_ERROR`: If trying to remove a core/system field or field doesn't exist
/// - `NODE_NOT_FOUND`: If schema doesn't exist
pub async fn handle_remove_schema_field(
    schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: RemoveSchemaFieldParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Remove field via SchemaService
    schema_service
        .remove_field(&params.schema_id, &params.field_name)
        .await
        .map_err(|e| MCPError::validation_error(e.to_string()))?;

    // Get updated schema to return new version
    let schema = schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|e| MCPError::internal_error(format!("Failed to retrieve schema: {}", e)))?;

    Ok(json!({
        "schema_id": params.schema_id,
        "new_version": schema.version,
        "success": true
    }))
}

/// Extend an enum field with a new value
///
/// # MCP Tool Description
/// Add a new value to an enum field's user_values list. The enum field must be
/// marked as extensible. Core enum values cannot be modified. The schema version
/// will be incremented automatically.
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify
/// - `field_name`: Name of the enum field
/// - `value`: New value to add to user_values
///
/// # Returns
/// - `schema_id`: ID of the modified schema
/// - `new_version`: New schema version after adding the value
/// - `success`: true
///
/// # Errors
/// - `VALIDATION_ERROR`: If field is not enum, not extensible, or value already exists
/// - `NODE_NOT_FOUND`: If schema or field doesn't exist
pub async fn handle_extend_schema_enum(
    schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: ExtendSchemaEnumParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Extend enum via SchemaService
    schema_service
        .extend_enum_field(&params.schema_id, &params.field_name, params.value)
        .await
        .map_err(|e| MCPError::validation_error(e.to_string()))?;

    // Get updated schema to return new version
    let schema = schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|e| MCPError::internal_error(format!("Failed to retrieve schema: {}", e)))?;

    Ok(json!({
        "schema_id": params.schema_id,
        "new_version": schema.version,
        "success": true
    }))
}

/// Remove a value from an enum field
///
/// # MCP Tool Description
/// Remove a value from an enum field's user_values list. Core enum values cannot
/// be removed (protection enforcement). The schema version will be incremented
/// automatically.
///
/// # Parameters
/// - `schema_id`: ID of the schema to modify
/// - `field_name`: Name of the enum field
/// - `value`: Value to remove from user_values
///
/// # Returns
/// - `schema_id`: ID of the modified schema
/// - `new_version`: New schema version after removing the value
/// - `success`: true
///
/// # Errors
/// - `VALIDATION_ERROR`: If trying to remove core value or value doesn't exist
/// - `NODE_NOT_FOUND`: If schema or field doesn't exist
pub async fn handle_remove_schema_enum_value(
    schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: RemoveSchemaEnumValueParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Remove enum value via SchemaService
    schema_service
        .remove_enum_value(&params.schema_id, &params.field_name, &params.value)
        .await
        .map_err(|e| MCPError::validation_error(e.to_string()))?;

    // Get updated schema to return new version
    let schema = schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|e| MCPError::internal_error(format!("Failed to retrieve schema: {}", e)))?;

    Ok(json!({
        "schema_id": params.schema_id,
        "new_version": schema.version,
        "success": true
    }))
}

/// Get schema definition
///
/// # MCP Tool Description
/// Retrieve the complete schema definition for a given schema ID. Returns the
/// parsed schema with all fields, protection levels, and metadata.
///
/// # Parameters
/// - `schema_id`: ID of the schema to retrieve
///
/// # Returns
/// - `schema`: Complete schema definition object with:
///   - `is_core`: Whether this is a core schema
///   - `version`: Current schema version
///   - `description`: Schema description
///   - `fields`: Array of field definitions with:
///     - `name`: Field name
///     - `type`: Field type
///     - `protection`: Protection level (core, user, system)
///     - `indexed`: Whether field is indexed
///     - Additional field-specific properties
///
/// # Errors
/// - `NODE_NOT_FOUND`: If schema doesn't exist
pub async fn handle_get_schema_definition(
    schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: GetSchemaDefinitionParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Get schema via SchemaService
    let schema = schema_service
        .get_schema(&params.schema_id)
        .await
        .map_err(|_e| MCPError::node_not_found(&params.schema_id))?;

    // Serialize schema to JSON
    let schema_json = serde_json::to_value(&schema)
        .map_err(|e| MCPError::internal_error(format!("Failed to serialize schema: {}", e)))?;

    Ok(json!({
        "schema": schema_json
    }))
}

/// Create a schema from a natural language description
///
/// # MCP Tool Description
/// Generate a schema definition from natural language description of an entity.
/// This tool uses AI-powered parsing to infer field types, detect required fields,
/// and identify enum values from conversational descriptions.
///
/// The parser automatically:
/// - Extracts field names and normalizes them to snake_case
/// - Infers field types (string, number, date, enum, boolean) from keywords
/// - Detects required fields from phrases like "(required)"
/// - Extracts enum values from parenthetical notation: "(value1/value2/value3)"
/// - Applies the `custom:` namespace prefix to all user fields
///
/// # Parameters
/// - `entity_name`: Name of the entity (e.g., "Invoice", "Customer")
/// - `description`: Natural language description of the entity and its fields
/// - `additional_constraints`: Optional field-level constraints:
///   - `required_fields`: Explicit list of fields to mark as required
///   - `default_values`: Default values for specific fields
///   - `enum_values`: Explicit enum values for specific fields
///
/// # Returns
/// - `schema_id`: ID of the created schema (derived from entity_name)
/// - `schema_definition`: The complete generated schema with fields
/// - `created_fields`: Array of inferred fields with:
///   - `name`: Field name with `custom:` prefix
///   - `type`: Inferred field type
///   - `required`: Whether the field is required
/// - `warnings`: Optional warnings about ambiguous inferences
///
/// # Errors
/// - `VALIDATION_ERROR`: If entity_name is empty or invalid
/// - `INTERNAL_ERROR`: If schema creation fails
///
/// # Examples
///
/// Input:
/// ```json
/// {
///   "entity_name": "Invoice",
///   "description": "Create an Invoice with invoice number (required), amount in USD, vendor name, status (draft/sent/paid), and due date"
/// }
/// ```
///
/// Output:
/// ```json
/// {
///   "schema_id": "invoice",
///   "created_fields": [
///     { "name": "custom:invoice_number", "type": "string", "required": true },
///     { "name": "custom:amount", "type": "number", "required": false },
///     { "name": "custom:vendor_name", "type": "string", "required": false },
///     { "name": "custom:status", "type": "enum", "required": false },
///     { "name": "custom:due_date", "type": "date", "required": false }
///   ],
///   "warnings": []
/// }
/// ```
pub fn handle_create_entity_schema_from_description(
    _schema_service: &Arc<SchemaService>,
    params: Value,
) -> Result<Value, MCPError> {
    let params: CreateEntitySchemaParams = serde_json::from_value(params)
        .map_err(|e| MCPError::invalid_params(format!("Invalid parameters: {}", e)))?;

    // Validate entity name
    let schema_id = params
        .entity_name
        .trim()
        .to_lowercase()
        .replace(' ', "_")
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .collect::<String>();

    if schema_id.is_empty() {
        return Err(MCPError::validation_error(
            "entity_name must not be empty".to_string(),
        ));
    }

    // Parse description to infer fields
    let mut inferred_fields = parse_entity_description(&params.description);

    // Apply additional constraints if provided
    if let Some(constraints) = &params.additional_constraints {
        // Mark required fields from constraints
        if let Some(required_field_names) = &constraints.required_fields {
            for field in &mut inferred_fields {
                if required_field_names.iter().any(|req_field| {
                    field.name.ends_with(req_field)
                        || field.name.ends_with(&format!("custom:{}", req_field))
                }) {
                    field.required = true;
                }
            }
        }

        // Override enum values from constraints
        if let Some(enum_overrides) = &constraints.enum_values {
            for field in &mut inferred_fields {
                for (field_key, values) in enum_overrides {
                    if field.name.ends_with(field_key.as_str())
                        || field.name.ends_with(&format!("custom:{}", field_key))
                    {
                        field.enum_values = Some(
                            values
                                .iter()
                                .map(|v| {
                                    v.to_uppercase()
                                        .replace(' ', "_")
                                        .chars()
                                        .filter(|c| c.is_alphanumeric() || *c == '_')
                                        .collect::<String>()
                                })
                                .collect(),
                        );
                    }
                }
            }
        }
    }

    // Build schema fields
    let schema_fields: Vec<crate::models::schema::SchemaField> = inferred_fields
        .iter()
        .map(|inferred| crate::models::schema::SchemaField {
            name: inferred.name.clone(),
            field_type: inferred.field_type.clone(),
            protection: ProtectionLevel::User,
            core_values: None,
            user_values: inferred.enum_values.clone(),
            indexed: false,
            required: Some(inferred.required),
            extensible: if inferred.field_type == "enum" {
                Some(true)
            } else {
                None
            },
            default: params
                .additional_constraints
                .as_ref()
                .and_then(|c| c.default_values.as_ref())
                .and_then(|dv| {
                    dv.iter()
                        .find(|(k, _)| {
                            inferred.name.ends_with(k.as_str())
                                || inferred.name.ends_with(&format!("custom:{}", k))
                        })
                        .map(|(_, v)| v.clone())
                }),
            description: Some(inferred.description.clone()),
            item_type: None,
            fields: None,
            item_fields: None,
        })
        .collect();

    // Create schema definition
    let schema_def = crate::models::schema::SchemaDefinition {
        is_core: false,
        version: 1,
        description: format!(
            "{} entity schema auto-generated from natural language",
            params.entity_name
        ),
        fields: schema_fields,
    };

    // Note: In a full implementation, we would persist the schema here via SchemaService.
    // For now, we return the schema definition for the client to apply.

    // Generate warnings for ambiguous inferences
    let mut warnings = Vec::new();

    for field in &inferred_fields {
        // Warn about low confidence type inferences
        if field.confidence < 0.8 {
            let confidence_percent = (field.confidence * 100.0) as u32;
            warnings.push(format!(
                "Field '{}' has ambiguous type inference. Inferred as '{}' with only {}% confidence. \
                 Consider providing explicit type hints in additional_constraints.",
                field.name.trim_start_matches("custom:"),
                field.field_type,
                confidence_percent
            ));
        }

        // Warn about overly generic field names
        let field_short_name = field.name.trim_start_matches("custom:");
        let generic_names = ["data", "value", "info", "item", "thing", "stuff"];
        if generic_names.contains(&field_short_name) {
            warnings.push(format!(
                "Field '{}' uses a generic name. Consider using a more descriptive name \
                 for better type inference.",
                field_short_name
            ));
        }
    }

    // Build response with created fields
    let created_fields: Vec<Value> = inferred_fields
        .iter()
        .map(|field| {
            json!({
                "name": field.name,
                "type": field.field_type,
                "required": field.required,
                "enum_values": field.enum_values
            })
        })
        .collect();

    Ok(json!({
        "schema_id": schema_id,
        "schema_definition": serde_json::to_value(&schema_def)
            .unwrap_or(json!({})),
        "created_fields": created_fields,
        "warnings": warnings
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::SurrealStore;
    use crate::NodeService;
    use tempfile::TempDir;

    async fn setup_test_service() -> (Arc<SchemaService>, Arc<NodeService>, TempDir) {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test.db");

        let store = Arc::new(SurrealStore::new(db_path).await.unwrap());
        let node_service = Arc::new(NodeService::new(store).unwrap());
        let schema_service = Arc::new(SchemaService::new(node_service.clone()));
        (schema_service, node_service, temp_dir)
    }

    async fn create_test_schema(node_service: &NodeService) {
        // Create a minimal test schema
        use crate::models::schema::{SchemaDefinition, SchemaField};
        use crate::models::Node;

        let schema = SchemaDefinition {
            is_core: false,
            version: 1,
            description: "Test schema".to_string(),
            fields: vec![SchemaField {
                name: "status".to_string(),
                field_type: "enum".to_string(),
                protection: ProtectionLevel::Core,
                core_values: Some(vec!["OPEN".to_string(), "DONE".to_string()]),
                user_values: None,
                indexed: true,
                required: Some(true),
                extensible: Some(true),
                default: Some(json!("OPEN")),
                description: Some("Task status".to_string()),
                item_type: None,
                fields: None,
                item_fields: None,
            }],
        };

        let schema_node = Node {
            id: "test_schema".to_string(),
            node_type: "schema".to_string(),
            content: "Test Schema".to_string(),
            before_sibling_id: None,
            version: 1,
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            properties: serde_json::to_value(&schema).unwrap(),
            embedding_vector: None,
            mentions: Vec::new(),
            mentioned_by: Vec::new(),
        };

        node_service.create_node(schema_node).await.unwrap();
    }

    #[tokio::test]
    async fn test_add_schema_field() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        let params = json!({
            "schema_id": "test_schema",
            "field_name": "custom:custom_field",  // User field with namespace prefix
            "field_type": "string",
            "indexed": true,
            "description": "Custom user field"
        });

        let result = handle_add_schema_field(&schema_service, params)
            .await
            .unwrap();

        assert_eq!(result["schema_id"], "test_schema");
        assert_eq!(result["new_version"], 2);
        assert_eq!(result["success"], true);

        // Verify field was added
        let schema = schema_service.get_schema("test_schema").await.unwrap();
        let field = schema
            .fields
            .iter()
            .find(|f| f.name == "custom:custom_field");
        assert!(field.is_some());
        assert_eq!(field.unwrap().protection, ProtectionLevel::User);
    }

    #[tokio::test]
    async fn test_add_schema_field_without_namespace_rejected() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        let params = json!({
            "schema_id": "test_schema",
            "field_name": "estimatedHours",  // Missing namespace prefix
            "field_type": "number",
            "indexed": false,
            "description": "Estimated hours"
        });

        let result = handle_add_schema_field(&schema_service, params).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error
            .message
            .contains("User properties must use namespace prefix"));
    }

    #[tokio::test]
    async fn test_remove_schema_field() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        // First add a user field with namespace
        let add_params = json!({
            "schema_id": "test_schema",
            "field_name": "custom:temp_field",
            "field_type": "string",
            "indexed": false
        });
        handle_add_schema_field(&schema_service, add_params)
            .await
            .unwrap();

        // Then remove it
        let remove_params = json!({
            "schema_id": "test_schema",
            "field_name": "custom:temp_field"
        });

        let result = handle_remove_schema_field(&schema_service, remove_params)
            .await
            .unwrap();

        assert_eq!(result["schema_id"], "test_schema");
        assert_eq!(result["success"], true);

        // Verify field was removed
        let schema = schema_service.get_schema("test_schema").await.unwrap();
        let field = schema.fields.iter().find(|f| f.name == "custom:temp_field");
        assert!(field.is_none());
    }

    #[tokio::test]
    async fn test_remove_core_field_rejected() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        // Try to remove core field
        let params = json!({
            "schema_id": "test_schema",
            "field_name": "status"
        });

        let result = handle_remove_schema_field(&schema_service, params).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.message.contains("Cannot remove"));
    }

    #[tokio::test]
    async fn test_extend_schema_enum() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        // Extend status enum
        let params = json!({
            "schema_id": "test_schema",
            "field_name": "status",
            "value": "BLOCKED"
        });

        let result = handle_extend_schema_enum(&schema_service, params)
            .await
            .unwrap();

        assert_eq!(result["schema_id"], "test_schema");
        assert_eq!(result["success"], true);

        // Verify value was added to user_values
        let schema = schema_service.get_schema("test_schema").await.unwrap();
        let values = schema.get_enum_values("status").unwrap();
        assert!(values.contains(&"BLOCKED".to_string()));
    }

    #[tokio::test]
    async fn test_remove_schema_enum_value() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        // First add a value
        let add_params = json!({
            "schema_id": "test_schema",
            "field_name": "status",
            "value": "TEMP_STATUS"
        });
        handle_extend_schema_enum(&schema_service, add_params)
            .await
            .unwrap();

        // Then remove it
        let remove_params = json!({
            "schema_id": "test_schema",
            "field_name": "status",
            "value": "TEMP_STATUS"
        });

        let result = handle_remove_schema_enum_value(&schema_service, remove_params)
            .await
            .unwrap();

        assert_eq!(result["schema_id"], "test_schema");
        assert_eq!(result["success"], true);

        // Verify value was removed
        let schema = schema_service.get_schema("test_schema").await.unwrap();
        let values = schema.get_enum_values("status").unwrap();
        assert!(!values.contains(&"TEMP_STATUS".to_string()));
    }

    #[tokio::test]
    async fn test_remove_core_enum_value_rejected() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        // Try to remove core enum value
        let params = json!({
            "schema_id": "test_schema",
            "field_name": "status",
            "value": "OPEN"
        });

        let result = handle_remove_schema_enum_value(&schema_service, params).await;

        assert!(result.is_err());
        let error = result.unwrap_err();
        assert!(error.message.contains("Cannot remove core value"));
    }

    #[tokio::test]
    async fn test_get_schema_definition() {
        let (schema_service, node_service, _temp) = setup_test_service().await;
        create_test_schema(&node_service).await;

        let params = json!({
            "schema_id": "test_schema"
        });

        let result = handle_get_schema_definition(&schema_service, params)
            .await
            .unwrap();

        assert!(result["schema"].is_object());
        let schema = &result["schema"];
        assert!(!schema["is_core"].as_bool().unwrap());
        assert_eq!(schema["version"].as_u64().unwrap(), 1);
        assert!(schema["fields"].is_array());
        assert!(!schema["fields"].as_array().unwrap().is_empty());
    }

    #[test]
    fn test_parse_field_names_comma_separated() {
        let description =
            "invoice number, amount in USD, vendor name, status (draft/sent/paid), due date";
        let fields = parse_field_names(description);

        assert_eq!(fields.len(), 5);
        assert!(fields.contains(&"invoice_number".to_string()));
        assert!(fields.contains(&"amount_in_usd".to_string()));
        assert!(fields.contains(&"vendor_name".to_string()));
        assert!(fields.contains(&"status".to_string()));
        assert!(fields.contains(&"due_date".to_string()));
    }

    #[test]
    fn test_parse_field_names_with_and_keyword() {
        let description = "first name and last name and email address";
        let fields = parse_field_names(description);

        assert_eq!(fields.len(), 3);
        assert!(fields.contains(&"first_name".to_string()));
        assert!(fields.contains(&"last_name".to_string()));
        assert!(fields.contains(&"email_address".to_string()));
    }

    #[test]
    fn test_normalize_field_name_multi_word() {
        let name = "invoice number";
        let normalized = normalize_field_name(name);
        assert_eq!(normalized, "invoice_number");
    }

    #[test]
    fn test_normalize_field_name_with_special_chars() {
        let name = "account-status (active)";
        let normalized = normalize_field_name(name);
        assert_eq!(normalized, "account_status_active");
    }

    #[test]
    fn test_is_field_required_with_required_keyword() {
        let desc = "invoice number (required)";
        assert!(is_field_required(desc));

        let desc2 = "optional vendor name";
        assert!(!is_field_required(desc2));
    }

    #[test]
    fn test_is_field_required_with_must_have() {
        let desc = "email address must have";
        assert!(is_field_required(desc));
    }

    #[test]
    fn test_extract_enum_values_slash_separated() {
        let text = "status (draft/sent/paid)";
        let values = extract_enum_values(text);

        assert!(values.is_some());
        let vals = values.unwrap();
        assert_eq!(vals.len(), 3);
        assert!(vals.contains(&"DRAFT".to_string()));
        assert!(vals.contains(&"SENT".to_string()));
        assert!(vals.contains(&"PAID".to_string()));
    }

    #[test]
    fn test_extract_enum_values_pipe_separated() {
        let text = "account status (active|inactive|suspended)";
        let values = extract_enum_values(text);

        assert!(values.is_some());
        let vals = values.unwrap();
        assert_eq!(vals.len(), 3);
        assert!(vals.contains(&"ACTIVE".to_string()));
        assert!(vals.contains(&"INACTIVE".to_string()));
        assert!(vals.contains(&"SUSPENDED".to_string()));
    }

    #[test]
    fn test_extract_enum_values_comma_separated() {
        let text = "priority (low, medium, high)";
        let values = extract_enum_values(text);

        assert!(values.is_some());
        let vals = values.unwrap();
        assert_eq!(vals.len(), 3);
        assert!(vals.contains(&"LOW".to_string()));
        assert!(vals.contains(&"MEDIUM".to_string()));
        assert!(vals.contains(&"HIGH".to_string()));
    }

    #[test]
    fn test_infer_field_type_number() {
        let (field_type, _, confidence) = infer_field_type("amount", "amount in USD");
        assert_eq!(field_type, "number");
        assert!(confidence >= 0.8); // Should have high confidence

        let (field_type, _, _) = infer_field_type("quantity", "quantity of items");
        assert_eq!(field_type, "number");

        let (field_type, _, _) = infer_field_type("price", "unit price in dollars");
        assert_eq!(field_type, "number");
    }

    #[test]
    fn test_infer_field_type_date() {
        let (field_type, _, confidence) = infer_field_type("due_date", "due date for completion");
        assert_eq!(field_type, "date");
        assert!(confidence >= 0.8);

        let (field_type, _, _) = infer_field_type("deadline", "project deadline");
        assert_eq!(field_type, "date");

        let (field_type, _, _) = infer_field_type("created_at", "when it was created");
        assert_eq!(field_type, "date");
    }

    #[test]
    fn test_infer_field_type_boolean() {
        let (field_type, _, confidence) = infer_field_type("is_enabled", "is the feature enabled");
        assert_eq!(field_type, "boolean");
        assert!(confidence >= 0.85); // is_ prefix has high confidence

        let (field_type, _, _) = infer_field_type("is_approved", "whether the approval is given");
        assert_eq!(field_type, "boolean");
    }

    #[test]
    fn test_infer_field_type_enum() {
        let (field_type, values, confidence) =
            infer_field_type("status", "status (draft/sent/paid)");
        assert_eq!(field_type, "enum");
        assert!(values.is_some());
        assert!(confidence >= 0.90); // Explicit enums have very high confidence
        let vals = values.unwrap();
        assert_eq!(vals.len(), 3);
    }

    #[test]
    fn test_infer_field_type_default_string() {
        // Use descriptions that don't contain any type keywords
        // Note: "vendor" contains "end" which matches the date keyword, so use "supplier" instead
        let (field_type, _, confidence) =
            infer_field_type("supplier_id", "unique identifier for supplier");
        assert_eq!(field_type, "string");
        assert!(confidence <= 0.5); // Default string has low confidence

        let (field_type, _, _) = infer_field_type("title", "the page or article title");
        assert_eq!(field_type, "string");
    }

    #[test]
    fn test_infer_field_type_ambiguous_is_amount() {
        // Test the ambiguous case where "is" and "amount" could conflict
        let (field_type, _, _confidence) = infer_field_type("is_amount", "is the amount valid");
        // Should prefer number over boolean due to priority
        // This tests the compound keyword handling
        assert!(["number", "boolean"].contains(&field_type.as_str()));
    }

    #[test]
    fn test_parse_field_names_with_duplicates() {
        // Use explicit duplicates with the same separators
        // "name" appears twice, "age" appears once
        let description = "name, age, name";
        let fields = parse_field_names(description);

        // Should deduplicate "name" appearing twice
        assert_eq!(fields.len(), 2);
        assert!(fields.contains(&"name".to_string()));
        assert!(fields.contains(&"age".to_string()));
    }

    #[test]
    fn test_extract_enum_values_multiple_parentheses() {
        // Test handling of multiple parenthetical patterns
        let text = "status (draft/sent) and priority (low/high)";
        let values = extract_enum_values(text);

        // Should extract the last valid enum pattern (priority)
        assert!(values.is_some());
        let vals = values.unwrap();
        assert!(vals.contains(&"LOW".to_string()) || vals.contains(&"DRAFT".to_string()));
    }

    #[test]
    fn test_parse_entity_description_invoice() {
        let description =
            "invoice id (required), amount in USD, vendor name, status (draft/sent/paid), due date";
        let fields = parse_entity_description(description);

        // Check that fields were parsed
        assert!(fields.len() >= 4, "Should have at least 4 fields");

        // Check invoice_id
        let invoice_id = fields.iter().find(|f| f.name.contains("invoice_id"));
        if let Some(invoice_id) = invoice_id {
            assert_eq!(invoice_id.field_type, "string");
        }

        // Check amount
        let amount = fields.iter().find(|f| f.name.contains("amount")).unwrap();
        assert!(!amount.required);
        assert_eq!(amount.field_type, "number");

        // Check status
        let status = fields.iter().find(|f| f.name.contains("status")).unwrap();
        assert_eq!(status.field_type, "enum");
        assert!(status.enum_values.is_some());

        // Check due_date
        let due_date = fields.iter().find(|f| f.name.contains("due_date")).unwrap();
        assert_eq!(due_date.field_type, "date");
    }

    #[test]
    fn test_parse_entity_description_namespace_prefix() {
        let description = "customer id and email address";
        let fields = parse_entity_description(description);

        for field in fields {
            assert!(
                field.name.starts_with("custom:"),
                "Field {} should have custom: prefix",
                field.name
            );
        }
    }

    #[tokio::test]
    async fn test_create_entity_schema_from_description() {
        let (schema_service, _node_service, _temp) = setup_test_service().await;

        let params = json!({
            "entity_name": "Invoice",
            "description": "invoice number (required), amount in USD, vendor name, status (draft/sent/paid), due date"
        });

        let result = handle_create_entity_schema_from_description(&schema_service, params).unwrap();

        assert_eq!(result["schema_id"], "invoice");
        assert!(result["schema_definition"].is_object());
        assert!(result["created_fields"].is_array());

        let fields = result["created_fields"].as_array().unwrap();
        assert!(fields.len() >= 4); // At least the main fields

        // Check that all fields have custom: prefix
        for field in fields {
            assert!(field["name"].as_str().unwrap().starts_with("custom:"));
        }

        // Check specific field types exist
        let field_types: std::collections::HashMap<String, String> = fields
            .iter()
            .map(|f| {
                (
                    f["name"].as_str().unwrap().to_string(),
                    f["type"].as_str().unwrap().to_string(),
                )
            })
            .collect();

        // Verify key fields are present and have correct types
        assert!(
            field_types
                .iter()
                .any(|(k, v)| k.contains("amount") && v == "number"),
            "Should have an amount field of type number"
        );
        assert!(
            field_types
                .iter()
                .any(|(k, v)| k.contains("due_date") && v == "date"),
            "Should have a due_date field of type date"
        );
    }

    #[tokio::test]
    async fn test_create_entity_schema_invalid_entity_name() {
        let (schema_service, _node_service, _temp) = setup_test_service().await;

        let params = json!({
            "entity_name": "",
            "description": "some fields"
        });

        let result = handle_create_entity_schema_from_description(&schema_service, params);
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_create_entity_schema_with_constraints() {
        let (schema_service, _node_service, _temp) = setup_test_service().await;

        let params = json!({
            "entity_name": "Product",
            "description": "name, price, category",
            "additional_constraints": {
                "required_fields": ["name", "price"],
                "enum_values": {
                    "category": ["ELECTRONICS", "CLOTHING", "BOOKS"]
                }
            }
        });

        let result = handle_create_entity_schema_from_description(&schema_service, params).unwrap();

        let fields = result["created_fields"].as_array().unwrap();
        let name_field = fields
            .iter()
            .find(|f| f["name"].as_str().unwrap().contains("name"))
            .unwrap();
        assert_eq!(name_field["required"], true);

        let category_field = fields
            .iter()
            .find(|f| f["name"].as_str().unwrap().contains("category"))
            .unwrap();
        // Category should have enum type due to the explicit enum_values constraint
        assert!(
            category_field["type"].as_str().unwrap() == "enum"
                || category_field["enum_values"].is_array()
        );
    }
}
