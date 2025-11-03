# Validation System Design

## Overview

The NodeSpace Validation System provides comprehensive business rule enforcement through natural language rule definition, cross-field validation logic, and conversational error handling. Users can define complex validation rules in plain English, which are automatically converted to executable logic and integrated into the AI-native workflow.

## Core Architecture

### Validation Engine Structure

```rust
pub struct ValidationEngine {
    formula_engine: FormulaEngine,
    rule_generator: ValidationRuleGenerator,
    cross_field_validator: CrossFieldValidator,
    context_manager: ValidationContextManager,
    error_handler: ConversationalErrorHandler,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub id: String,
    pub name: String,                          // "VIP Status Purchase Requirement"
    pub description: String,                   // User's natural language input
    pub target_field: String,                  // Field being validated
    pub dependent_fields: Vec<String>,         // Fields this rule depends on
    pub rule_type: ValidationRuleType,
    pub condition_expression: String,          // When to apply this rule
    pub validation_expression: String,         // What must be true
    pub error_message: String,                // User-friendly error message
    pub severity: ValidationSeverity,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub created_by: String,                    // User who created the rule
    pub is_active: bool,
    pub applies_to: ValidationScope,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationRuleType {
    Required,           // "This field is mandatory"
    DataType,           // "Email must be valid format"
    Range,              // "Salary must be between 30k and 500k"
    Length,             // "Name must be at least 2 characters"
    Pattern,            // "Phone number must match pattern"
    Conditional,        // "If X then Y must be Z"
    Relationship,       // "End date must be after start date"
    BusinessLogic,      // "Manager salary must exceed direct reports"
    Consistency,        // "Premium customers must have premium features"
    Temporal,           // "Meeting date must be in business hours"
    Aggregate,          // "Invoice total must equal sum of line items"
    Custom,             // User-defined complex logic
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationSeverity {
    Error,      // Prevents saving/updating
    Warning,    // Shows warning but allows operation
    Info,       // Informational only
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationScope {
    EntityType(String),                        // Applies to all entities of this type
    SpecificEntity(String),                    // Applies to one specific entity
    EntityWithCondition(String, String),       // Conditional scope: "employees WHERE role = 'Manager'"
    Global,                                    // Applies to all entities
}
```

### Natural Language Rule Generation

