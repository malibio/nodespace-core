# Testing Strategies for NodeSpace

## Overview

NodeSpace requires comprehensive testing across multiple dimensions: Rust backend services, Svelte frontend components, AI integrations, real-time systems, and plugin architectures. This specification outlines testing strategies that emphasize **real implementations** over mocks, ensuring robust validation of the complete system.

## Core Testing Philosophy

### Real Implementation Priority

Following the user's explicit requirement: "I don't want to use mock/placeholders. I want the real implementation. If something is not working, I want it fixed" - all testing strategies prioritize real service implementations:

- **Real Database**: Use actual LanceDB instances for testing
- **Real AI Services**: Test with actual NLP engines (Ollama, FastEmbed)
- **Real File Operations**: Test with actual file I/O and storage
- **Real Network Operations**: Test with real API calls and websockets
- **Real Plugin Loading**: Test actual plugin compilation and loading

---

## Testing Architecture Overview

### Multi-Layer Testing Strategy

```rust
// Testing service architecture that mirrors production
pub struct TestingServices {
    // Real implementations with test configuration
    pub storage: Arc<LanceDBStorage>,           // Real LanceDB with test DB
    pub vector_db: Arc<LanceDBStorage>,           // Real LanceDB with test collections
    pub nlp_engine: Arc<OllamaNLPEngine>,         // Real Ollama with test models
    pub search_engine: Arc<FastEmbedSearch>,      // Real FastEmbed with test indexes
    pub validation_engine: Arc<ValidationEngine>, // Real validation with test rules
    pub file_storage: Arc<FileSystemStorage>,     // Real filesystem with test directories
}

impl TestingServices {
    pub async fn create_isolated_instance() -> Result<Self, Error> {
        // Create completely isolated test environment with real services
        let test_db_name = format!("nodespace_test_{}", uuid::Uuid::new_v4());
        let test_vector_collection = format!("test_vectors_{}", uuid::Uuid::new_v4());
        let test_file_root = format!("/tmp/nodespace_test_{}", uuid::Uuid::new_v4());
        
        // Initialize real services with test-specific configuration
        let storage = Arc::new(LanceDBStorage::new(&format!(
            "/tmp/test_lance_db_{}", test_db_name
        )).await?);
        
        let vector_db = Arc::new(LanceDBStorage::new(&format!(
            "/tmp/test_vectors/{}", test_vector_collection
        )).await?);
        
        let nlp_engine = Arc::new(OllamaNLPEngine::new(OllamaConfig {
            base_url: "http://localhost:11434".to_string(),
            model_name: "llama2:7b".to_string(), // Use smaller model for tests
            timeout: Duration::from_secs(30),
        }).await?);
        
        // Verify all services are actually working
        storage.health_check().await?;
        vector_db.health_check().await?;
        nlp_engine.health_check().await?;
        
        tokio::fs::create_dir_all(&test_file_root).await?;
        
        Ok(TestingServices {
            storage,
            vector_db,
            nlp_engine: nlp_engine.clone(),
            search_engine: Arc::new(FastEmbedSearch::new(nlp_engine.clone()).await?),
            validation_engine: Arc::new(ValidationEngine::new()),
            file_storage: Arc::new(FileSystemStorage::new(&test_file_root)),
        })
    }
    
    pub async fn cleanup(&self) -> Result<(), Error> {
        // Clean up test data but keep services for inspection if needed
        self.storage.drop_test_database().await?;
        self.vector_db.drop_test_collection().await?;
        self.file_storage.cleanup_test_directory().await?;
        Ok(())
    }
}
```

---

## Unit Testing

### Entity Management Testing

```rust
#[cfg(test)]
mod entity_management_tests {
    use super::*;
    use tokio_test;
    
    #[tokio::test]
    async fn test_entity_creation_with_real_services() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        
        // Create employee schema
        let employee_schema = EntitySchema {
            name: "employee".to_string(),
            description: "Employee management".to_string(),
            stored_fields: vec![
                FieldDefinition {
                    name: "first_name".to_string(),
                    display_name: "First Name".to_string(),
                    field_type: FieldType::Text { max_length: Some(100) },
                    description: "Employee's first name".to_string(),
                    is_required: true,
                    default_value: None,
                    constraints: vec![],
                    ui_hints: UIHints::default(),
                },
                FieldDefinition {
                    name: "salary".to_string(),
                    display_name: "Annual Salary".to_string(),
                    field_type: FieldType::Currency { currency_code: "USD".to_string() },
                    description: "Employee's annual salary".to_string(),
                    is_required: true,
                    default_value: None,
                    constraints: vec![
                        FieldConstraint::MinValue(30000.0),
                        FieldConstraint::MaxValue(500000.0),
                    ],
                    ui_hints: UIHints::default(),
                },
            ],
            calculated_fields: vec![
                CalculatedFieldDefinition {
                    field: CalculatedField {
                        name: "monthly_salary".to_string(),
                        display_name: "Monthly Salary".to_string(),
                        formula: "salary / 12".to_string(),
                        return_type: FieldType::Currency { currency_code: "USD".to_string() },
                        dependencies: vec!["salary".to_string()],
                        description: "Monthly salary calculation".to_string(),
                        cache_value: None,
                        cache_valid: false,
                        last_calculated: Utc::now(),
                        calculation_order: 1,
                    },
                    ui_hints: UIHints::default(),
                    update_triggers: vec![UpdateTrigger::FieldChange("salary".to_string())],
                }
            ],
            validation_rules: vec![],
            display_settings: DisplaySettings::default(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        
        // Register schema with real storage
        entity_manager.register_schema(employee_schema).await.unwrap();
        
        // Create entity with real data
        let field_values = hashmap! {
            "first_name".to_string() => EntityValue::Text("John".to_string()),
            "salary".to_string() => EntityValue::Currency { 
                amount: 85000.0, 
                currency: "USD".to_string() 
            },
        };
        
        let entity = entity_manager.create_entity(
            "employee",
            field_values,
            None
        ).await.unwrap();
        
        // Verify entity was created in real database
        let stored_entity = services.storage.load_entity(&entity.base.id).await.unwrap();
        assert_eq!(stored_entity.base.id, entity.base.id);
        assert_eq!(stored_entity.entity_type, "employee");
        
        // Verify calculated fields were computed
        assert!(entity.calculated_fields.contains_key("monthly_salary"));
        if let Some(monthly_field) = entity.calculated_fields.get("monthly_salary") {
            if let Some(EntityValue::Currency { amount, .. }) = &monthly_field.cache_value {
                assert!((amount - 7083.33).abs() < 0.01); // 85000 / 12
            } else {
                panic!("Monthly salary not calculated correctly");
            }
        }
        
        // Verify real search indexing
        let search_results = services.search_engine
            .keyword_search("John", SearchConfig::default())
            .await.unwrap();
        
        assert!(!search_results.is_empty());
        assert!(search_results.iter().any(|r| r.node_id == entity.base.id));
        
        // Cleanup
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_validation_with_real_ai() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let validation_engine = ValidationEngine::new(
            services.nlp_engine.clone(),
            services.storage.clone(),
        );
        
        // Create validation rule using natural language
        let rule_description = "VIP customers must have total purchases of at least $5000";
        let validation_rule = validation_engine
            .create_rule_from_natural_language(rule_description)
            .await.unwrap();
        
        // Test the rule with real entity
        let customer_data = hashmap! {
            "name".to_string() => EntityValue::Text("Jane Doe".to_string()),
            "status".to_string() => EntityValue::Text("VIP".to_string()),
            "total_purchases".to_string() => EntityValue::Currency { 
                amount: 3200.0, 
                currency: "USD".to_string() 
            },
        };
        
        let validation_result = validation_engine
            .validate_entity_data(&customer_data, &[validation_rule])
            .await.unwrap();
        
        // Should fail validation
        assert!(!validation_result.is_valid);
        assert!(!validation_result.violations.is_empty());
        
        let violation = &validation_result.violations[0];
        assert!(violation.message.contains("5000") || violation.message.contains("$5,000"));
        
        // Test with valid data
        let valid_customer_data = hashmap! {
            "name".to_string() => EntityValue::Text("John Smith".to_string()),
            "status".to_string() => EntityValue::Text("VIP".to_string()),
            "total_purchases".to_string() => EntityValue::Currency { 
                amount: 6500.0, 
                currency: "USD".to_string() 
            },
        };
        
        let valid_result = validation_engine
            .validate_entity_data(&valid_customer_data, &[validation_rule])
            .await.unwrap();
        
        assert!(valid_result.is_valid);
        assert!(valid_result.violations.is_empty());
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_cross_field_validation_with_real_formula_engine() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let validation_engine = ValidationEngine::new(
            services.nlp_engine.clone(),
            services.storage.clone(),
        );
        
        // Test complex cross-field validation
        let project_data = hashmap! {
            "name".to_string() => EntityValue::Text("NodeSpace Development".to_string()),
            "status".to_string() => EntityValue::Text("Completed".to_string()),
            "start_date".to_string() => EntityValue::Date(
                Utc.ymd(2024, 1, 15).and_hms(0, 0, 0)
            ),
            "end_date".to_string() => EntityValue::Date(
                Utc.ymd(2024, 6, 30).and_hms(0, 0, 0)
            ),
            "completion_percentage".to_string() => EntityValue::Number(85.0),
        };
        
        // Create validation rules using real formula engine
        let validation_rules = vec![
            ValidationRule {
                id: "project_completion_consistency".to_string(),
                field_name: "status".to_string(),
                description: "If project status is 'Completed', completion must be 100%".to_string(),
                rule_type: ValidationType::Conditional,
                expression: "status == 'Completed' IMPLIES completion_percentage == 100".to_string(),
                error_message: "Completed projects must have 100% completion".to_string(),
                severity: ValidationSeverity::Error,
            },
            ValidationRule {
                id: "project_date_consistency".to_string(),
                field_name: "end_date".to_string(),
                description: "End date must be after start date".to_string(),
                rule_type: ValidationType::CrossField,
                expression: "end_date > start_date".to_string(),
                error_message: "End date must be after start date".to_string(),
                severity: ValidationSeverity::Error,
            },
        ];
        
        let validation_result = validation_engine
            .validate_entity_data(&project_data, &validation_rules)
            .await.unwrap();
        
        // Should fail due to completion percentage
        assert!(!validation_result.is_valid);
        assert_eq!(validation_result.violations.len(), 1);
        assert!(validation_result.violations[0].message.contains("100%"));
        
        // Test with corrected data
        let mut corrected_data = project_data.clone();
        corrected_data.insert(
            "completion_percentage".to_string(), 
            EntityValue::Number(100.0)
        );
        
        let corrected_result = validation_engine
            .validate_entity_data(&corrected_data, &validation_rules)
            .await.unwrap();
        
        assert!(corrected_result.is_valid);
        
        services.cleanup().await.unwrap();
    }
}
```

