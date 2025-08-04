# Entity Management Specification

## Overview

The Entity Management system provides structured data capabilities within NodeSpace, allowing users to create custom entity types (Employee, Customer, Project, etc.) with sophisticated field management, calculated properties, and AI-native interactions. This system bridges traditional database functionality with the hierarchical node interface.

## Core Architecture

### EntityNode Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNode {
    pub base: TextNode,                                    // Inherits hierarchical capabilities
    pub entity_type: String,                               // "employee", "customer", "project"
    pub stored_fields: HashMap<String, EntityValue>,       // Actual field data
    pub calculated_fields: HashMap<String, CalculatedField>, // Computed properties
    pub schema: EntitySchema,                              // Field definitions and rules
    pub last_calculated: DateTime<Utc>,                    // Cache invalidation timestamp
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    pub name: String,                                      // "Employee Management"
    pub description: String,                               // User-friendly description
    pub stored_fields: Vec<FieldDefinition>,               // Database-backed fields
    pub calculated_fields: Vec<CalculatedFieldDefinition>, // Formula-based fields
    pub validation_rules: Vec<ValidationRule>,             // Business rules
    pub display_settings: DisplaySettings,                // UI configuration
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldDefinition {
    pub name: String,                                      // "first_name", "salary"
    pub display_name: String,                              // "First Name", "Annual Salary"
    pub field_type: FieldType,
    pub description: String,                               // Help text for users
    pub is_required: bool,
    pub default_value: Option<EntityValue>,
    pub constraints: Vec<FieldConstraint>,                 // Min/max, length, etc.
    pub ui_hints: UIHints,                                 // Display preferences
}
```

### Field Type System

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    // Basic types
    Text { max_length: Option<usize> },
    Number { decimal_places: Option<u8> },
    Integer,
    Boolean,
    Date,
    DateTime,
    
    // Specialized types  
    Email,
    Phone,
    Currency { currency_code: String },
    Percentage,
    URL,
    
    // Complex types
    Reference { 
        target_entity: String,                             // Points to another entity type
        cascade_delete: bool,                              // Delete behavior
    },
    List { 
        item_type: Box<FieldType>,                         // Array of any field type
        max_items: Option<usize>,
    },
    Enum { 
        options: Vec<EnumOption>,                          // Predefined choices
        allow_custom: bool,                                // Can users add options?
    },
    
    // Rich content
    RichText,                                              // Markdown with formatting
    File { 
        allowed_extensions: Vec<String>,                   // ["pdf", "docx"]
        max_size_mb: Option<f32>,
    },
    Image { 
        max_width: Option<u32>,
        max_height: Option<u32>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntityValue {
    Text(String),
    Number(f64),
    Integer(i64),
    Boolean(bool),
    Date(DateTime<Utc>),
    Email(String),
    Phone(String),
    Currency { amount: f64, currency: String },
    Percentage(f64),
    URL(String),
    Reference(String),                                     // Node ID of referenced entity
    List(Vec<EntityValue>),
    Enum(String),
    RichText(String),                                      // Markdown content
    File { path: String, name: String, size: u64 },
    Image { path: String, width: u32, height: u32 },
    Null,
}
```

## Calculated Fields System