```rust
pub struct ValidationRuleGenerator {
    nlp_engine: Arc<dyn NLPEngine>,
    rule_templates: HashMap<String, RuleTemplate>,
    pattern_matcher: RulePatternMatcher,
}

impl ValidationRuleGenerator {
    pub async fn generate_validation_rule(
        &self,
        user_description: &str,
        entity_schema: &EntitySchema,
        context: &RuleGenerationContext
    ) -> Result<ValidationRule, Error> {
        // 1. Classify the rule type from natural language
        let rule_classification = self.classify_rule_type(user_description).await?;
        
        // 2. Extract components using AI
        let extraction_prompt = self.build_rule_extraction_prompt(
            user_description,
            &rule_classification,
            entity_schema
        );
        
        let rule_json = self.nlp_engine.generate_text(&extraction_prompt, "").await?;
        let extracted_rule: ExtractedRule = serde_json::from_str(&rule_json)?;
        
        // 3. Validate and convert to executable form
        let validation_rule = self.convert_to_validation_rule(
            extracted_rule,
            entity_schema,
            context
        )?;
        
        // 4. Test rule compilation
        self.test_rule_compilation(&validation_rule, entity_schema)?;
        
        Ok(validation_rule)
    }
    
    async fn classify_rule_type(&self, description: &str) -> Result<ValidationRuleType, Error> {
        let classification_prompt = format!(r#"
Classify this validation rule description into one of these categories:

REQUIRED - Field must have a value
DATATYPE - Field must be correct type/format (email, phone, etc.)  
RANGE - Numeric field must be within bounds
LENGTH - Text field length constraints
PATTERN - Field must match specific pattern
CONDITIONAL - "If X then Y must be Z" logic
RELATIONSHIP - One field depends on another field's value
BUSINESSLOGIC - Complex business rules involving calculations
CONSISTENCY - Multiple fields must be consistent with each other
TEMPORAL - Date/time related constraints
AGGREGATE - Rules involving sums, counts, or other aggregations
CUSTOM - Complex multi-field logic

Description: "{}"

Respond with just the category name.
"#, description);

        let response = self.nlp_engine.generate_text(&classification_prompt, "").await?;
        
        match response.trim().to_uppercase().as_str() {
            "REQUIRED" => Ok(ValidationRuleType::Required),
            "DATATYPE" => Ok(ValidationRuleType::DataType),
            "RANGE" => Ok(ValidationRuleType::Range),
            "LENGTH" => Ok(ValidationRuleType::Length),
            "PATTERN" => Ok(ValidationRuleType::Pattern),
            "CONDITIONAL" => Ok(ValidationRuleType::Conditional),
            "RELATIONSHIP" => Ok(ValidationRuleType::Relationship),
            "BUSINESSLOGIC" => Ok(ValidationRuleType::BusinessLogic),
            "CONSISTENCY" => Ok(ValidationRuleType::Consistency),
            "TEMPORAL" => Ok(ValidationRuleType::Temporal),
            "AGGREGATE" => Ok(ValidationRuleType::Aggregate),
            _ => Ok(ValidationRuleType::Custom),
        }
    }
    
    fn build_rule_extraction_prompt(
        &self,
        description: &str,
        rule_type: &ValidationRuleType,
        schema: &EntitySchema
    ) -> String {
        let available_fields = schema.stored_fields.iter()
            .chain(schema.calculated_fields.iter().map(|cf| &cf.field))
            .map(|f| format!("  {}: {} - {}", f.name, f.field_type.to_string(), f.description))
            .collect::<Vec<_>>()
            .join("\n");
        
        format!(r#"
Extract validation rule components from this natural language description:

Description: "{}"
Rule Type: {:?}

Available fields in this entity:
{}

Extract the following components and respond in JSON:
{{
  "name": "descriptive_rule_name",
  "target_field": "field_being_validated",
  "dependent_fields": ["fields", "this", "rule", "depends", "on"],
  "condition_expression": "when_to_apply_this_rule_or_true_for_always",
  "validation_expression": "what_must_be_true_for_validation_to_pass",
  "error_message": "user_friendly_error_message",
  "severity": "Error|Warning|Info"
}}

Examples:

"Email field is required" →
{{
  "name": "Email Required",
  "target_field": "email",
  "dependent_fields": [],
  "condition_expression": "true",
  "validation_expression": "email != null AND email != ''",
  "error_message": "Email address is required",
  "severity": "Error"
}}

"If a user's total purchases are below 5000, they cannot be marked as 'VIP' status" →
{{
  "name": "VIP Status Purchase Requirement",
  "target_field": "status",
  "dependent_fields": ["total_purchases"],
  "condition_expression": "total_purchases < 5000",
  "validation_expression": "status != 'VIP'",
  "error_message": "Users with less than $5000 in purchases cannot have VIP status",
  "severity": "Error"
}}

"Manager salary should be higher than their direct reports" →
{{
  "name": "Manager Salary Hierarchy",
  "target_field": "salary",
  "dependent_fields": ["role", "direct_reports"],
  "condition_expression": "role == 'Manager'",
  "validation_expression": "salary > MAX(direct_reports.salary)",
  "error_message": "Manager salary must be higher than all direct report salaries",
  "severity": "Warning"
}}

"Age should be realistic for an employee" →
{{
  "name": "Realistic Employee Age",
  "target_field": "age",
  "dependent_fields": [],
  "condition_expression": "true",
  "validation_expression": "age >= 16 AND age <= 80",
  "error_message": "Employee age should be between 16 and 80 years",
  "severity": "Warning"
}}
"#, description, rule_type, available_fields)
    }
    
    fn convert_to_validation_rule(
        &self,
        extracted: ExtractedRule,
        schema: &EntitySchema,
        context: &RuleGenerationContext
    ) -> Result<ValidationRule, Error> {
        // Validate that referenced fields exist
        for field_name in &extracted.dependent_fields {
            if !schema.has_field(field_name) {
                return Err(Error::InvalidFieldReference {
                    field_name: field_name.clone(),
                    available_fields: schema.get_field_names(),
                });
            }
        }
        
        // Generate unique ID
        let rule_id = uuid::Uuid::new_v4().to_string();
        
        Ok(ValidationRule {
            id: rule_id,
            name: extracted.name,
            description: context.original_description.clone(),
            target_field: extracted.target_field,
            dependent_fields: extracted.dependent_fields,
            rule_type: context.rule_type.clone(),
            condition_expression: extracted.condition_expression,
            validation_expression: extracted.validation_expression,
            error_message: extracted.error_message,
            severity: extracted.severity,
            created_at: Utc::now(),
            modified_at: Utc::now(),
            created_by: context.user_id.clone(),
            is_active: true,
            applies_to: ValidationScope::EntityType(schema.name.clone()),
        })
    }
    
    fn test_rule_compilation(
        &self,
        rule: &ValidationRule,
        schema: &EntitySchema
    ) -> Result<(), Error> {
        // Create test field values
        let test_values = self.create_test_field_values(schema);
        
        // Test condition compilation
        if let Err(e) = self.formula_engine.compile(&rule.condition_expression) {
            return Err(Error::RuleCompilationError {
                rule_name: rule.name.clone(),
                expression: rule.condition_expression.clone(),
                error: format!("Condition compilation failed: {}", e),
            });
        }
        
        // Test validation compilation
        if let Err(e) = self.formula_engine.compile(&rule.validation_expression) {
            return Err(Error::RuleCompilationError {
                rule_name: rule.name.clone(),
                expression: rule.validation_expression.clone(),
                error: format!("Validation compilation failed: {}", e),
            });
        }
        
        // Test actual evaluation with test data
        match self.formula_engine.evaluate(&rule.condition_expression, &test_values) {
            Ok(EntityValue::Boolean(_)) => {}, // Good
            Ok(other) => return Err(Error::RuleCompilationError {
                rule_name: rule.name.clone(),
                expression: rule.condition_expression.clone(),
                error: format!("Condition must return boolean, got: {:?}", other),
            }),
            Err(e) => return Err(Error::RuleCompilationError {
                rule_name: rule.name.clone(),
                expression: rule.condition_expression.clone(),
                error: format!("Condition evaluation failed: {}", e),
            }),
        }
        
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
struct ExtractedRule {
    name: String,
    target_field: String,
    dependent_fields: Vec<String>,
    condition_expression: String,
    validation_expression: String,
    error_message: String,
    severity: ValidationSeverity,
}
```