### AI Integration Testing

```rust
#[cfg(test)]
mod ai_integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_intent_classification_with_real_nlp() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let intent_classifier = IntentClassifier::new(services.nlp_engine.clone());
        
        let test_cases = vec![
            ("Show me all employees in engineering", ChatIntent::EntityCRUD),
            ("Create a task for code review", ChatIntent::NodeManipulation),
            ("What is our deployment process?", ChatIntent::RAGQuery),
            ("Draft an onboarding checklist", ChatIntent::ContentGeneration),
            ("Why can't I set this user as VIP?", ChatIntent::ValidationQuery),
            ("What is machine learning?", ChatIntent::GeneralKnowledge),
            ("How many PDF files do I have?", ChatIntent::SystemQuery),
        ];
        
        for (message, expected_intent) in test_cases {
            let context = ConversationContext::default();
            let classification = intent_classifier
                .classify_message(message, &context)
                .await.unwrap();
            
            assert_eq!(classification.intent, expected_intent, 
                      "Failed to classify: '{}'. Expected: {:?}, Got: {:?}", 
                      message, expected_intent, classification.intent);
            assert!(classification.confidence > 0.5, 
                   "Low confidence for: '{}'", message);
        }
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_rag_processing_with_real_knowledge_base() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let rag_processor = RAGProcessor::new(
            services.search_engine.clone(),
            services.nlp_engine.clone(),
        );
        
        // Create test knowledge base content
        let test_documents = vec![
            ("Deployment Process", "Our deployment process involves three stages: development, staging, and production. We use GitHub Actions for CI/CD."),
            ("Code Review Guidelines", "All code must be reviewed by at least two team members. Use meaningful commit messages and ensure tests pass."),
            ("NodeSpace Architecture", "NodeSpace uses a Rust backend with Tauri for the desktop application. The frontend is built with Svelte."),
        ];
        
        // Index documents in real search engine
        for (title, content) in &test_documents {
            let node_id = uuid::Uuid::new_v4().to_string();
            services.search_engine.index_document(&node_id, content).await.unwrap();
            
            // Also create embeddings
            let embedding = services.nlp_engine.embed_text(content).await.unwrap();
            services.vector_db.save_embedding(&node_id, &embedding, hashmap! {
                "title".to_string() => title.to_string(),
                "content_type".to_string() => "document".to_string(),
            }).await.unwrap();
        }
        
        // Test RAG queries
        let test_queries = vec![
            ("What is our deployment process?", "deployment"),
            ("How do we do code reviews?", "review"),
            ("What technology does NodeSpace use?", "Rust"),
        ];
        
        for (query, expected_keyword) in test_queries {
            let context = ConversationContext::default();
            let response = rag_processor.process_rag_query(query, &context).await.unwrap();
            
            assert!(!response.answer.is_empty(), "Empty response for: {}", query);
            assert!(response.answer.to_lowercase().contains(expected_keyword), 
                   "Response doesn't contain '{}' for query: '{}'\nResponse: {}", 
                   expected_keyword, query, response.answer);
            assert!(!response.sources.is_empty(), "No sources for: {}", query);
            assert!(response.confidence > 0.3, "Low confidence for: {}", query);
        }
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_content_generation_with_real_ai() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let content_generator = ContentGenerator::new(
            services.nlp_engine.clone(),
            Arc::new(NodeFactory::new(services.storage.clone())),
        );
        
        let generation_request = ContentGenerationRequest {
            description: "Create an employee onboarding checklist".to_string(),
            style: ContentStyle::Professional,
            target_length: Some(300),
            format: Some("markdown".to_string()),
            context_nodes: None,
        };
        
        let context = ConversationContext::default();
        let response = content_generator
            .generate_content(&generation_request, &context)
            .await.unwrap();
        
        // Verify content quality
        assert!(!response.content.text.is_empty());
        assert!(response.content.text.len() > 100, "Generated content too short");
        assert!(response.content.text.to_lowercase().contains("onboarding") || 
               response.content.text.to_lowercase().contains("checklist"));
        
        // Verify node suggestions
        assert!(!response.node_suggestions.is_empty());
        
        // Test node creation from suggestions
        let created_nodes = content_generator
            .create_nodes_from_suggestions(&response.node_suggestions, None)
            .await.unwrap();
        
        assert!(!created_nodes.is_empty());
        
        // Verify nodes were actually created in storage
        for node_id in &created_nodes {
            let node = services.storage.load_node(node_id).await.unwrap();
            assert!(!node.content.is_empty());
        }
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_entity_crud_with_natural_language() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        let crud_processor = EntityCRUDProcessor::new(
            entity_manager.clone(),
            services.nlp_engine.clone(),
        );
        
        // Register employee schema
        let employee_schema = create_test_employee_schema();
        entity_manager.register_schema(employee_schema).await.unwrap();
        
        // Test entity creation via natural language
        let creation_message = "Add a new employee named Sarah Johnson, she's a Senior Engineer with a salary of $95,000";
        let context = ConversationContext::default();
        
        let response = crud_processor
            .process_entity_crud(creation_message, &context)
            .await.unwrap();
        
        match response {
            EntityCRUDResponse::Success { affected_entities, .. } => {
                assert_eq!(affected_entities.len(), 1);
                
                // Verify entity was actually created
                let entity = services.storage.load_entity(&affected_entities[0]).await.unwrap();
                assert_eq!(entity.entity_type, "employee");
                
                // Verify extracted fields
                if let Some(EntityValue::Text(name)) = entity.stored_fields.get("first_name") {
                    assert_eq!(name, "Sarah");
                }
                if let Some(EntityValue::Text(last_name)) = entity.stored_fields.get("last_name") {
                    assert_eq!(last_name, "Johnson");
                }
                if let Some(EntityValue::Currency { amount, .. }) = entity.stored_fields.get("salary") {
                    assert_eq!(*amount, 95000.0);
                }
            },
            other => panic!("Expected Success, got: {:?}", other),
        }
        
        // Test entity query
        let query_message = "Show me all engineers earning over $90,000";
        let query_response = crud_processor
            .process_entity_crud(query_message, &context)
            .await.unwrap();
        
        // Should find our created employee
        match query_response {
            EntityCRUDResponse::Success { affected_entities, .. } => {
                assert!(!affected_entities.is_empty());
            },
            other => panic!("Expected Success for query, got: {:?}", other),
        }
        
        services.cleanup().await.unwrap();
    }
}
```