### Formula-Based Computed Properties

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculatedField {
    pub name: String,                                      // "full_name", "total_compensation"
    pub display_name: String,                              // "Full Name", "Total Compensation"
    pub formula: String,                                   // "first_name + ' ' + last_name"
    pub return_type: FieldType,
    pub dependencies: Vec<String>,                         // Fields this calculation depends on
    pub description: String,                               // What this field represents
    pub cache_value: Option<EntityValue>,                  // Cached result
    pub cache_valid: bool,                                 // Is cache still valid?
    pub last_calculated: DateTime<Utc>,
    pub calculation_order: u32,                            // Dependency resolution order
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculatedFieldDefinition {
    pub field: CalculatedField,
    pub ui_hints: UIHints,                                 // Display settings
    pub update_triggers: Vec<UpdateTrigger>,               // When to recalculate
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum UpdateTrigger {
    FieldChange(String),                                   // When specific field changes
    AnyFieldChange,                                        // When any field changes
    TimeInterval(Duration),                                // Periodic updates (e.g., age)
    External(String),                                      // External system updates
}
```

### Formula Engine Integration

```rust
pub struct EntityCalculationEngine {
    formula_engine: FormulaEngine,
    dependency_graph: DependencyGraph,
    calculation_cache: HashMap<String, CalculationResult>,
}

impl EntityCalculationEngine {
    pub async fn recalculate_entity(
        &mut self,
        entity: &mut EntityNode,
        changed_fields: &[String]
    ) -> Result<Vec<FieldUpdate>, Error> {
        // 1. Find affected calculated fields
        let affected_fields = self.dependency_graph
            .get_dependent_fields(changed_fields);
        
        // 2. Calculate in dependency order
        let calculation_order = self.dependency_graph
            .topological_sort(&affected_fields)?;
        
        let mut updates = Vec::new();
        
        for field_name in calculation_order {
            let calc_field = entity.schema.get_calculated_field(&field_name)?;
            
            // 3. Evaluate formula with current field values
            let field_values = entity.get_all_field_values()?;
            let new_value = self.formula_engine.evaluate(
                &calc_field.formula,
                &field_values
            )?;
            
            // 4. Update if value changed
            let old_value = entity.calculated_fields
                .get(&field_name)
                .and_then(|f| f.cache_value.clone());
                
            if old_value.as_ref() != Some(&new_value) {
                entity.update_calculated_field(&field_name, new_value.clone());
                
                updates.push(FieldUpdate {
                    field_name: field_name.clone(),
                    old_value,
                    new_value,
                    calculation_time: Utc::now(),
                });
            }
        }
        
        Ok(updates)
    }
    
    pub fn validate_formula(
        &self,
        formula: &str,
        available_fields: &[FieldDefinition]
    ) -> Result<FormulaValidation, Error> {
        // Parse formula and check field references
        let field_refs = self.extract_field_references(formula)?;
        
        let mut issues = Vec::new();
        
        for field_ref in &field_refs {
            if !available_fields.iter().any(|f| f.name == *field_ref) {
                issues.push(ValidationIssue::UnknownField {
                    field_name: field_ref.clone(),
                });
            }
        }
        
        // Test formula compilation
        match self.formula_engine.compile(formula) {
            Ok(_) => Ok(FormulaValidation {
                is_valid: issues.is_empty(),
                dependencies: field_refs,
                issues,
            }),
            Err(e) => Ok(FormulaValidation {
                is_valid: false,
                dependencies: field_refs,
                issues: vec![ValidationIssue::SyntaxError(e.to_string())],
            }),
        }
    }
}
```

### Formula Examples

```rust
// Common calculated field patterns:

// String concatenation
"full_name" = "first_name + ' ' + last_name"

// Numeric calculations  
"total_compensation" = "salary + bonus + stock_value"
"monthly_salary" = "salary / 12"

// Date calculations
"years_employed" = "ROUND(DAYS(start_date, TODAY()) / 365.25, 1)"
"days_until_review" = "DAYS(TODAY(), next_review_date)"

// Conditional logic
"status" = "IF(active, 'Active', 'Inactive')"
"seniority_level" = "IF(years_employed >= 10, 'Senior', IF(years_employed >= 5, 'Mid', 'Junior'))"

// Complex business logic
"overtime_eligible" = "employment_type == 'Full-time' AND salary < 50000"
"manager_status" = "IS_MANAGER(role) AND team_size > 0"

// Aggregations (when referencing related entities)
"direct_reports_count" = "COUNT(employees WHERE manager_id == this.id)"
"team_total_salary" = "SUM(employees.salary WHERE manager_id == this.id)"

// Currency and formatting
"salary_formatted" = "FORMAT_CURRENCY(salary, 'USD')"
"completion_percentage" = "ROUND((completed_tasks / total_tasks) * 100, 1) + '%'"
```

## Entity Schema Management

### Schema Creation and Evolution

```rust
pub struct EntitySchemaManager {
    schemas: HashMap<String, EntitySchema>,
    schema_versions: HashMap<String, Vec<SchemaVersion>>,
    migration_engine: SchemaMigrationEngine,
}

impl EntitySchemaManager {
    pub async fn create_entity_schema(
        &mut self,
        name: &str,
        description: &str,
        fields: Vec<FieldDefinition>
    ) -> Result<EntitySchema, Error> {
        // Validate field definitions
        self.validate_field_definitions(&fields)?;
        
        // Create schema
        let schema = EntitySchema {
            name: name.to_string(),
            description: description.to_string(),
            stored_fields: fields,
            calculated_fields: Vec::new(),
            validation_rules: Vec::new(),
            display_settings: DisplaySettings::default(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        
        // Store schema
        self.schemas.insert(name.to_string(), schema.clone());
        
        // Create initial version
        self.schema_versions.entry(name.to_string())
            .or_insert_with(Vec::new)
            .push(SchemaVersion {
                version: 1,
                schema: schema.clone(),
                migration_notes: "Initial schema creation".to_string(),
                created_at: Utc::now(),
            });
        
        Ok(schema)
    }
    
    pub async fn add_calculated_field(
        &mut self,
        entity_type: &str,
        calculated_field: CalculatedField
    ) -> Result<(), Error> {
        let schema = self.schemas.get_mut(entity_type)
            .ok_or("Entity schema not found")?;
        
        // Validate formula against existing fields
        let available_fields: Vec<_> = schema.stored_fields.iter()
            .chain(schema.calculated_fields.iter().map(|cf| &cf.field))
            .collect();
            
        let formula_validation = self.validate_calculated_field_formula(
            &calculated_field.formula,
            &available_fields
        )?;
        
        if !formula_validation.is_valid {
            return Err(Error::InvalidFormula(formula_validation.issues));
        }
        
        // Add to schema
        schema.calculated_fields.push(CalculatedFieldDefinition {
            field: calculated_field,
            ui_hints: UIHints::default(),
            update_triggers: vec![UpdateTrigger::AnyFieldChange],
        });
        
        schema.modified_at = Utc::now();
        
        // Update dependency graph
        self.rebuild_dependency_graph(entity_type)?;
        
        Ok(())
    }
    
    pub async fn evolve_schema(
        &mut self,
        entity_type: &str,
        changes: Vec<SchemaChange>
    ) -> Result<SchemaMigration, Error> {
        let current_schema = self.schemas.get(entity_type)
            .ok_or("Entity schema not found")?;
        
        // Plan migration
        let migration_plan = self.migration_engine
            .plan_migration(current_schema, &changes)?;
        
        // Validate migration safety
        if migration_plan.has_data_loss() {
            return Err(Error::UnsafeMigration(migration_plan.data_loss_warnings));
        }
        
        // Execute migration
        let new_schema = self.migration_engine
            .execute_migration(current_schema, migration_plan).await?;
        
        // Update schema
        self.schemas.insert(entity_type.to_string(), new_schema.clone());
        
        // Record version
        let version_number = self.schema_versions.get(entity_type)
            .map(|versions| versions.len() + 1)
            .unwrap_or(1);
            
        self.schema_versions.entry(entity_type.to_string())
            .or_insert_with(Vec::new)
            .push(SchemaVersion {
                version: version_number,
                schema: new_schema.clone(),
                migration_notes: format!("Applied changes: {:?}", changes),
                created_at: Utc::now(),
            });
        
        Ok(SchemaMigration {
            from_version: version_number - 1,
            to_version: version_number,
            changes_applied: changes,
            affected_entities: migration_plan.affected_entity_count,
        })
    }
}

#[derive(Debug, Clone)]
pub enum SchemaChange {
    AddField(FieldDefinition),
    RemoveField(String),
    ModifyField { name: String, new_definition: FieldDefinition },
    AddCalculatedField(CalculatedField),
    RemoveCalculatedField(String),
    ModifyCalculatedField { name: String, new_formula: String },
    AddValidationRule(ValidationRule),
    RemoveValidationRule(String),
}
```

### Entity CRUD Operations

```rust
pub struct EntityManager {
    schemas: Arc<EntitySchemaManager>,
    calculation_engine: Arc<EntityCalculationEngine>,
    validation_engine: Arc<ValidationEngine>,
    storage: Arc<dyn NodeStorage>,
    search_engine: Arc<dyn SearchEngine>,
}

impl EntityManager {
    pub async fn create_entity(
        &self,
        entity_type: &str,
        field_values: HashMap<String, EntityValue>,
        parent_id: Option<String>
    ) -> Result<EntityNode, Error> {
        // 1. Get schema
        let schema = self.schemas.get_schema(entity_type)
            .ok_or("Unknown entity type")?;
        
        // 2. Validate required fields
        for field in &schema.stored_fields {
            if field.is_required && !field_values.contains_key(&field.name) {
                return Err(Error::RequiredFieldMissing(field.name.clone()));
            }
        }
        
        // 3. Validate field values
        let validation_result = self.validation_engine
            .validate_field_values(&field_values, &schema).await?;
            
        if !validation_result.is_valid {
            return Err(Error::ValidationFailed(validation_result.violations));
        }
        
        // 4. Create base entity
        let node_id = uuid::Uuid::new_v4().to_string();
        let base_node = TextNode {
            id: node_id.clone(),
            content: format!("{} Entity", schema.name),
            parent_id,
            children: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        
        let mut entity = EntityNode {
            base: base_node,
            entity_type: entity_type.to_string(),
            stored_fields: field_values,
            calculated_fields: HashMap::new(),
            schema: schema.clone(),
            last_calculated: Utc::now(),
        };
        
        // 5. Calculate computed fields
        let calculation_updates = self.calculation_engine
            .recalculate_entity(&mut entity, &[]).await?;
        
        // 6. Store entity
        self.storage.save_entity(&entity).await?;
        
        // 7. Generate embeddings for search
        let embeddings = entity.generate_embeddings(&self.search_engine).await?;
        self.storage.save_embeddings(&entity.base.id, embeddings).await?;
        
        Ok(entity)
    }
    
    pub async fn update_entity_field(
        &self,
        entity_id: &str,
        field_name: &str,
        new_value: EntityValue
    ) -> Result<EntityUpdateResult, Error> {
        // 1. Load entity
        let mut entity = self.storage.load_entity(entity_id).await?;
        
        // 2. Validate field exists and type matches
        let field_def = entity.schema.get_field_definition(&field_name)
            .ok_or("Field not found")?;
            
        if !self.value_matches_type(&new_value, &field_def.field_type) {
            return Err(Error::TypeMismatch {
                field: field_name.to_string(),
                expected: field_def.field_type.clone(),
                actual: new_value.get_type(),
            });
        }
        
        // 3. Store old value for comparison
        let old_value = entity.stored_fields.get(field_name).cloned();
        
        // 4. Update field
        entity.stored_fields.insert(field_name.to_string(), new_value);
        entity.base.modified_at = Utc::now();
        
        // 5. Validate entity with new value
        let validation_result = self.validation_engine
            .validate_entity(&entity, Some(&[field_name.to_string()])).await?;
            
        if !validation_result.is_valid {
            return Err(Error::ValidationFailed(validation_result.violations));
        }
        
        // 6. Recalculate dependent calculated fields
        let calculation_updates = self.calculation_engine
            .recalculate_entity(&mut entity, &[field_name.to_string()]).await?;
        
        // 7. Save updated entity
        self.storage.save_entity(&entity).await?;
        
        // 8. Update search embeddings if content changed
        if self.affects_search_content(&field_name, &entity.schema) {
            let embeddings = entity.generate_embeddings(&self.search_engine).await?;
            self.storage.save_embeddings(&entity.base.id, embeddings).await?;
        }
        
        Ok(EntityUpdateResult {
            entity_id: entity_id.to_string(),
            field_updates: vec![FieldUpdate {
                field_name: field_name.to_string(),
                old_value,
                new_value: entity.stored_fields.get(field_name).cloned().unwrap(),
                calculation_time: Utc::now(),
            }],
            calculated_field_updates: calculation_updates,
            validation_warnings: validation_result.warnings,
        })
    }
    
    pub async fn query_entities(
        &self,
        entity_type: &str,
        query: EntityQuery
    ) -> Result<EntityQueryResult, Error> {
        // Build query execution plan
        let execution_plan = self.build_query_plan(entity_type, &query)?;
        
        // Execute query with optimizations
        let entities = self.execute_entity_query(execution_plan).await?;
        
        // Apply any calculated fields needed for the query
        let enriched_entities = self.apply_view_calculations(&entities, &query).await?;
        
        Ok(EntityQueryResult {
            entities: enriched_entities,
            total_count: entities.len(),
            query_time: Utc::now(),
            execution_stats: ExecutionStats::default(),
        })
    }
}

#[derive(Debug, Clone)]
pub struct EntityQuery {
    pub filters: Vec<QueryFilter>,
    pub sort_by: Vec<SortCriteria>,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub include_calculated_fields: Vec<String>,
    pub search_text: Option<String>,                    // Full-text search
}

#[derive(Debug, Clone)]
pub struct QueryFilter {
    pub field_name: String,
    pub operator: FilterOperator,
    pub value: EntityValue,
    pub case_sensitive: bool,
}

#[derive(Debug, Clone)]
pub enum FilterOperator {
    Equals,
    NotEquals,
    GreaterThan,
    LessThan,
    GreaterThanOrEqual,
    LessThanOrEqual,
    Contains,
    StartsWith,
    EndsWith,
    In(Vec<EntityValue>),
    Between(EntityValue, EntityValue),
    IsNull,
    IsNotNull,
    Matches(String),                                    // Regex pattern
}
```

## AI Integration

### Natural Language Entity Operations

```rust
impl EntityManager {
    pub async fn create_entity_from_natural_language(
        &self,
        entity_type: &str,
        description: &str,
        nlp_engine: &dyn NLPEngine
    ) -> Result<EntityNode, Error> {
        // 1. Get schema for field extraction guidance
        let schema = self.schemas.get_schema(entity_type)
            .ok_or("Unknown entity type")?;
        
        // 2. Extract field values using AI
        let extraction_prompt = self.build_extraction_prompt(&schema, description);
        let extracted_json = nlp_engine.generate_text(&extraction_prompt, "").await?;
        
        // 3. Parse extracted field values
        let field_values: HashMap<String, EntityValue> = 
            serde_json::from_str(&extracted_json)?;
        
        // 4. Create entity normally
        self.create_entity(entity_type, field_values, None).await
    }
    
    pub async fn update_entity_from_natural_language(
        &self,
        entity_id: &str,
        update_description: &str,
        nlp_engine: &dyn NLPEngine
    ) -> Result<EntityUpdateResult, Error> {
        // 1. Load existing entity
        let entity = self.storage.load_entity(entity_id).await?;
        
        // 2. Extract field updates using AI
        let update_prompt = self.build_update_extraction_prompt(&entity, update_description);
        let update_json = nlp_engine.generate_text(&update_prompt, "").await?;
        
        // 3. Parse field updates
        let field_updates: HashMap<String, EntityValue> = 
            serde_json::from_str(&update_json)?;
        
        // 4. Apply updates
        let mut results = Vec::new();
        for (field_name, new_value) in field_updates {
            let result = self.update_entity_field(entity_id, &field_name, new_value).await?;
            results.push(result);
        }
        
        // 5. Combine results
        Ok(self.merge_update_results(results))
    }
    
    fn build_extraction_prompt(&self, schema: &EntitySchema, description: &str) -> String {
        let field_descriptions = schema.stored_fields.iter()
            .map(|f| format!("  {}: {} - {}", f.name, f.field_type.to_string(), f.description))
            .collect::<Vec<_>>()
            .join("\n");
        
        format!(r#"
Extract field values for creating a {} entity from this description: "{}"

Available fields:
{}

Extract values and respond in JSON format:
{{
  "field_name": "extracted_value_or_null",
  ...
}}

Rules:
- Use null for fields not mentioned or unclear
- Convert values to appropriate types (numbers, dates, booleans)
- For dates, use ISO 8601 format
- For references, extract the entity name/ID if mentioned
"#, schema.name, description, field_descriptions)
    }
    
    fn build_update_extraction_prompt(&self, entity: &EntityNode, update_description: &str) -> String {
        let current_values = entity.stored_fields.iter()
            .map(|(k, v)| format!("  {}: {:?}", k, v))
            .collect::<Vec<_>>()
            .join("\n");
        
        format!(r#"
Update this {} entity based on the description: "{}"

Current field values:
{}

Extract field updates and respond in JSON format:
{{
  "field_name": "new_value",
  ...
}}

Only include fields that should be updated. Use null to clear a field.
"#, entity.entity_type, update_description, current_values)
    }
}
```

## Performance Optimizations

### Calculation Caching and Invalidation

```rust
pub struct CalculationCache {
    cache: HashMap<String, CachedCalculation>,
    dependency_graph: DependencyGraph,
    invalidation_log: Vec<InvalidationEvent>,
}

#[derive(Debug, Clone)]
pub struct CachedCalculation {
    pub entity_id: String,
    pub field_name: String,
    pub value: EntityValue,
    pub calculated_at: DateTime<Utc>,
    pub dependencies_hash: u64,                     // Hash of dependent field values
    pub valid_until: Option<DateTime<Utc>>,         // TTL expiration
}

impl CalculationCache {
    pub fn get_cached_value(
        &self,
        entity_id: &str,
        field_name: &str,
        current_dependencies: &HashMap<String, EntityValue>
    ) -> Option<EntityValue> {
        let cache_key = format!("{}:{}", entity_id, field_name);
        
        if let Some(cached) = self.cache.get(&cache_key) {
            // Check if cache is still valid
            if let Some(valid_until) = cached.valid_until {
                if Utc::now() > valid_until {
                    return None; // Expired
                }
            }
            
            // Check if dependencies changed
            let current_hash = self.hash_dependencies(current_dependencies);
            if current_hash == cached.dependencies_hash {
                return Some(cached.value.clone());
            }
        }
        
        None
    }
    
    pub fn invalidate_dependent_calculations(
        &mut self,
        entity_id: &str,
        changed_fields: &[String]
    ) -> Vec<String> {
        let mut invalidated_fields = Vec::new();
        
        for changed_field in changed_fields {
            let dependent_fields = self.dependency_graph
                .get_dependent_fields(&[changed_field.clone()]);
            
            for dependent_field in dependent_fields {
                let cache_key = format!("{}:{}", entity_id, dependent_field);
                if self.cache.remove(&cache_key).is_some() {
                    invalidated_fields.push(dependent_field);
                    
                    self.invalidation_log.push(InvalidationEvent {
                        entity_id: entity_id.to_string(),
                        field_name: dependent_field,
                        reason: InvalidationReason::DependencyChanged(changed_field.clone()),
                        timestamp: Utc::now(),
                    });
                }
            }
        }
        
        invalidated_fields
    }
}
```

### Batch Operations

```rust
impl EntityManager {
    pub async fn batch_update_entities(
        &self,
        updates: Vec<EntityBatchUpdate>
    ) -> Result<BatchUpdateResult, Error> {
        let mut results = Vec::new();
        let mut failed_updates = Vec::new();
        
        // Group updates by entity type for optimization
        let updates_by_type = self.group_updates_by_type(updates);
        
        for (entity_type, type_updates) in updates_by_type {
            // Load schema once per type
            let schema = self.schemas.get_schema(&entity_type)
                .ok_or("Unknown entity type")?;
            
            // Process updates in batches
            for batch in type_updates.chunks(100) {
                match self.process_update_batch(&schema, batch).await {
                    Ok(batch_results) => results.extend(batch_results),
                    Err(e) => {
                        // Log failed batch but continue with others
                        failed_updates.extend(batch.iter().cloned());
                        log::error!("Batch update failed: {}", e);
                    }
                }
            }
        }
        
        Ok(BatchUpdateResult {
            successful_updates: results,
            failed_updates,
            total_processed: results.len() + failed_updates.len(),
        })
    }
    
    async fn process_update_batch(
        &self,
        schema: &EntitySchema,
        batch: &[EntityBatchUpdate]
    ) -> Result<Vec<EntityUpdateResult>, Error> {
        // Start transaction
        let transaction = self.storage.begin_transaction().await?;
        
        let mut results = Vec::new();
        
        for update in batch {
            match self.process_single_update_in_transaction(&transaction, schema, update).await {
                Ok(result) => results.push(result),
                Err(e) => {
                    // Rollback transaction on any failure
                    transaction.rollback().await?;
                    return Err(e);
                }
            }
        }
        
        // Commit all updates together
        transaction.commit().await?;
        
        Ok(results)
    }
}

#[derive(Debug, Clone)]
pub struct EntityBatchUpdate {
    pub entity_id: String,
    pub entity_type: String,
    pub field_updates: HashMap<String, EntityValue>,
    pub validation_mode: ValidationMode,
}

#[derive(Debug, Clone)]
pub enum ValidationMode {
    Strict,      // Fail entire batch on any validation error
    Lenient,     // Skip invalid updates but continue batch
    Disabled,    // Skip validation for performance (dangerous)
}
```

## Testing Strategy

### Entity Management Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_entity_creation_with_calculated_fields() {
        let services = create_test_services().await;
        let entity_manager = EntityManager::new(services);
        
        // Create employee schema with calculated fields
        let schema = create_employee_schema_with_calculations();
        entity_manager.schemas.add_schema(schema).await?;
        
        // Create employee entity
        let field_values = hashmap! {
            "first_name" => EntityValue::Text("John".to_string()),
            "last_name" => EntityValue::Text("Doe".to_string()),
            "salary" => EntityValue::Number(85000.0),
            "bonus" => EntityValue::Number(15000.0),
            "start_date" => EntityValue::Date(Utc.ymd(2020, 1, 15).and_hms(0, 0, 0)),
        };
        
        let entity = entity_manager.create_entity("employee", field_values, None).await?;
        
        // Verify calculated fields were computed
        assert_eq!(
            entity.get_calculated_field_value("full_name")?,
            EntityValue::Text("John Doe".to_string())
        );
        
        assert_eq!(
            entity.get_calculated_field_value("total_compensation")?,
            EntityValue::Number(100000.0)
        );
        
        // Verify years_employed is calculated correctly
        let years_employed = entity.get_calculated_field_value("years_employed")?;
        if let EntityValue::Number(years) = years_employed {
            assert!(years > 4.0 && years < 5.0); // Approximately 4+ years
        } else {
            panic!("Expected number for years_employed");
        }
    }
    
    #[tokio::test]
    async fn test_calculated_field_updates() {
        let services = create_test_services().await;
        let entity_manager = EntityManager::new(services);
        
        // Create entity
        let entity_id = "test_employee_1";
        let mut entity = create_test_employee(entity_id);
        
        // Update salary
        let update_result = entity_manager.update_entity_field(
            entity_id,
            "salary",
            EntityValue::Number(95000.0)
        ).await?;
        
        // Verify total_compensation was recalculated
        assert_eq!(update_result.calculated_field_updates.len(), 1);
        
        let total_comp_update = &update_result.calculated_field_updates[0];
        assert_eq!(total_comp_update.field_name, "total_compensation");
        assert_eq!(total_comp_update.new_value, EntityValue::Number(110000.0)); // 95000 + 15000
    }
    
    #[tokio::test]
    async fn test_complex_formula_dependencies() {
        let services = create_test_services().await;
        let entity_manager = EntityManager::new(services);
        
        // Create schema with complex dependency chain
        let schema = EntitySchema {
            calculated_fields: vec![
                // Level 1: Direct calculations
                create_calc_field("total_compensation", "salary + bonus"),
                create_calc_field("monthly_salary", "salary / 12"),
                
                // Level 2: Depends on level 1
                create_calc_field("tax_bracket", "IF(total_compensation > 100000, 'High', 'Standard')"),
                
                // Level 3: Depends on level 2
                create_calc_field("take_home", "total_compensation * IF(tax_bracket == 'High', 0.7, 0.8)"),
            ],
            // ... other schema fields
        };
        
        entity_manager.schemas.add_schema(schema).await?;
        
        let entity = create_test_entity_with_salary(85000.0, 15000.0);
        
        // Update salary and verify entire chain recalculates
        let update_result = entity_manager.update_entity_field(
            &entity.base.id,
            "salary",
            EntityValue::Number(120000.0)
        ).await?;
        
        // Should have updated 4 calculated fields
        assert_eq!(update_result.calculated_field_updates.len(), 4);
        
        // Verify final calculation is correct
        let take_home = entity_manager.get_entity_field_value(&entity.base.id, "take_home").await?;
        assert_eq!(take_home, EntityValue::Number(94500.0)); // (120000 + 15000) * 0.7
    }
    
    #[tokio::test]
    async fn test_natural_language_entity_creation() {
        let services = create_test_services().await;
        let entity_manager = EntityManager::new(services);
        
        // Create employee from natural language
        let description = "Add a new employee named Sarah Johnson, she's a Senior Engineer with a salary of $95,000, starting next Monday";
        
        let entity = entity_manager.create_entity_from_natural_language(
            "employee",
            description,
            &services.nlp_engine
        ).await?;
        
        // Verify extracted fields
        assert_eq!(
            entity.get_field_value("first_name")?,
            EntityValue::Text("Sarah".to_string())
        );
        assert_eq!(
            entity.get_field_value("last_name")?,
            EntityValue::Text("Johnson".to_string())
        );
        assert_eq!(
            entity.get_field_value("role")?,
            EntityValue::Text("Senior Engineer".to_string())
        );
        assert_eq!(
            entity.get_field_value("salary")?,
            EntityValue::Number(95000.0)
        );
        
        // Verify calculated fields work
        assert_eq!(
            entity.get_calculated_field_value("full_name")?,
            EntityValue::Text("Sarah Johnson".to_string())
        );
    }
    
    #[tokio::test]
    async fn test_batch_operations_performance() {
        let services = create_test_services().await;
        let entity_manager = EntityManager::new(services);
        
        // Create 1000 test entities
        let start_time = Instant::now();
        
        let batch_updates: Vec<EntityBatchUpdate> = (0..1000)
            .map(|i| EntityBatchUpdate {
                entity_id: format!("employee_{}", i),
                entity_type: "employee".to_string(),
                field_updates: hashmap! {
                    "salary" => EntityValue::Number(50000.0 + (i as f64 * 100.0)),
                },
                validation_mode: ValidationMode::Strict,
            })
            .collect();
        
        let result = entity_manager.batch_update_entities(batch_updates).await?;
        
        let duration = start_time.elapsed();
        
        assert_eq!(result.successful_updates.len(), 1000);
        assert_eq!(result.failed_updates.len(), 0);
        
        // Should complete batch operations efficiently
        assert!(duration.as_millis() < 5000, "Batch operations took too long: {:?}", duration);
    }
}
```

---

This Entity Management Specification provides a comprehensive foundation for structured data handling within NodeSpace, with sophisticated calculated field capabilities, AI integration, and real-world performance considerations. The system enables users to create custom entity types that behave like traditional database tables while integrating seamlessly with the hierarchical node interface and AI-native features.