### Cross-Field Validation Engine

```rust
pub struct CrossFieldValidator {
    formula_engine: FormulaEngine,
    dependency_tracker: DependencyTracker,
    validation_cache: ValidationCache,
}

impl CrossFieldValidator {
    pub async fn validate_entity(
        &self,
        entity: &EntityNode,
        changed_fields: &[String],
        context: &ValidationContext
    ) -> Result<ValidationResult, Error> {
        // 1. Get applicable rules
        let applicable_rules = self.get_applicable_rules(
            &entity.entity_type,
            changed_fields,
            &entity.schema.validation_rules
        );
        
        // 2. Get all field values (stored + calculated)
        let all_field_values = entity.get_all_field_values()?;
        
        // 3. Validate each applicable rule
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        let mut info_messages = Vec::new();
        
        for rule in applicable_rules {
            let validation_result = self.validate_single_rule(
                rule,
                &all_field_values,
                context
            ).await?;
            
            if !validation_result.passed {
                let violation = ValidationViolation {
                    rule_id: rule.id.clone(),
                    rule_name: rule.name.clone(),
                    field_name: rule.target_field.clone(),
                    message: self.personalize_error_message(&rule.error_message, &all_field_values),
                    severity: rule.severity.clone(),
                    rule_description: rule.description.clone(),
                    dependent_fields: rule.dependent_fields.clone(),
                    condition_that_triggered: rule.condition_expression.clone(),
                    failed_validation: rule.validation_expression.clone(),
                    context_info: validation_result.context_info,
                };
                
                match rule.severity {
                    ValidationSeverity::Error => violations.push(violation),
                    ValidationSeverity::Warning => warnings.push(violation),
                    ValidationSeverity::Info => info_messages.push(violation),
                }
            }
        }
        
        Ok(ValidationResult {
            is_valid: violations.is_empty(),
            violations,
            warnings,
            info_messages,
            validation_time: Utc::now(),
            rules_evaluated: applicable_rules.len(),
        })
    }
    
    async fn validate_single_rule(
        &self,
        rule: &ValidationRule,
        field_values: &HashMap<String, EntityValue>,
        context: &ValidationContext
    ) -> Result<SingleRuleValidationResult, Error> {
        // 1. Check if condition is met
        let condition_result = self.evaluate_condition(
            &rule.condition_expression,
            field_values,
            context
        ).await?;
        
        if !condition_result.condition_met {
            return Ok(SingleRuleValidationResult {
                passed: true, // Rule doesn't apply
                context_info: HashMap::new(),
            });
        }
        
        // 2. Evaluate validation expression
        let validation_result = self.evaluate_validation(
            &rule.validation_expression,
            field_values,
            context
        ).await?;
        
        Ok(SingleRuleValidationResult {
            passed: validation_result.validation_passed,
            context_info: validation_result.context_info,
        })
    }
    
    async fn evaluate_condition(
        &self,
        condition_expr: &str,
        field_values: &HashMap<String, EntityValue>,
        context: &ValidationContext
    ) -> Result<ConditionEvaluationResult, Error> {
        let mut scope = rhai::Scope::new();
        
        // Add field values to scope
        for (name, value) in field_values {
            self.add_field_to_scope(&mut scope, name, value);
        }
        
        // Add context values
        self.add_context_to_scope(&mut scope, context);
        
        // Add validation-specific functions
        self.add_validation_functions(&mut scope);
        
        match self.formula_engine.engine.eval_with_scope(&mut scope, condition_expr) {
            Ok(result) => {
                match result.as_bool() {
                    Ok(condition_met) => Ok(ConditionEvaluationResult {
                        condition_met,
                        evaluation_context: HashMap::new(),
                    }),
                    Err(_) => Err(Error::ValidationEvaluationError {
                        expression: condition_expr.to_string(),
                        error: "Condition must return a boolean value".to_string(),
                    }),
                }
            },
            Err(e) => Err(Error::ValidationEvaluationError {
                expression: condition_expr.to_string(),
                error: e.to_string(),
            }),
        }
    }
    
    fn add_validation_functions(&self, scope: &mut rhai::Scope) {
        // Date/time functions
        scope.push_constant("TODAY", || chrono::Utc::now().timestamp());
        scope.push_constant("NOW", || chrono::Utc::now().timestamp());
        
        // String validation functions
        scope.push_constant("IS_EMAIL", |email: String| -> bool {
            let email_regex = regex::Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();
            email_regex.is_match(&email)
        });
        
        scope.push_constant("IS_PHONE", |phone: String| -> bool {
            let phone_regex = regex::Regex::new(r"^\+?[\d\s\-\(\)]{10,}$").unwrap();
            phone_regex.is_match(&phone.replace(&[' ', '-', '(', ')'][..], ""))
        });
        
        scope.push_constant("MATCHES", |text: String, pattern: String| -> bool {
            match regex::Regex::new(&pattern) {
                Ok(regex) => regex.is_match(&text),
                Err(_) => false,
            }
        });
        
        // Business logic functions
        scope.push_constant("IS_BUSINESS_DAY", |timestamp: i64| -> bool {
            let date = chrono::DateTime::from_timestamp(timestamp, 0).unwrap();
            !matches!(date.weekday(), chrono::Weekday::Sat | chrono::Weekday::Sun)
        });
        
        scope.push_constant("WORKING_HOURS", |timestamp: i64| -> bool {
            let datetime = chrono::DateTime::from_timestamp(timestamp, 0).unwrap();
            let hour = datetime.hour();
            hour >= 9 && hour <= 17
        });
        
        // Aggregate functions
        scope.push_constant("MAX", |values: rhai::Array| -> f64 {
            values.iter()
                .filter_map(|v| v.as_float().ok())
                .fold(f64::NEG_INFINITY, f64::max)
        });
        
        scope.push_constant("MIN", |values: rhai::Array| -> f64 {
            values.iter()
                .filter_map(|v| v.as_float().ok())
                .fold(f64::INFINITY, f64::min)
        });
        
        scope.push_constant("SUM", |values: rhai::Array| -> f64 {
            values.iter()
                .filter_map(|v| v.as_float().ok())
                .sum()
        });
        
        scope.push_constant("COUNT", |values: rhai::Array| -> i64 {
            values.len() as i64
        });
        
        scope.push_constant("AVERAGE", |values: rhai::Array| -> f64 {
            let nums: Vec<f64> = values.iter()
                .filter_map(|v| v.as_float().ok())
                .collect();
            if nums.is_empty() { 0.0 } else { nums.iter().sum::<f64>() / nums.len() as f64 }
        });
    }
    
    fn personalize_error_message(
        &self,
        template: &str,
        field_values: &HashMap<String, EntityValue>
    ) -> String {
        let mut message = template.to_string();
        
        // Replace field value placeholders
        for (field_name, value) in field_values {
            let placeholder = format!("{{{}}}", field_name);
            let value_str = match value {
                EntityValue::Text(s) => s.clone(),
                EntityValue::Number(n) => format!("{}", n),
                EntityValue::Integer(i) => format!("{}", i),
                EntityValue::Currency { amount, currency } => format!("{} {}", amount, currency),
                EntityValue::Date(d) => d.format("%Y-%m-%d").to_string(),
                EntityValue::Boolean(b) => b.to_string(),
                _ => "unknown".to_string(),
            };
            message = message.replace(&placeholder, &value_str);
        }
        
        // Add contextual information
        message = self.add_contextual_suggestions(message, field_values);
        
        message
    }
    
    fn add_contextual_suggestions(
        &self,
        message: String,
        field_values: &HashMap<String, EntityValue>
    ) -> String {
        // Add helpful suggestions based on the type of validation failure
        // This could be enhanced with AI-generated suggestions
        message
    }
}

#[derive(Debug)]
struct SingleRuleValidationResult {
    passed: bool,
    context_info: HashMap<String, String>,
}

#[derive(Debug)]
struct ConditionEvaluationResult {
    condition_met: bool,
    evaluation_context: HashMap<String, String>,
}
```