---

## Integration Testing

### Real-Time System Testing

```rust
#[cfg(test)]
mod real_time_integration_tests {
    use super::*;
    use tokio::time::{sleep, Duration};
    use std::sync::atomic::{AtomicUsize, Ordering};
    
    #[tokio::test]
    async fn test_query_real_time_updates_with_real_services() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let query_manager = QueryResultManager::new(
            services.storage.clone(),
            services.search_engine.clone(),
        );
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        
        // Register task schema
        let task_schema = create_test_task_schema();
        entity_manager.register_schema(task_schema).await.unwrap();
        
        // Create query for tasks due this week
        let query_node = QueryNode {
            base: create_test_text_node("tasks_due_this_week"),
            query: QueryDefinition {
                name: "Tasks Due This Week".to_string(),
                description: "Show all tasks due in the next 7 days".to_string(),
                entity_types: vec!["task".to_string()],
                filters: vec![
                    QueryFilter {
                        field_name: "due_date".to_string(),
                        operator: FilterOperator::Between,
                        value: QueryValue::Dynamic("TODAY() AND TODAY() + 7".to_string()),
                        case_sensitive: false,
                        negate: false,
                    },
                    QueryFilter {
                        field_name: "status".to_string(),
                        operator: FilterOperator::NotEquals,
                        value: QueryValue::Static(EntityValue::Text("completed".to_string())),
                        case_sensitive: false,
                        negate: false,
                    },
                ],
                sort_by: vec![SortCriteria {
                    field: "due_date".to_string(),
                    direction: SortDirection::Ascending,
                }],
                limit: None,
                offset: None,
                include_calculated_fields: vec![],
                search_text: None,
                date_range: None,
                aggregations: vec![],
            },
            view_calculated_fields: vec![],
            results: vec![],
            last_executed: Utc::now(),
            auto_refresh: true,
            refresh_triggers: vec![
                RefreshTrigger::FieldChange { 
                    entity_type: "task".to_string(), 
                    field: "status".to_string(),
                    condition: None,
                },
                RefreshTrigger::NodeCreate { entity_type: "task".to_string() },
            ],
            subscription_id: uuid::Uuid::new_v4().to_string(),
            performance_metrics: QueryPerformanceMetrics::default(),
        };
        
        // Register query
        query_manager.register_query(&query_node, &services).await.unwrap();
        
        // Set up update counter for testing
        let update_count = Arc::new(AtomicUsize::new(0));
        let update_count_clone = update_count.clone();
        
        // Subscribe to updates
        let (tx, mut rx) = tokio::sync::mpsc::channel(10);
        query_manager.add_subscriber(&query_node.id, QuerySubscriber {
            subscriber_id: "test_subscriber".to_string(),
            subscriber_type: SubscriberType::UIComponent { 
                component_id: "test_component".to_string() 
            },
            notification_preferences: NotificationPreferences::default(),
            last_notified: Utc::now(),
        }).await.unwrap();
        
        // Start listening for updates
        let update_listener = tokio::spawn(async move {
            while let Some(update) = rx.recv().await {
                update_count_clone.fetch_add(1, Ordering::SeqCst);
                println!("Received query update: {:?}", update);
            }
        });
        
        // Create tasks that should trigger updates
        let task_data_1 = hashmap! {
            "title".to_string() => EntityValue::Text("Implement real-time updates".to_string()),
            "due_date".to_string() => EntityValue::Date(Utc::now() + chrono::Duration::days(3)),
            "status".to_string() => EntityValue::Text("pending".to_string()),
            "priority".to_string() => EntityValue::Text("high".to_string()),
        };
        
        let task_1 = entity_manager.create_entity(
            "task",
            task_data_1,
            None
        ).await.unwrap();
        
        // Wait for update propagation
        sleep(Duration::from_millis(100)).await;
        
        // Create another task
        let task_data_2 = hashmap! {
            "title".to_string() => EntityValue::Text("Write integration tests".to_string()),
            "due_date".to_string() => EntityValue::Date(Utc::now() + chrono::Duration::days(5)),
            "status".to_string() => EntityValue::Text("pending".to_string()),
            "priority".to_string() => EntityValue::Text("medium".to_string()),
        };
        
        let task_2 = entity_manager.create_entity(
            "task",
            task_data_2,
            None
        ).await.unwrap();
        
        sleep(Duration::from_millis(100)).await;
        
        // Update task status (should trigger update)
        entity_manager.update_entity_field(
            &task_1.base.id,
            "status",
            EntityValue::Text("completed".to_string())
        ).await.unwrap();
        
        sleep(Duration::from_millis(100)).await;
        
        // Verify updates were received
        let final_update_count = update_count.load(Ordering::SeqCst);
        assert!(final_update_count >= 2, 
               "Expected at least 2 updates, got {}", final_update_count);
        
        // Verify query results are correct
        let current_results = query_manager.get_query_results(&query_node.id).await.unwrap();
        
        // Should contain task_2 but not task_1 (completed)
        assert_eq!(current_results.len(), 1);
        assert_eq!(current_results[0], task_2.base.id);
        
        update_listener.abort();
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_calculated_field_updates_propagation() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        
        // Create employee schema with calculated fields
        let employee_schema = EntitySchema {
            name: "employee".to_string(),
            description: "Employee management with calculations".to_string(),
            stored_fields: vec![
                create_field_def("first_name", FieldType::Text { max_length: Some(100) }, true),
                create_field_def("last_name", FieldType::Text { max_length: Some(100) }, true),
                create_field_def("salary", FieldType::Currency { currency_code: "USD".to_string() }, true),
                create_field_def("bonus", FieldType::Currency { currency_code: "USD".to_string() }, false),
            ],
            calculated_fields: vec![
                CalculatedFieldDefinition {
                    field: CalculatedField {
                        name: "full_name".to_string(),
                        display_name: "Full Name".to_string(),
                        formula: "first_name + ' ' + last_name".to_string(),
                        return_type: FieldType::Text { max_length: None },
                        dependencies: vec!["first_name".to_string(), "last_name".to_string()],
                        description: "Employee's full name".to_string(),
                        cache_value: None,
                        cache_valid: false,
                        last_calculated: Utc::now(),
                        calculation_order: 1,
                    },
                    ui_hints: UIHints::default(),
                    update_triggers: vec![
                        UpdateTrigger::FieldChange("first_name".to_string()),
                        UpdateTrigger::FieldChange("last_name".to_string()),
                    ],
                },
                CalculatedFieldDefinition {
                    field: CalculatedField {
                        name: "total_compensation".to_string(),
                        display_name: "Total Compensation".to_string(),
                        formula: "salary + COALESCE(bonus, 0)".to_string(),
                        return_type: FieldType::Currency { currency_code: "USD".to_string() },
                        dependencies: vec!["salary".to_string(), "bonus".to_string()],
                        description: "Total annual compensation".to_string(),
                        cache_value: None,
                        cache_valid: false,
                        last_calculated: Utc::now(),
                        calculation_order: 2,
                    },
                    ui_hints: UIHints::default(),
                    update_triggers: vec![
                        UpdateTrigger::FieldChange("salary".to_string()),
                        UpdateTrigger::FieldChange("bonus".to_string()),
                    ],
                },
            ],
            validation_rules: vec![],
            display_settings: DisplaySettings::default(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        
        entity_manager.register_schema(employee_schema).await.unwrap();
        
        // Create employee
        let employee_data = hashmap! {
            "first_name".to_string() => EntityValue::Text("John".to_string()),
            "last_name".to_string() => EntityValue::Text("Doe".to_string()),
            "salary".to_string() => EntityValue::Currency { 
                amount: 85000.0, 
                currency: "USD".to_string() 
            },
            "bonus".to_string() => EntityValue::Currency { 
                amount: 15000.0, 
                currency: "USD".to_string() 
            },
        };
        
        let employee = entity_manager.create_entity(
            "employee",
            employee_data,
            None
        ).await.unwrap();
        
        // Verify initial calculated fields
        assert_eq!(
            employee.get_calculated_field_value("full_name").unwrap(),
            EntityValue::Text("John Doe".to_string())
        );
        assert_eq!(
            employee.get_calculated_field_value("total_compensation").unwrap(),
            EntityValue::Currency { amount: 100000.0, currency: "USD".to_string() }
        );
        
        // Update salary and verify cascade
        let update_result = entity_manager.update_entity_field(
            &employee.base.id,
            "salary",
            EntityValue::Currency { amount: 95000.0, currency: "USD".to_string() }
        ).await.unwrap();
        
        // Verify calculated field was updated
        assert!(!update_result.calculated_field_updates.is_empty());
        
        let total_comp_update = update_result.calculated_field_updates.iter()
            .find(|u| u.field_name == "total_compensation")
            .expect("total_compensation should be updated");
        
        assert_eq!(
            total_comp_update.new_value,
            EntityValue::Currency { amount: 110000.0, currency: "USD".to_string() }
        );
        
        // Verify in storage
        let updated_employee = services.storage.load_entity(&employee.base.id).await.unwrap();
        assert_eq!(
            updated_employee.get_calculated_field_value("total_compensation").unwrap(),
            EntityValue::Currency { amount: 110000.0, currency: "USD".to_string() }
        );
        
        services.cleanup().await.unwrap();
    }
}
```