### Conversational Error Handling

```rust
pub struct ConversationalErrorHandler {
    nlp_engine: Arc<dyn NLPEngine>,
    suggestion_generator: ValidationSuggestionGenerator,
    error_context_builder: ErrorContextBuilder,
}

impl ConversationalErrorHandler {
    pub async fn generate_conversational_error_response(
        &self,
        validation_result: &ValidationResult,
        original_intent: &str,
        entity_context: &EntityContext
    ) -> Result<ConversationalErrorResponse, Error> {
        match validation_result.violations.len() {
            0 => Ok(ConversationalErrorResponse::Success),
            1 => self.handle_single_validation_error(
                &validation_result.violations[0],
                original_intent,
                entity_context
            ).await,
            _ => self.handle_multiple_validation_errors(
                &validation_result.violations,
                original_intent,
                entity_context
            ).await,
        }
    }
    
    async fn handle_single_validation_error(
        &self,
        violation: &ValidationViolation,
        original_intent: &str,
        entity_context: &EntityContext
    ) -> Result<ConversationalErrorResponse, Error> {
        let error_explanation_prompt = format!(r#"
The user tried to: "{}"

This failed because of this business rule violation:
- Rule: {}
- Field: {}
- Error: {}
- Dependent fields: {}

Current entity context:
{}

Generate a helpful, conversational response that:
1. Explains what went wrong in simple, non-technical terms
2. Shows the specific values that caused the problem
3. Suggests 2-3 concrete ways to fix the issue
4. Maintains a helpful, supportive tone
5. Offers to help implement the suggested fixes

Make the response conversational and actionable, as if you're a helpful assistant.
"#, 
            original_intent,
            violation.rule_name,
            violation.field_name,
            violation.message,
            violation.dependent_fields.join(", "),
            self.format_entity_context(entity_context)
        );
        
        let conversational_response = self.nlp_engine
            .generate_text(&error_explanation_prompt, "").await?;
        
        // Generate actionable suggestions
        let suggestions = self.suggestion_generator
            .generate_fix_suggestions(violation, entity_context).await?;
        
        Ok(ConversationalErrorResponse::ValidationError {
            explanation: conversational_response,
            suggestions,
            can_override: violation.severity != ValidationSeverity::Error,
            retry_options: self.generate_retry_options(violation, entity_context),
        })
    }
    
    async fn handle_multiple_validation_errors(
        &self,
        violations: &[ValidationViolation],
        original_intent: &str,
        entity_context: &EntityContext
    ) -> Result<ConversationalErrorResponse, Error> {
        let violations_summary = violations.iter()
            .map(|v| format!("• {}: {}", v.field_name, v.message))
            .collect::<Vec<_>>()
            .join("\n");
        
        let multiple_errors_prompt = format!(r#"
The user tried to: "{}"

This failed because of multiple business rule violations:
{}

Entity context:
{}

Generate a helpful response that:
1. Acknowledges multiple issues without being overwhelming
2. Groups related problems together
3. Prioritizes the most important fixes first
4. Suggests tackling them in a logical order
5. Offers to help with each fix step by step

Keep the tone encouraging and organized.
"#, 
            original_intent,
            violations_summary,
            self.format_entity_context(entity_context)
        );
        
        let conversational_response = self.nlp_engine
            .generate_text(&multiple_errors_prompt, "").await?;
        
        // Group suggestions by priority
        let grouped_suggestions = self.suggestion_generator
            .group_fix_suggestions_by_priority(violations, entity_context).await?;
        
        Ok(ConversationalErrorResponse::MultipleErrors {
            explanation: conversational_response,
            error_groups: grouped_suggestions,
            suggested_order: self.determine_fix_order(violations),
        })
    }
    
    fn format_entity_context(&self, context: &EntityContext) -> String {
        let mut context_lines = Vec::new();
        
        context_lines.push(format!("Entity Type: {}", context.entity_type));
        
        if let Some(current_values) = &context.current_field_values {
            context_lines.push("Current Values:".to_string());
            for (field, value) in current_values.iter().take(5) { // Limit for readability
                context_lines.push(format!("  {}: {:?}", field, value));
            }
        }
        
        if let Some(attempted_values) = &context.attempted_changes {
            context_lines.push("Attempted Changes:".to_string());
            for (field, value) in attempted_values {
                context_lines.push(format!("  {}: {:?}", field, value));
            }
        }
        
        context_lines.join("\n")
    }
}

#[derive(Debug, Clone)]
pub enum ConversationalErrorResponse {
    Success,
    ValidationError {
        explanation: String,
        suggestions: Vec<FixSuggestion>,
        can_override: bool,
        retry_options: Vec<RetryOption>,
    },
    MultipleErrors {
        explanation: String,
        error_groups: Vec<ErrorGroup>,
        suggested_order: Vec<String>, // Field names in suggested fix order
    },
}

#[derive(Debug, Clone)]
pub struct FixSuggestion {
    pub description: String,                    // "Increase total purchases to at least $5,000"
    pub action_type: SuggestionActionType,
    pub estimated_effort: EffortLevel,
    pub can_auto_apply: bool,
    pub auto_apply_description: Option<String>, // What will happen if auto-applied
}

#[derive(Debug, Clone)]
pub enum SuggestionActionType {
    UpdateField { field: String, suggested_value: EntityValue },
    UpdateMultipleFields { updates: HashMap<String, EntityValue> },
    ManagerOverride { reason: String },
    ContactSupport { issue_description: String },
    Custom { instructions: String },
}

#[derive(Debug, Clone)]
pub enum EffortLevel {
    Immediate,  // Can fix right now
    Quick,      // 5 minutes or less
    Moderate,   // Requires some research/coordination
    Complex,    // Significant effort required
}
```