### Plugin Testing with Real Services

```rust
#[cfg(test)]
mod plugin_integration_tests {
    use super::*;
    use nodespace_pdf_node::PDFPlugin;
    
    #[tokio::test]
    async fn test_pdf_plugin_full_integration() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let plugin_services = PluginServices {
            storage: services.storage.clone(),
            nlp_engine: services.nlp_engine.clone(),
            search_engine: services.search_engine.clone(),
            text_processor: Arc::new(TextProcessor::new()),
            validation_engine: services.validation_engine.clone(),
        };
        
        let pdf_plugin = PDFPlugin::new(plugin_services);
        
        // Create test PDF file
        let test_pdf_path = "/tmp/test_document.pdf";
        let pdf_content = create_test_pdf_with_content(
            "NodeSpace Testing Strategy",
            vec![
                "This document outlines our comprehensive testing approach.",
                "We use real services rather than mocks for integration testing.",
                "All AI components are tested with actual NLP models.",
            ]
        );
        tokio::fs::write(test_pdf_path, pdf_content).await.unwrap();
        
        // Process PDF through plugin
        let pdf_node = pdf_plugin.create_from_file(test_pdf_path, None).await.unwrap();
        
        // Verify PDF processing
        assert_eq!(pdf_node.node_type(), "pdf");
        assert!(pdf_node.page_count > 0);
        assert!(!pdf_node.extracted_text.is_empty());
        assert!(pdf_node.extracted_text.contains("NodeSpace Testing Strategy"));
        
        // Verify real storage integration
        let stored_node = services.storage.load_node(&pdf_node.base.id).await.unwrap();
        assert_eq!(stored_node.base.id, pdf_node.base.id);
        
        // Verify real embeddings were generated
        let embeddings = services.vector_db.get_embeddings(&pdf_node.base.id).await.unwrap();
        assert!(!embeddings.is_empty());
        
        // Test different embedding levels
        let doc_embedding = embeddings.iter()
            .find(|e| e.metadata.get("level") == Some(&"document".to_string()))
            .expect("Document-level embedding should exist");
        
        let page_embeddings: Vec<_> = embeddings.iter()
            .filter(|e| e.metadata.get("level") == Some(&"page".to_string()))
            .collect();
        assert!(!page_embeddings.is_empty());
        
        // Verify real search integration
        let search_results = services.search_engine
            .similarity_search(&doc_embedding.embedding, SearchConfig::default())
            .await.unwrap();
        
        assert!(!search_results.is_empty());
        let top_result = &search_results[0];
        assert_eq!(top_result.node_id, pdf_node.base.id);
        assert!(top_result.content.contains("NodeSpace"));
        
        // Test keyword search
        let keyword_results = services.search_engine
            .keyword_search("testing strategy", SearchConfig::default())
            .await.unwrap();
        
        assert!(!keyword_results.is_empty());
        assert!(keyword_results.iter().any(|r| r.node_id == pdf_node.base.id));
        
        // Test validation integration
        let validation_result = pdf_plugin.validate_pdf_node(&pdf_node).await.unwrap();
        assert!(validation_result.is_valid);
        
        // Test with invalid PDF (missing file)
        let mut invalid_node = pdf_node.clone();
        invalid_node.file_path = "/nonexistent/file.pdf".to_string();
        
        let invalid_result = pdf_plugin.validate_pdf_node(&invalid_node).await.unwrap();
        assert!(!invalid_result.is_valid);
        assert!(!invalid_result.violations.is_empty());
        
        // Cleanup
        tokio::fs::remove_file(test_pdf_path).await.unwrap();
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_pdf_search_integration_with_real_ai() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let plugin_services = PluginServices {
            storage: services.storage.clone(),
            nlp_engine: services.nlp_engine.clone(),
            search_engine: services.search_engine.clone(),
            text_processor: Arc::new(TextProcessor::new()),
            validation_engine: services.validation_engine.clone(),
        };
        
        let pdf_plugin = PDFPlugin::new(plugin_services);
        
        // Create multiple PDF documents with different content
        let test_pdfs = vec![
            ("Machine Learning Basics", "Introduction to machine learning algorithms and neural networks."),
            ("Rust Programming Guide", "Comprehensive guide to Rust programming language and memory safety."),
            ("NodeSpace Architecture", "Technical specification for NodeSpace system architecture."),
        ];
        
        let mut created_nodes = Vec::new();
        
        for (title, content) in &test_pdfs {
            let pdf_path = format!("/tmp/{}.pdf", title.replace(" ", "_"));
            let pdf_content = create_test_pdf_with_content(title, vec![content]);
            tokio::fs::write(&pdf_path, pdf_content).await.unwrap();
            
            let pdf_node = pdf_plugin.create_from_file(&pdf_path, None).await.unwrap();
            created_nodes.push((pdf_node, pdf_path));
        }
        
        // Wait for indexing to complete
        sleep(Duration::from_millis(500)).await;
        
        // Test semantic search
        let ml_query = "artificial intelligence and neural networks";
        let ml_embedding = services.nlp_engine.embed_query(ml_query).await.unwrap();
        let ml_results = services.search_engine
            .similarity_search(&ml_embedding, SearchConfig {
                max_results: 10,
                min_similarity: 0.3,
                ..Default::default()
            })
            .await.unwrap();
        
        assert!(!ml_results.is_empty());
        
        // Should find machine learning document
        let ml_result = ml_results.iter()
            .find(|r| r.content.to_lowercase().contains("machine learning"))
            .expect("Should find ML document");
        
        assert!(ml_result.similarity_score > 0.5);
        
        // Test keyword search
        let rust_results = services.search_engine
            .keyword_search("Rust programming", SearchConfig::default())
            .await.unwrap();
        
        assert!(!rust_results.is_empty());
        let rust_result = rust_results.iter()
            .find(|r| r.content.contains("Rust"))
            .expect("Should find Rust document");
        
        // Test architecture search
        let arch_results = services.search_engine
            .keyword_search("NodeSpace", SearchConfig::default())
            .await.unwrap();
        
        assert!(!arch_results.is_empty());
        
        // Cleanup
        for (_, pdf_path) in &created_nodes {
            tokio::fs::remove_file(pdf_path).await.ok();
        }
        services.cleanup().await.unwrap();
    }
}
```

---

## End-to-End Testing

### Complete User Workflow Testing

```rust
#[cfg(test)]
mod end_to_end_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_complete_knowledge_management_workflow() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        
        // Initialize all systems
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        let ai_services = AIServices::new(services.clone());
        let plugin_registry = PluginRegistry::new();
        
        // Step 1: User uploads a PDF document
        let pdf_plugin = PDFPlugin::new(PluginServices {
            storage: services.storage.clone(),
            nlp_engine: services.nlp_engine.clone(),
            search_engine: services.search_engine.clone(),
            text_processor: Arc::new(TextProcessor::new()),
            validation_engine: services.validation_engine.clone(),
        });
        
        let test_pdf_path = "/tmp/project_requirements.pdf";
        let pdf_content = create_test_pdf_with_content(
            "Project Requirements Document",
            vec![
                "The NodeSpace project requires implementation of the following features:",
                "1. Real-time collaboration capabilities",
                "2. AI-powered content generation", 
                "3. Advanced search functionality",
                "4. Plugin architecture for extensibility",
            ]
        );
        tokio::fs::write(test_pdf_path, pdf_content).await.unwrap();
        
        let pdf_node = pdf_plugin.create_from_file(test_pdf_path, None).await.unwrap();
        
        // Step 2: User asks AI to analyze the document
        let mut ai_chat = AIChatNode {
            base: create_test_text_node("project_analysis_chat"),
            conversation: Vec::new(),
            context_sources: Vec::new(),
            intent: ChatIntent::RAGQuery,
            generated_content: Vec::new(),
        };
        
        let analysis_query = "What are the main requirements mentioned in the project document?";
        let analysis_response = ai_chat.process_message(analysis_query, &ai_services).await.unwrap();
        
        // Verify AI found the content
        assert!(analysis_response.content.to_lowercase().contains("real-time"));
        assert!(analysis_response.content.to_lowercase().contains("ai-powered"));
        assert!(analysis_response.content.to_lowercase().contains("search"));
        assert!(analysis_response.content.to_lowercase().contains("plugin"));
        
        // Step 3: User asks AI to create project entities
        let entity_creation_request = "Create a project called 'NodeSpace Development' with the requirements from the document as tasks";
        let creation_response = ai_chat.process_message(entity_creation_request, &ai_services).await.unwrap();
        
        // Verify entities were created
        let projects = entity_manager.query_entities("project", EntityQuery {
            filters: vec![
                QueryFilter {
                    field_name: "name".to_string(),
                    operator: FilterOperator::Contains,
                    value: QueryValue::Static(EntityValue::Text("NodeSpace".to_string())),
                    case_sensitive: false,
                    negate: false,
                }
            ],
            sort_by: vec![],
            limit: None,
            offset: None,
            include_calculated_fields: vec![],
            search_text: None,
        }).await.unwrap();
        
        assert!(!projects.entities.is_empty());
        let project = &projects.entities[0];
        
        // Step 4: User creates a query to track project tasks
        let task_query = QueryNode {
            base: create_test_text_node("project_tasks_query"),
            query: QueryDefinition {
                name: "NodeSpace Project Tasks".to_string(),
                description: "All tasks for NodeSpace project".to_string(),
                entity_types: vec!["task".to_string()],
                filters: vec![
                    QueryFilter {
                        field_name: "project_id".to_string(),
                        operator: FilterOperator::Equals,
                        value: QueryValue::Static(EntityValue::Reference(project.base.id.clone())),
                        case_sensitive: false,
                        negate: false,
                    },
                    QueryFilter {
                        field_name: "status".to_string(),
                        operator: FilterOperator::NotEquals,
                        value: QueryValue::Static(EntityValue::Text("completed".to_string())),
                        case_sensitive: false,
                        negate: false,
                    },
                ],
                sort_by: vec![SortCriteria {
                    field: "priority".to_string(),
                    direction: SortDirection::Descending,
                }],
                limit: None,
                offset: None,
                include_calculated_fields: vec!["urgency_score".to_string()],
                search_text: None,
                date_range: None,
                aggregations: vec![],
            },
            view_calculated_fields: vec![
                ViewCalculatedField {
                    name: "urgency_score".to_string(),
                    formula: "CASE WHEN priority = 'high' THEN 3 WHEN priority = 'medium' THEN 2 ELSE 1 END".to_string(),
                    return_type: FieldType::Number { decimal_places: Some(0) },
                }
            ],
            results: vec![],
            last_executed: Utc::now(),
            auto_refresh: true,
            refresh_triggers: vec![
                RefreshTrigger::FieldChange { 
                    entity_type: "task".to_string(), 
                    field: "status".to_string(),
                    condition: None,
                },
            ],
            subscription_id: uuid::Uuid::new_v4().to_string(),
            performance_metrics: QueryPerformanceMetrics::default(),
        };
        
        let query_manager = QueryResultManager::new(
            services.storage.clone(),
            services.search_engine.clone(),
        );
        query_manager.register_query(&task_query, &services).await.unwrap();
        
        // Step 5: User updates task status and verifies real-time updates
        let tasks = entity_manager.query_entities("task", EntityQuery {
            filters: vec![
                QueryFilter {
                    field_name: "project_id".to_string(),
                    operator: FilterOperator::Equals,
                    value: QueryValue::Static(EntityValue::Reference(project.base.id.clone())),
                    case_sensitive: false,
                    negate: false,
                }
            ],
            sort_by: vec![],
            limit: None,
            offset: None,
            include_calculated_fields: vec![],
            search_text: None,
        }).await.unwrap();
        
        assert!(!tasks.entities.is_empty());
        let first_task = &tasks.entities[0];
        
        // Update task status
        entity_manager.update_entity_field(
            &first_task.base.id,
            "status",
            EntityValue::Text("in_progress".to_string())
        ).await.unwrap();
        
        // Verify query was updated
        sleep(Duration::from_millis(100)).await;
        let updated_results = query_manager.get_query_results(&task_query.id).await.unwrap();
        
        // Should still include the task (status changed to in_progress, not completed)
        assert!(updated_results.iter().any(|id| id == &first_task.base.id));
        
        // Step 6: User asks for project status via AI
        let status_query = "What's the current status of the NodeSpace project?";
        let status_response = ai_chat.process_message(status_query, &ai_services).await.unwrap();
        
        // Should mention the project and task status
        assert!(status_response.content.to_lowercase().contains("nodespace"));
        assert!(status_response.content.to_lowercase().contains("progress") || 
               status_response.content.to_lowercase().contains("status"));
        
        // Step 7: User searches across all content
        let search_query = "real-time collaboration";
        let search_embedding = services.nlp_engine.embed_query(search_query).await.unwrap();
        let search_results = services.search_engine
            .similarity_search(&search_embedding, SearchConfig::default())
            .await.unwrap();
        
        // Should find both PDF content and task content
        assert!(!search_results.is_empty());
        assert!(search_results.iter().any(|r| r.node_id == pdf_node.base.id));
        
        // Cleanup
        tokio::fs::remove_file(test_pdf_path).await.ok();
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_validation_workflow_with_ai_assistance() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let entity_manager = EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        );
        let conversational_validator = ConversationalValidator::new(
            services.validation_engine.clone(),
            services.nlp_engine.clone(),
        );
        
        // Create customer schema with business rules
        let customer_schema = EntitySchema {
            name: "customer".to_string(),
            description: "Customer management".to_string(),
            stored_fields: vec![
                create_field_def("name", FieldType::Text { max_length: Some(200) }, true),
                create_field_def("email", FieldType::Email, true),
                create_field_def("status", FieldType::Enum { 
                    options: vec![
                        EnumOption { value: "regular".to_string(), label: "Regular".to_string() },
                        EnumOption { value: "premium".to_string(), label: "Premium".to_string() },
                        EnumOption { value: "vip".to_string(), label: "VIP".to_string() },
                    ], 
                    allow_custom: false 
                }, true),
                create_field_def("total_purchases", FieldType::Currency { currency_code: "USD".to_string() }, true),
            ],
            calculated_fields: vec![],
            validation_rules: vec![
                ValidationRule {
                    id: "vip_purchase_requirement".to_string(),
                    field_name: "status".to_string(),
                    description: "VIP customers must have at least $5000 in total purchases".to_string(),
                    rule_type: ValidationType::Conditional,
                    expression: "status == 'vip' IMPLIES total_purchases >= 5000".to_string(),
                    error_message: "VIP status requires at least $5,000 in total purchases".to_string(),
                    severity: ValidationSeverity::Error,
                },
            ],
            display_settings: DisplaySettings::default(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
        };
        
        entity_manager.register_schema(customer_schema).await.unwrap();
        
        // Step 1: User tries to create VIP customer with insufficient purchases
        let invalid_customer_data = hashmap! {
            "name".to_string() => EntityValue::Text("John Smith".to_string()),
            "email".to_string() => EntityValue::Email("john@example.com".to_string()),
            "status".to_string() => EntityValue::Text("vip".to_string()),
            "total_purchases".to_string() => EntityValue::Currency { 
                amount: 3200.0, 
                currency: "USD".to_string() 
            },
        };
        
        let creation_result = entity_manager.create_entity(
            "customer",
            invalid_customer_data,
            None
        ).await;
        
        // Should fail validation
        assert!(creation_result.is_err());
        
        if let Err(Error::ValidationFailed(violations)) = creation_result {
            // Step 2: Use conversational validator to explain the error
            let validation_result = ValidationResult {
                is_valid: false,
                violations: violations.clone(),
                warnings: vec![],
            };
            
            let original_request = "Create VIP customer John Smith with $3,200 in purchases";
            let context = ConversationContext::default();
            
            let conversational_response = conversational_validator
                .handle_validation_error(&validation_result, original_request, &context)
                .await.unwrap();
            
            match conversational_response {
                ConversationalValidationResponse::ValidationError { 
                    explanation, 
                    resolution_options,
                    .. 
                } => {
                    // Verify explanation is user-friendly
                    assert!(explanation.contains("$5,000") || explanation.contains("5000"));
                    assert!(explanation.to_lowercase().contains("vip"));
                    assert!(explanation.to_lowercase().contains("purchases"));
                    
                    // Verify resolution options were provided
                    assert!(!resolution_options.is_empty());
                    
                    // Find option to increase purchases
                    let increase_option = resolution_options.iter()
                        .find(|opt| opt.title.to_lowercase().contains("increase") || 
                                    opt.title.to_lowercase().contains("purchase"))
                        .expect("Should have option to increase purchases");
                    
                    assert!(increase_option.confidence > 0.7);
                },
                other => panic!("Expected ValidationError, got: {:?}", other),
            }
        } else {
            panic!("Expected validation failure");
        }
        
        // Step 3: User fixes the data based on AI suggestion
        let corrected_customer_data = hashmap! {
            "name".to_string() => EntityValue::Text("John Smith".to_string()),
            "email".to_string() => EntityValue::Email("john@example.com".to_string()),
            "status".to_string() => EntityValue::Text("vip".to_string()),
            "total_purchases".to_string() => EntityValue::Currency { 
                amount: 6500.0, 
                currency: "USD".to_string() 
            },
        };
        
        let corrected_result = entity_manager.create_entity(
            "customer",
            corrected_customer_data,
            None
        ).await.unwrap();
        
        // Should succeed
        assert_eq!(corrected_result.entity_type, "customer");
        
        // Verify in storage
        let stored_customer = services.storage.load_entity(&corrected_result.base.id).await.unwrap();
        assert_eq!(stored_customer.base.id, corrected_result.base.id);
        
        services.cleanup().await.unwrap();
    }
}
```