### Validation Context Management

```rust
pub struct ValidationContext {
    pub entity_id: String,
    pub entity_type: String,
    pub operation_type: OperationType,
    pub user_id: String,
    pub current_timestamp: DateTime<Utc>,
    pub related_entities: HashMap<String, Vec<EntityNode>>,
    pub external_data: HashMap<String, serde_json::Value>,
    pub validation_mode: ValidationMode,
    pub skip_rules: HashSet<String>,                    // Rule IDs to skip
    pub additional_context: HashMap<String, String>,
}

#[derive(Debug, Clone)]
pub enum OperationType {
    Create,
    Update { changed_fields: Vec<String> },
    Delete,
    BulkUpdate { batch_size: usize },
}

#[derive(Debug, Clone)]
pub enum ValidationMode {
    Strict,      // All rules must pass
    Lenient,     // Warnings allowed, only errors block
    Advisory,    // All violations become warnings
    Disabled,    // Skip validation entirely (dangerous)
}

impl ValidationContext {
    pub fn for_entity_update(
        entity_id: String,
        entity_type: String,
        changed_fields: Vec<String>,
        user_id: String
    ) -> Self {
        Self {
            entity_id,
            entity_type,
            operation_type: OperationType::Update { changed_fields },
            user_id,
            current_timestamp: Utc::now(),
            related_entities: HashMap::new(),
            external_data: HashMap::new(),
            validation_mode: ValidationMode::Strict,
            skip_rules: HashSet::new(),
            additional_context: HashMap::new(),
        }
    }
    
    pub async fn enrich_with_related_entities(
        &mut self,
        entity_manager: &EntityManager,
        rules: &[ValidationRule]
    ) -> Result<(), Error> {
        // Find all entity types referenced in validation rules
        let referenced_entity_types = self.extract_referenced_entity_types(rules);
        
        for entity_type in referenced_entity_types {
            let related_entities = entity_manager
                .find_related_entities(&self.entity_id, &entity_type)
                .await?;
            
            self.related_entities.insert(entity_type, related_entities);
        }
        
        Ok(())
    }
    
    fn extract_referenced_entity_types(&self, rules: &[ValidationRule]) -> HashSet<String> {
        let mut entity_types = HashSet::new();
        
        for rule in rules {
            // Parse rule expressions to find entity type references
            if rule.validation_expression.contains("employees.") {
                entity_types.insert("employee".to_string());
            }
            if rule.validation_expression.contains("projects.") {
                entity_types.insert("project".to_string());
            }
            // Add more entity type detection logic as needed
        }
        
        entity_types
    }
}
```

## Advanced Validation Patterns

### Business Rule Templates

```rust
pub struct BusinessRuleTemplates {
    templates: HashMap<String, RuleTemplate>,
}

impl BusinessRuleTemplates {
    pub fn new() -> Self {
        let mut templates = HashMap::new();
        
        // Financial rules
        templates.insert("salary_hierarchy".to_string(), RuleTemplate {
            name: "Salary Hierarchy Validation".to_string(),
            description: "Manager salaries must be higher than their direct reports".to_string(),
            pattern: r"(\w+)\s+salary\s+(?:must be|should be)\s+(?:higher|greater)\s+than\s+(?:their\s+)?(.+)".to_string(),
            template_expression: "role == '{manager_role}' IMPLIES salary > MAX({reports_field}.salary)".to_string(),
            default_severity: ValidationSeverity::Warning,
            category: RuleCategory::BusinessLogic,
        });
        
        // Status consistency rules
        templates.insert("status_consistency".to_string(), RuleTemplate {
            name: "Status Consistency Validation".to_string(),
            description: "If status is X, then field Y must be Z".to_string(),
            pattern: r"if\s+(.+)\s+(?:is|equals?)\s+['\"](.+)['\"],?\s+then\s+(.+)\s+must\s+(?:be|equal)\s+(.+)".to_string(),
            template_expression: "{status_field} == '{status_value}' IMPLIES {target_field} {operator} {target_value}".to_string(),
            default_severity: ValidationSeverity::Error,
            category: RuleCategory::Consistency,
        });
        
        // Date relationship rules
        templates.insert("date_sequence".to_string(), RuleTemplate {
            name: "Date Sequence Validation".to_string(),
            description: "One date must be before/after another date".to_string(),
            pattern: r"(.+)\s+(?:must be|should be)\s+(before|after)\s+(.+)".to_string(),
            template_expression: "{first_date} {operator} {second_date}".to_string(),
            default_severity: ValidationSeverity::Error,
            category: RuleCategory::Temporal,
        });
        
        // Aggregate consistency rules
        templates.insert("sum_validation".to_string(), RuleTemplate {
            name: "Sum Validation".to_string(),
            description: "Total field must equal sum of component fields".to_string(),
            pattern: r"(.+)\s+(?:must equal|should equal|equals)\s+(?:sum of|total of)\s+(.+)".to_string(),
            template_expression: "ABS({total_field} - SUM({component_fields})) < 0.01".to_string(),
            default_severity: ValidationSeverity::Error,
            category: RuleCategory::Aggregate,
        });
        
        Self { templates }
    }
    
    pub fn find_matching_template(&self, description: &str) -> Option<&RuleTemplate> {
        for template in self.templates.values() {
            if let Ok(regex) = regex::Regex::new(&template.pattern) {
                if regex.is_match(description) {
                    return Some(template);
                }
            }
        }
        None
    }
}

#[derive(Debug, Clone)]
pub struct RuleTemplate {
    pub name: String,
    pub description: String,
    pub pattern: String,                        // Regex pattern to match descriptions
    pub template_expression: String,            // Template with placeholders
    pub default_severity: ValidationSeverity,
    pub category: RuleCategory,
}

#[derive(Debug, Clone)]
pub enum RuleCategory {
    DataIntegrity,
    BusinessLogic,
    Consistency,
    Temporal,
    Financial,
    Aggregate,
    Security,
    Compliance,
}
```

### Performance Optimization