---

## Performance Testing

### Load Testing with Real Services

```rust
#[cfg(test)]
mod performance_tests {
    use super::*;
    use std::time::Instant;
    use tokio::time::Duration;
    
    #[tokio::test]
    async fn test_concurrent_entity_operations() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let entity_manager = Arc::new(EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        ));
        
        // Register test schema
        let schema = create_test_employee_schema();
        entity_manager.register_schema(schema).await.unwrap();
        
        let start_time = Instant::now();
        let num_operations = 100;
        
        // Create many concurrent entity operations
        let creation_tasks: Vec<_> = (0..num_operations).map(|i| {
            let entity_manager = entity_manager.clone();
            tokio::spawn(async move {
                let employee_data = hashmap! {
                    "first_name".to_string() => EntityValue::Text(format!("Employee{}", i)),
                    "last_name".to_string() => EntityValue::Text("Test".to_string()),
                    "salary".to_string() => EntityValue::Currency { 
                        amount: 50000.0 + (i as f64 * 1000.0), 
                        currency: "USD".to_string() 
                    },
                    "department".to_string() => EntityValue::Text(
                        if i % 3 == 0 { "Engineering" } 
                        else if i % 3 == 1 { "Sales" } 
                        else { "Marketing" }.to_string()
                    ),
                };
                
                entity_manager.create_entity("employee", employee_data, None).await
            })
        }).collect();
        
        // Wait for all operations to complete
        let results = futures::future::join_all(creation_tasks).await;
        
        let creation_time = start_time.elapsed();
        
        // Verify all operations succeeded
        let successful_creations: Vec<_> = results.into_iter()
            .filter_map(|r| r.ok().and_then(|r| r.ok()))
            .collect();
        
        assert_eq!(successful_creations.len(), num_operations);
        
        // Performance assertions
        assert!(creation_time < Duration::from_secs(30), 
               "Creation took too long: {:?}", creation_time);
        
        let ops_per_second = num_operations as f64 / creation_time.as_secs_f64();
        println!("Entity creation rate: {:.2} ops/second", ops_per_second);
        assert!(ops_per_second > 3.0, "Entity creation too slow: {:.2} ops/sec", ops_per_second);
        
        // Test concurrent updates
        let update_start = Instant::now();
        let update_tasks: Vec<_> = successful_creations.iter().enumerate().map(|(i, entity)| {
            let entity_manager = entity_manager.clone();
            let entity_id = entity.base.id.clone();
            tokio::spawn(async move {
                entity_manager.update_entity_field(
                    &entity_id,
                    "salary",
                    EntityValue::Currency { 
                        amount: 60000.0 + (i as f64 * 500.0), 
                        currency: "USD".to_string() 
                    }
                ).await
            })
        }).collect();
        
        let update_results = futures::future::join_all(update_tasks).await;
        let update_time = update_start.elapsed();
        
        let successful_updates: Vec<_> = update_results.into_iter()
            .filter_map(|r| r.ok().and_then(|r| r.ok()))
            .collect();
        
        assert_eq!(successful_updates.len(), num_operations);
        
        let update_ops_per_second = num_operations as f64 / update_time.as_secs_f64();
        println!("Entity update rate: {:.2} ops/second", update_ops_per_second);
        assert!(update_ops_per_second > 5.0, "Entity updates too slow: {:.2} ops/sec", update_ops_per_second);
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_search_performance_with_large_dataset() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        
        // Create large dataset
        let num_documents = 1000;
        let document_creation_start = Instant::now();
        
        let creation_tasks: Vec<_> = (0..num_documents).map(|i| {
            let search_engine = services.search_engine.clone();
            let nlp_engine = services.nlp_engine.clone();
            tokio::spawn(async move {
                let content = format!(
                    "Document {} about {} containing information on {} and {}. This document discusses {} in detail.",
                    i,
                    if i % 5 == 0 { "machine learning" } else { "software engineering" },
                    if i % 3 == 0 { "algorithms" } else { "architecture" },
                    if i % 4 == 0 { "performance" } else { "scalability" },
                    if i % 2 == 0 { "real-time systems" } else { "data processing" }
                );
                
                let node_id = format!("doc_{}", i);
                
                // Index for keyword search
                search_engine.index_document(&node_id, &content).await?;
                
                // Create embedding for semantic search
                let embedding = nlp_engine.embed_text(&content).await?;
                services.vector_db.save_embedding(&node_id, &embedding, hashmap! {
                    "document_type".to_string() => "test_doc".to_string(),
                    "index".to_string() => i.to_string(),
                }).await?;
                
                Ok::<(), Error>(())
            })
        }).collect();
        
        // Wait for all documents to be indexed
        let creation_results = futures::future::join_all(creation_tasks).await;
        let successful_creations = creation_results.into_iter()
            .filter(|r| r.is_ok() && r.as_ref().unwrap().is_ok())
            .count();
        
        let creation_time = document_creation_start.elapsed();
        println!("Indexed {} documents in {:?}", successful_creations, creation_time);
        assert_eq!(successful_creations, num_documents);
        
        // Test search performance
        let search_queries = vec![
            "machine learning algorithms",
            "software architecture patterns", 
            "real-time performance optimization",
            "data processing scalability",
        ];
        
        for query in &search_queries {
            // Test keyword search performance
            let keyword_start = Instant::now();
            let keyword_results = services.search_engine
                .keyword_search(query, SearchConfig {
                    max_results: 50,
                    fuzzy_matching: true,
                    ..Default::default()
                })
                .await.unwrap();
            let keyword_time = keyword_start.elapsed();
            
            assert!(!keyword_results.is_empty(), "No keyword results for: {}", query);
            assert!(keyword_time < Duration::from_millis(500), 
                   "Keyword search too slow for '{}': {:?}", query, keyword_time);
            
            // Test semantic search performance
            let semantic_start = Instant::now();
            let query_embedding = services.nlp_engine.embed_query(query).await.unwrap();
            let semantic_results = services.search_engine
                .similarity_search(&query_embedding, SearchConfig {
                    max_results: 50,
                    min_similarity: 0.3,
                    ..Default::default()
                })
                .await.unwrap();
            let semantic_time = semantic_start.elapsed();
            
            assert!(!semantic_results.is_empty(), "No semantic results for: {}", query);
            assert!(semantic_time < Duration::from_secs(2), 
                   "Semantic search too slow for '{}': {:?}", query, semantic_time);
            
            println!("Query '{}': keyword={:?}, semantic={:?}", 
                    query, keyword_time, semantic_time);
        }
        
        services.cleanup().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_real_time_update_performance() {
        let services = TestingServices::create_isolated_instance().await.unwrap();
        let query_manager = Arc::new(Mutex::new(QueryResultManager::new(
            services.storage.clone(),
            services.search_engine.clone(),
        )));
        let entity_manager = Arc::new(EntityManager::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.validation_engine.clone(),
        ));
        
        // Register schema
        let schema = create_test_task_schema();
        entity_manager.register_schema(schema).await.unwrap();
        
        // Create multiple queries that will be affected by updates
        let num_queries = 10;
        let mut query_ids = Vec::new();
        
        for i in 0..num_queries {
            let query_node = QueryNode {
                base: create_test_text_node(&format!("test_query_{}", i)),
                query: QueryDefinition {
                    name: format!("Test Query {}", i),
                    description: format!("Test query number {}", i),
                    entity_types: vec!["task".to_string()],
                    filters: vec![
                        QueryFilter {
                            field_name: "priority".to_string(),
                            operator: FilterOperator::Equals,
                            value: QueryValue::Static(EntityValue::Text(
                                if i % 2 == 0 { "high" } else { "medium" }.to_string()
                            )),
                            case_sensitive: false,
                            negate: false,
                        }
                    ],
                    sort_by: vec![SortCriteria {
                        field: "created_at".to_string(),
                        direction: SortDirection::Descending,
                    }],
                    limit: Some(100),
                    offset: None,
                    include_calculated_fields: vec![],
                    search_text: None,
                    date_range: None,
                    aggregations: vec![],
                },
                view_calculated_fields: vec![],
                results: vec![],
                last_executed: Utc::now(),
                auto_refresh: true,
                refresh_triggers: vec![
                    RefreshTrigger::FieldChange { 
                        entity_type: "task".to_string(), 
                        field: "priority".to_string(),
                        condition: None,
                    },
                    RefreshTrigger::NodeCreate { entity_type: "task".to_string() },
                ],
                subscription_id: uuid::Uuid::new_v4().to_string(),
                performance_metrics: QueryPerformanceMetrics::default(),
            };
            
            {
                let mut qm = query_manager.lock().await;
                qm.register_query(&query_node, &services).await.unwrap();
            }
            query_ids.push(query_node.id);
        }
        
        // Create many tasks that will trigger updates
        let num_tasks = 200;
        let update_start = Instant::now();
        
        let task_creation_tasks: Vec<_> = (0..num_tasks).map(|i| {
            let entity_manager = entity_manager.clone();
            tokio::spawn(async move {
                let task_data = hashmap! {
                    "title".to_string() => EntityValue::Text(format!("Task {}", i)),
                    "description".to_string() => EntityValue::Text(format!("Description for task {}", i)),
                    "priority".to_string() => EntityValue::Text(
                        if i % 2 == 0 { "high" } else { "medium" }.to_string()
                    ),
                    "status".to_string() => EntityValue::Text("pending".to_string()),
                    "due_date".to_string() => EntityValue::Date(
                        Utc::now() + chrono::Duration::days(i as i64 % 30)
                    ),
                };
                
                entity_manager.create_entity("task", task_data, None).await
            })
        }).collect();
        
        // Execute all task creations
        let creation_results = futures::future::join_all(task_creation_tasks).await;
        let successful_creations: Vec<_> = creation_results.into_iter()
            .filter_map(|r| r.ok().and_then(|r| r.ok()))
            .collect();
        
        let total_update_time = update_start.elapsed();
        
        assert_eq!(successful_creations.len(), num_tasks);
        
        // Each task creation should have triggered updates to multiple queries
        let total_expected_updates = num_tasks * (num_queries / 2); // Half queries match each priority
        
        println!("Created {} tasks triggering ~{} query updates in {:?}", 
                num_tasks, total_expected_updates, total_update_time);
        
        // Performance assertions
        assert!(total_update_time < Duration::from_secs(60), 
               "Real-time updates took too long: {:?}", total_update_time);
        
        let updates_per_second = total_expected_updates as f64 / total_update_time.as_secs_f64();
        println!("Update processing rate: {:.2} updates/second", updates_per_second);
        assert!(updates_per_second > 10.0, "Update processing too slow: {:.2} updates/sec", updates_per_second);
        
        services.cleanup().await.unwrap();
    }
}
```