```rust
pub struct ValidationPerformanceOptimizer {
    rule_cache: HashMap<String, CompiledRule>,
    dependency_graph: ValidationDependencyGraph,
    execution_stats: ValidationExecutionStats,
}

impl ValidationPerformanceOptimizer {
    pub fn optimize_validation_execution(
        &self,
        rules: &[ValidationRule],
        changed_fields: &[String]
    ) -> ValidationExecutionPlan {
        // 1. Filter rules that could be affected by changed fields
        let relevant_rules = self.filter_relevant_rules(rules, changed_fields);
        
        // 2. Order rules by dependency and cost
        let execution_order = self.determine_optimal_execution_order(&relevant_rules);
        
        // 3. Identify rules that can be executed in parallel
        let parallel_groups = self.identify_parallel_execution_groups(&execution_order);
        
        // 4. Pre-compile expressions for faster execution
        let compiled_rules = self.pre_compile_rules(&relevant_rules);
        
        ValidationExecutionPlan {
            execution_groups: parallel_groups,
            compiled_rules,
            estimated_execution_time: self.estimate_execution_time(&relevant_rules),
            can_use_cache: self.can_use_cached_results(&relevant_rules, changed_fields),
        }
    }
    
    fn filter_relevant_rules(
        &self,
        rules: &[ValidationRule],
        changed_fields: &[String]
    ) -> Vec<&ValidationRule> {
        rules.iter()
            .filter(|rule| {
                // Rule is relevant if:
                // 1. Target field was changed
                changed_fields.contains(&rule.target_field) ||
                // 2. Any dependent field was changed
                rule.dependent_fields.iter().any(|dep| changed_fields.contains(dep)) ||
                // 3. Rule has no dependencies (always applies)
                rule.dependent_fields.is_empty()
            })
            .collect()
    }
    
    fn determine_optimal_execution_order(
        &self,
        rules: &[&ValidationRule]
    ) -> Vec<String> {
        // Sort by execution cost (lighter rules first)
        let mut rule_costs: Vec<(String, f32)> = rules.iter()
            .map(|rule| {
                let cost = self.calculate_rule_execution_cost(rule);
                (rule.id.clone(), cost)
            })
            .collect();
        
        rule_costs.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        
        rule_costs.into_iter().map(|(id, _)| id).collect()
    }
    
    fn calculate_rule_execution_cost(&self, rule: &ValidationRule) -> f32 {
        let mut cost = 1.0; // Base cost
        
        // Add cost for each dependent field
        cost += rule.dependent_fields.len() as f32 * 0.1;
        
        // Add cost for complex operations
        if rule.validation_expression.contains("MAX") ||
           rule.validation_expression.contains("MIN") ||
           rule.validation_expression.contains("SUM") {
            cost += 2.0;
        }
        
        // Add cost for regex operations
        if rule.validation_expression.contains("MATCHES") {
            cost += 1.5;
        }
        
        // Add cost for external data access
        if rule.validation_expression.contains("external.") {
            cost += 5.0;
        }
        
        cost
    }
}

#[derive(Debug)]
pub struct ValidationExecutionPlan {
    execution_groups: Vec<Vec<String>>,  // Groups that can run in parallel
    compiled_rules: HashMap<String, CompiledRule>,
    estimated_execution_time: Duration,
    can_use_cache: bool,
}

#[derive(Debug)]
pub struct CompiledRule {
    rule_id: String,
    compiled_condition: rhai::AST,
    compiled_validation: rhai::AST,
    dependencies: Vec<String>,
}
```

## Testing Strategy