---

## Continuous Integration Testing

### Automated Test Pipeline

```bash
#!/bin/bash
# test-pipeline.sh - Comprehensive CI testing script

set -e  # Exit on any error

echo "🚀 Starting NodeSpace Test Pipeline"

# Environment setup
export RUST_LOG=debug
export TEST_LANCE_DB_PATH="/tmp/nodespace_ci_test"
export TEST_VECTOR_DB_PATH="/tmp/ci_test_vectors"
export TEST_FILES_PATH="/tmp/ci_test_files"

# Cleanup function
cleanup() {
    echo "🧹 Cleaning up test environment..."
    docker-compose -f docker-compose.test.yml down || true
    rm -rf $TEST_VECTOR_DB_PATH || true
    rm -rf $TEST_FILES_PATH || true
}

# Set up cleanup on exit
trap cleanup EXIT

echo "📦 Setting up test services..."

# Start real services for testing
docker-compose -f docker-compose.test.yml up -d

# Wait for services to be ready
echo "⏳ Waiting for services to be ready..."
sleep 10

# Verify services are available
echo "🔍 Verifying test services..."
pg_isready -h localhost -p 5432 -U test_user || exit 1
curl -f http://localhost:11434/api/version || exit 1

# Run Rust backend tests
echo "🦀 Running Rust backend tests..."
cd nodespace-core
cargo test --release -- --test-threads=4 --nocapture

# Run plugin tests
echo "🔌 Running plugin tests..."
cd ../nodespace-pdf-node
cargo test --release -- --test-threads=2 --nocapture

cd ../nodespace-image-node  
cargo test --release -- --test-threads=2 --nocapture

cd ../nodespace-code-node
cargo test --release -- --test-threads=2 --nocapture

# Run integration tests
echo "🔗 Running integration tests..."
cd ../nodespace-app/src-tauri
cargo test --release integration_tests -- --test-threads=1 --nocapture

# Run frontend tests
echo "🎨 Running frontend tests..."
cd ../../
bun test

# Run end-to-end tests
echo "🎯 Running end-to-end tests..."
cd ../
bun run test:e2e

# Performance benchmarks
echo "⚡ Running performance benchmarks..."
cd nodespace-core
cargo bench

# Security audit
echo "🔒 Running security audit..."
cargo audit

# Generate test report
echo "📊 Generating test report..."
echo "Test Pipeline Completed Successfully at $(date)" > test-report.txt
echo "Services Used:" >> test-report.txt
echo "- LanceDB: $TEST_LANCE_DB_PATH" >> test-report.txt  
echo "- Ollama: http://localhost:11434" >> test-report.txt
echo "- Vector DB: $TEST_VECTOR_DB_PATH" >> test-report.txt

echo "✅ All tests passed! NodeSpace is ready for deployment."
```

### Docker Test Environment

```yaml
# docker-compose.test.yml
version: '3.8'
services:
  # LanceDB runs in-process, no separate container needed
    
  ollama-test:
    image: ollama/ollama:latest
    ports:
      - "11434:11434"
    volumes:
      - ollama_models:/root/.ollama
    environment:
      - OLLAMA_MODELS=/root/.ollama/models
    command: |
      sh -c "ollama serve & 
             sleep 10 && 
             ollama pull llama2:7b &&
             wait"
             
volumes:
  ollama_models:
    driver: local
```

---

## Testing Documentation and Reports

### Test Coverage Requirements

```rust
// Coverage configuration in Cargo.toml
[package.metadata.coverage]
include = [
    "src/entity_management/*",
    "src/ai_integration/*", 
    "src/validation/*",
    "src/real_time/*",
    "src/plugin_system/*"
]
exclude = [
    "src/tests/*",
    "src/test_utils/*"
]
target_coverage = 85  # Minimum 85% coverage required

// Example test with coverage annotations
#[cfg(test)]
mod coverage_tests {
    use super::*;
    
    #[test]
    #[coverage(always)]  // This test must always be included in coverage
    fn test_critical_path_validation() {
        // Test critical validation logic that must have coverage
    }
    
    #[test] 
    #[coverage(integration)]  // Only count in integration coverage
    fn test_full_workflow() {
        // Complex integration test
    }
}
```

### Test Result Documentation

```markdown
# NodeSpace Test Results

## Test Summary
- **Total Tests**: 1,247
- **Passed**: 1,247  
- **Failed**: 0
- **Test Coverage**: 87.3%
- **Performance Tests**: All within acceptable limits

## Service Verification  
✅ LanceDB - All CRUD operations tested
✅ LanceDB - Vector operations and similarity search tested  
✅ Ollama - AI inference and embeddings tested
✅ File System - Real file operations tested
✅ Real-time Updates - WebSocket and event propagation tested

## Critical Path Coverage
- ✅ Entity CRUD with validation: 94% coverage
- ✅ AI intent classification: 89% coverage  
- ✅ Real-time query updates: 91% coverage
- ✅ Plugin loading and execution: 85% coverage
- ✅ Cross-field validation: 88% coverage

## Performance Benchmarks
- Entity creation: 15.3 ops/second (target: >10)
- Query execution: 234ms avg (target: <500ms) 
- Real-time updates: 47 updates/second (target: >20)
- Search latency: 156ms avg (target: <200ms)

## Integration Test Results
All end-to-end workflows tested with real services:
- ✅ PDF upload → AI analysis → Entity creation → Query updates
- ✅ Natural language validation → Error explanation → Resolution
- ✅ Multi-plugin workflow → Cross-plugin search → Real-time sync
```

---

This comprehensive testing strategy ensures NodeSpace is thoroughly validated using real implementations across all layers. The emphasis on real services over mocks provides confidence that the system will work correctly in production environments, while the multi-dimensional testing approach covers functionality, performance, integration, and user workflows.