### Validation System Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_natural_language_rule_generation() {
        let services = create_test_services().await;
        let rule_generator = ValidationRuleGenerator::new(services.nlp_engine.clone());
        
        let employee_schema = create_employee_schema();
        let context = RuleGenerationContext {
            original_description: "If a user's total purchases are below 5000, they cannot be marked as 'VIP' status".to_string(),
            rule_type: ValidationRuleType::Conditional,
            user_id: "test_user".to_string(),
        };
        
        let rule = rule_generator.generate_validation_rule(
            "If a user's total purchases are below 5000, they cannot be marked as 'VIP' status",
            &employee_schema,
            &context
        ).await?;
        
        assert_eq!(rule.name, "VIP Status Purchase Requirement");
        assert_eq!(rule.target_field, "status");
        assert_eq!(rule.dependent_fields, vec!["total_purchases"]);
        assert_eq!(rule.condition_expression, "total_purchases < 5000");
        assert_eq!(rule.validation_expression, "status != 'VIP'");
        assert_eq!(rule.severity, ValidationSeverity::Error);
    }
    
    #[tokio::test]
    async fn test_cross_field_validation_execution() {
        let services = create_test_services().await;
        let validator = CrossFieldValidator::new(services.formula_engine.clone());
        
        // Create test entity that violates VIP rule
        let mut customer = EntityNode {
            entity_type: "customer".to_string(),
            stored_fields: hashmap! {
                "name" => EntityValue::Text("John Doe".to_string()),
                "total_purchases" => EntityValue::Number(3000.0),
                "status" => EntityValue::Text("VIP".to_string()),
            },
            schema: create_customer_schema_with_vip_rule(),
            // ... other fields
        };
        
        let validation_context = ValidationContext::for_entity_update(
            customer.base.id.clone(),
            customer.entity_type.clone(),
            vec!["status".to_string()],
            "test_user".to_string()
        );
        
        let result = validator.validate_entity(
            &customer,
            &["status"],
            &validation_context
        ).await?;
        
        assert!(!result.is_valid);
        assert_eq!(result.violations.len(), 1);
        
        let violation = &result.violations[0];
        assert_eq!(violation.field_name, "status");
        assert!(violation.message.contains("$5000"));
        assert!(violation.message.contains("VIP"));
    }
    
    #[tokio::test]
    async fn test_conversational_error_handling() {
        let services = create_test_services().await;
        let error_handler = ConversationalErrorHandler::new(services.nlp_engine.clone());
        
        let violation = ValidationViolation {
            rule_name: "VIP Status Purchase Requirement".to_string(),
            field_name: "status".to_string(),
            message: "Users with less than $5000 in purchases cannot have VIP status".to_string(),
            severity: ValidationSeverity::Error,
            dependent_fields: vec!["total_purchases".to_string()],
            // ... other fields
        };
        
        let validation_result = ValidationResult {
            is_valid: false,
            violations: vec![violation],
            warnings: vec![],
            info_messages: vec![],
            validation_time: Utc::now(),
            rules_evaluated: 1,
        };
        
        let entity_context = EntityContext {
            entity_type: "customer".to_string(),
            current_field_values: Some(hashmap! {
                "name" => EntityValue::Text("John Doe".to_string()),
                "total_purchases" => EntityValue::Number(3000.0),
                "status" => EntityValue::Text("Regular".to_string()),
            }),
            attempted_changes: Some(hashmap! {
                "status" => EntityValue::Text("VIP".to_string()),
            }),
        };
        
        let response = error_handler.generate_conversational_error_response(
            &validation_result,
            "Set John's status to VIP",
            &entity_context
        ).await?;
        
        match response {
            ConversationalErrorResponse::ValidationError { explanation, suggestions, .. } => {
                assert!(explanation.contains("$3"));
                assert!(explanation.contains("$5000"));
                assert!(explanation.contains("VIP"));
                assert!(!suggestions.is_empty());
                
                // Check that suggestions are actionable
                let first_suggestion = &suggestions[0];
                assert!(first_suggestion.description.contains("increase") || 
                       first_suggestion.description.contains("purchase"));
            },
            _ => panic!("Expected ValidationError response"),
        }
    }
    
    #[tokio::test]
    async fn test_validation_performance_optimization() {
        let services = create_test_services().await;
        let optimizer = ValidationPerformanceOptimizer::new();
        
        // Create entity with many validation rules
        let rules = create_complex_rule_set(); // 50+ rules
        let changed_fields = vec!["salary".to_string()];
        
        let execution_plan = optimizer.optimize_validation_execution(&rules, &changed_fields);
        
        // Should filter to only relevant rules
        assert!(execution_plan.compiled_rules.len() < rules.len());
        
        // Should group independent rules for parallel execution
        assert!(execution_plan.execution_groups.len() > 1);
        
        // Should have reasonable execution time estimate
        assert!(execution_plan.estimated_execution_time.as_millis() < 1000);
    }
    
    #[tokio::test]
    async fn test_business_rule_templates() {
        let templates = BusinessRuleTemplates::new();
        
        // Test salary hierarchy template
        let description = "Manager salary must be higher than their direct reports";
        let template = templates.find_matching_template(description);
        
        assert!(template.is_some());
        let template = template.unwrap();
        assert_eq!(template.category, RuleCategory::BusinessLogic);
        assert!(template.template_expression.contains("MAX"));
    }
    
    #[tokio::test]
    async fn test_validation_caching() {
        let services = create_test_services().await;
        let mut validation_cache = ValidationCache::new();
        
        let entity = create_test_entity();
        let rules = vec![create_test_validation_rule()];
        
        // First validation - cache miss
        let start_time = Instant::now();
        let result1 = services.validation_engine.validate_entity(&entity, &[], &Default::default()).await?;
        let first_duration = start_time.elapsed();
        
        // Second validation - cache hit (no fields changed)
        let start_time = Instant::now();
        let result2 = services.validation_engine.validate_entity(&entity, &[], &Default::default()).await?;
        let second_duration = start_time.elapsed();
        
        assert_eq!(result1.is_valid, result2.is_valid);
        assert!(second_duration < first_duration / 2, "Cache should significantly speed up validation");
    }
}

fn create_customer_schema_with_vip_rule() -> EntitySchema {
    EntitySchema {
        validation_rules: vec![
            ValidationRule {
                id: "vip_purchase_rule".to_string(),
                name: "VIP Status Purchase Requirement".to_string(),
                target_field: "status".to_string(),
                dependent_fields: vec!["total_purchases".to_string()],
                rule_type: ValidationRuleType::Conditional,
                condition_expression: "total_purchases < 5000".to_string(),
                validation_expression: "status != 'VIP'".to_string(),
                error_message: "Users with less than $5000 in purchases cannot have VIP status".to_string(),
                severity: ValidationSeverity::Error,
                // ... other fields
            }
        ],
        // ... other schema fields
    }
}
```

---

This Validation System Design provides a comprehensive framework for business rule enforcement in NodeSpace, with natural language rule definition, sophisticated cross-field validation, conversational error handling, and performance optimizations. The system enables users to define complex business logic in plain English while maintaining robust validation capabilities and providing helpful, actionable feedback when validation fails.