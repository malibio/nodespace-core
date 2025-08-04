# AI Integration Specification

## Overview

NodeSpace's AI integration provides natural language capabilities across all system operations, from content creation and entity management to validation and query processing. The system uses intent classification to route user requests to appropriate handlers and maintains conversational context for enhanced user experience.

## Core AI Architecture

### Intent Classification System

The AI system begins every interaction by classifying user intent to determine the appropriate processing pipeline:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatIntent {
    RAGQuery,              // "What is our deployment process?"
    ContentGeneration,     // "Draft an onboarding checklist"
    GeneralKnowledge,      // "What is Rust ownership?"
    EntityCRUD,            // "Add a new employee named John"
    NodeManipulation,      // "Create a task for code review"
    ValidationQuery,       // "Why can't I set this user as VIP?"
    SystemQuery,           // "How many PDF nodes do I have?"
}

pub struct IntentClassifier {
    nlp_engine: Arc<dyn NLPEngine>,
    classification_cache: LruCache<String, (ChatIntent, f32)>, // message -> (intent, confidence)
}

impl IntentClassifier {
    pub async fn classify_message(
        &mut self,
        message: &str,
        context: &ConversationContext
    ) -> Result<IntentClassification, Error> {
        // Check cache first
        if let Some((cached_intent, confidence)) = self.classification_cache.get(message) {
            return Ok(IntentClassification {
                intent: cached_intent.clone(),
                confidence: *confidence,
                reasoning: None,
            });
        }
        
        // Build classification prompt with context
        let classification_prompt = self.build_classification_prompt(message, context);
        
        // Get AI classification
        let response = self.nlp_engine.generate_text(&classification_prompt, "").await?;
        let classification = self.parse_classification_response(&response)?;
        
        // Cache result
        self.classification_cache.put(
            message.to_string(),
            (classification.intent.clone(), classification.confidence)
        );
        
        Ok(classification)
    }
    
    fn build_classification_prompt(&self, message: &str, context: &ConversationContext) -> String {
        let context_info = if !context.recent_messages.is_empty() {
            format!("Recent conversation:\n{}\n\n", 
                   context.recent_messages.iter()
                       .map(|m| format!("{}: {}", m.role, m.content))
                       .collect::<Vec<_>>()
                       .join("\n"))
        } else {
            String::new()
        };
        
        format!(r#"
{context_info}Classify this user message into one of these categories:

1. RAG_QUERY: User wants information from their existing knowledge base
   - Examples: "What is our deployment process?", "Find information about project X"
   
2. CONTENT_GENERATION: User wants to create new content/documents
   - Examples: "Draft an onboarding checklist", "Write a project proposal"
   
3. GENERAL_KNOWLEDGE: User asks about general topics not in their knowledge base
   - Examples: "What is Rust ownership?", "Explain machine learning"
   
4. ENTITY_CRUD: User wants to create/modify structured data entities
   - Examples: "Add a new employee named John", "Update Sarah's salary to $95,000"
   
5. NODE_MANIPULATION: User wants to create/modify nodes directly
   - Examples: "Create a task for code review", "Move this node under project X"
   
6. VALIDATION_QUERY: User is asking about validation errors or business rules
   - Examples: "Why can't I set this user as VIP?", "What are the rules for project status?"
   
7. SYSTEM_QUERY: User wants information about the system itself
   - Examples: "How many PDF nodes do I have?", "Show me all overdue tasks"

User message: "{message}"

Respond with JSON:
{{
  "intent": "RAG_QUERY" | "CONTENT_GENERATION" | "GENERAL_KNOWLEDGE" | "ENTITY_CRUD" | "NODE_MANIPULATION" | "VALIDATION_QUERY" | "SYSTEM_QUERY",
  "confidence": 0.0-1.0,
  "reasoning": "Brief explanation of why this classification was chosen"
}}
"#)
    }
    
    fn parse_classification_response(&self, response: &str) -> Result<IntentClassification, Error> {
        let classification: serde_json::Value = serde_json::from_str(response.trim())?;
        
        let intent_str = classification["intent"].as_str()
            .ok_or("Missing intent in classification response")?;
        
        let intent = match intent_str {
            "RAG_QUERY" => ChatIntent::RAGQuery,
            "CONTENT_GENERATION" => ChatIntent::ContentGeneration,
            "GENERAL_KNOWLEDGE" => ChatIntent::GeneralKnowledge,
            "ENTITY_CRUD" => ChatIntent::EntityCRUD,
            "NODE_MANIPULATION" => ChatIntent::NodeManipulation,
            "VALIDATION_QUERY" => ChatIntent::ValidationQuery,
            "SYSTEM_QUERY" => ChatIntent::SystemQuery,
            _ => return Err(Error::UnknownIntent(intent_str.to_string())),
        };
        
        let confidence = classification["confidence"].as_f64().unwrap_or(0.5) as f32;
        let reasoning = classification["reasoning"].as_str().map(|s| s.to_string());
        
        Ok(IntentClassification {
            intent,
            confidence,
            reasoning,
        })
    }
}

#[derive(Debug, Clone)]
pub struct IntentClassification {
    pub intent: ChatIntent,
    pub confidence: f32,
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ConversationContext {
    pub recent_messages: Vec<ChatMessage>,
    pub current_node_context: Option<String>,
    pub active_entities: Vec<String>,
    pub user_preferences: UserPreferences,
}
```

---

## RAG Query Processing

### Knowledge Base Search and Response Generation

```rust
pub struct RAGProcessor {
    search_engine: Arc<dyn SearchEngine>,
    nlp_engine: Arc<dyn NLPEngine>,
    query_cache: LruCache<String, RAGResponse>,
}

impl RAGProcessor {
    pub async fn process_rag_query(
        &mut self,
        query: &str,
        context: &ConversationContext
    ) -> Result<RAGResponse, Error> {
        // Check cache
        if let Some(cached_response) = self.query_cache.get(query) {
            return Ok(cached_response.clone());
        }
        
        // 1. Generate query embedding
        let query_embedding = self.nlp_engine.embed_query(query).await?;
        
        // 2. Search knowledge base with multiple strategies
        let search_results = self.multi_strategy_search(&query_embedding, query).await?;
        
        // 3. Rank and filter results
        let ranked_results = self.rank_search_results(&search_results, query).await?;
        
        // 4. Generate contextual response
        let response = self.generate_contextual_response(query, &ranked_results, context).await?;
        
        // Cache the response
        self.query_cache.put(query.to_string(), response.clone());
        
        Ok(response)
    }
    
    async fn multi_strategy_search(
        &self,
        query_embedding: &[f32],
        query_text: &str
    ) -> Result<Vec<SearchResult>, Error> {
        // Run multiple search strategies in parallel
        let semantic_search = self.search_engine.similarity_search(query_embedding, SearchConfig {
            max_results: 20,
            min_similarity: 0.7,
            ..Default::default()
        });
        
        let keyword_search = self.search_engine.keyword_search(query_text, SearchConfig {
            max_results: 10,
            fuzzy_matching: true,
            ..Default::default()
        });
        
        let metadata_search = self.search_engine.metadata_search(&self.extract_metadata_filters(query_text));
        
        // Await all searches
        let (semantic_results, keyword_results, metadata_results) = tokio::try_join!(
            semantic_search,
            keyword_search,
            metadata_search
        )?;
        
        // Combine and deduplicate results
        let mut combined_results = Vec::new();
        combined_results.extend(semantic_results);
        combined_results.extend(keyword_results);
        combined_results.extend(metadata_results);
        
        // Remove duplicates based on node_id
        let mut seen_ids = std::collections::HashSet::new();
        combined_results.retain(|result| seen_ids.insert(result.node_id.clone()));
        
        Ok(combined_results)
    }
    
    async fn rank_search_results(
        &self,
        results: &[SearchResult],
        query: &str
    ) -> Result<Vec<RankedSearchResult>, Error> {
        let mut ranked_results = Vec::new();
        
        for result in results {
            // Calculate multiple relevance scores
            let semantic_score = result.similarity_score;
            let keyword_score = self.calculate_keyword_relevance(&result.content, query);
            let recency_score = self.calculate_recency_score(result.modified_at);
            let authority_score = self.calculate_authority_score(result);
            
            // Weighted combination
            let combined_score = 
                semantic_score * 0.4 +
                keyword_score * 0.3 +
                recency_score * 0.2 +
                authority_score * 0.1;
            
            ranked_results.push(RankedSearchResult {
                search_result: result.clone(),
                combined_score,
                semantic_score,
                keyword_score,
                recency_score,
                authority_score,
            });
        }
        
        // Sort by combined score
        ranked_results.sort_by(|a, b| b.combined_score.partial_cmp(&a.combined_score).unwrap());
        
        // Take top results
        ranked_results.truncate(5);
        
        Ok(ranked_results)
    }
    
    async fn generate_contextual_response(
        &self,
        query: &str,
        ranked_results: &[RankedSearchResult],
        context: &ConversationContext
    ) -> Result<RAGResponse, Error> {
        if ranked_results.is_empty() {
            return Ok(RAGResponse {
                answer: "I couldn't find any relevant information in your knowledge base for that query.".to_string(),
                sources: Vec::new(),
                confidence: 0.0,
                suggested_actions: vec![
                    "Try rephrasing your question".to_string(),
                    "Check if the information exists in your documents".to_string(),
                ],
            });
        }
        
        // Build context from search results
        let context_chunks: Vec<String> = ranked_results.iter()
            .map(|r| format!("Source: {} (Score: {:.2})\nContent: {}\n", 
                           r.search_result.title.as_deref().unwrap_or("Untitled"),
                           r.combined_score,
                           self.truncate_content(&r.search_result.content, 500)))
            .collect();
        
        let context_text = context_chunks.join("\n---\n");
        
        // Generate response with source attribution
        let response_prompt = format!(r#"
Answer the user's question using ONLY the provided context. Be specific and cite your sources.

Question: {query}

Context:
{context_text}

Instructions:
1. Answer the question directly and comprehensively
2. Use specific information from the provided sources
3. If the context doesn't fully answer the question, say so
4. Include source references in your answer
5. If multiple sources have conflicting information, mention both perspectives

Response format:
- Start with a direct answer
- Include supporting details from the sources
- End with source citations

Answer:
"#);
        
        let generated_response = self.nlp_engine.generate_text(&response_prompt, "").await?;
        
        // Extract sources for attribution
        let sources: Vec<SourceAttribution> = ranked_results.iter()
            .map(|r| SourceAttribution {
                node_id: r.search_result.node_id.clone(),
                title: r.search_result.title.clone().unwrap_or("Untitled".to_string()),
                relevance_score: r.combined_score,
                excerpt: self.extract_relevant_excerpt(&r.search_result.content, query, 200),
                node_type: r.search_result.node_type.clone(),
            })
            .collect();
        
        // Calculate confidence based on result quality
        let confidence = self.calculate_response_confidence(ranked_results);
        
        // Generate suggested actions
        let suggested_actions = self.generate_suggested_actions(query, ranked_results, confidence);
        
        Ok(RAGResponse {
            answer: generated_response,
            sources,
            confidence,
            suggested_actions,
        })
    }
    
    fn calculate_response_confidence(&self, ranked_results: &[RankedSearchResult]) -> f32 {
        if ranked_results.is_empty() {
            return 0.0;
        }
        
        let top_score = ranked_results[0].combined_score;
        let avg_score = ranked_results.iter()
            .map(|r| r.combined_score)
            .sum::<f32>() / ranked_results.len() as f32;
        
        // Confidence based on top result quality and consistency
        let quality_factor = (top_score).min(1.0);
        let consistency_factor = if ranked_results.len() > 1 {
            1.0 - (top_score - avg_score).abs()
        } else {
            0.8 // Slight penalty for single source
        };
        
        (quality_factor * consistency_factor).max(0.1).min(0.95)
    }
}

#[derive(Debug, Clone)]
pub struct RAGResponse {
    pub answer: String,
    pub sources: Vec<SourceAttribution>,
    pub confidence: f32,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SourceAttribution {
    pub node_id: String,
    pub title: String,
    pub relevance_score: f32,
    pub excerpt: String,
    pub node_type: String,
}

#[derive(Debug, Clone)]
pub struct RankedSearchResult {
    pub search_result: SearchResult,
    pub combined_score: f32,
    pub semantic_score: f32,
    pub keyword_score: f32,
    pub recency_score: f32,
    pub authority_score: f32,
}
```

---

## Content Generation

### AI-Powered Content Creation with Node Generation

```rust
pub struct ContentGenerator {
    nlp_engine: Arc<dyn NLPEngine>,
    node_factory: Arc<NodeFactory>,
    template_library: TemplateLibrary,
}

impl ContentGenerator {
    pub async fn generate_content(
        &self,
        request: &ContentGenerationRequest,
        context: &ConversationContext
    ) -> Result<ContentGenerationResponse, Error> {
        // 1. Analyze request type and determine best approach
        let generation_strategy = self.analyze_generation_request(request).await?;
        
        // 2. Generate content using appropriate strategy
        let generated_content = match generation_strategy {
            GenerationStrategy::TemplateBasedGeneration { template_id } => {
                self.generate_with_template(&template_id, request).await?
            },
            GenerationStrategy::FreeformGeneration => {
                self.generate_freeform_content(request).await?
            },
            GenerationStrategy::StructuredGeneration { format } => {
                self.generate_structured_content(request, &format).await?
            },
        };
        
        // 3. Offer node creation options
        let node_suggestions = self.suggest_node_structure(&generated_content, context).await?;
        
        Ok(ContentGenerationResponse {
            content: generated_content,
            node_suggestions,
            suggested_actions: self.generate_action_suggestions(&generated_content),
        })
    }
    
    async fn analyze_generation_request(
        &self,
        request: &ContentGenerationRequest
    ) -> Result<GenerationStrategy, Error> {
        let analysis_prompt = format!(r#"
Analyze this content generation request and determine the best approach:

Request: "{}"

Available strategies:
1. TEMPLATE_BASED: Use a pre-defined template (good for: emails, reports, checklists)
2. FREEFORM: Generate completely custom content (good for: creative writing, explanations)
3. STRUCTURED: Generate content in a specific format (good for: lists, tables, forms)

Consider:
- Is this a common business document type?
- Does it need specific formatting?
- Would a template be helpful?

Respond with JSON:
{{
  "strategy": "TEMPLATE_BASED" | "FREEFORM" | "STRUCTURED",
  "template_id": "optional_template_name",
  "format": "optional_format_type",
  "reasoning": "explanation"
}}
"#, request.description);
        
        let response = self.nlp_engine.generate_text(&analysis_prompt, "").await?;
        let analysis: serde_json::Value = serde_json::from_str(&response)?;
        
        match analysis["strategy"].as_str().unwrap_or("FREEFORM") {
            "TEMPLATE_BASED" => Ok(GenerationStrategy::TemplateBasedGeneration {
                template_id: analysis["template_id"].as_str().unwrap_or("default").to_string(),
            }),
            "STRUCTURED" => Ok(GenerationStrategy::StructuredGeneration {
                format: analysis["format"].as_str().unwrap_or("list").to_string(),
            }),
            _ => Ok(GenerationStrategy::FreeformGeneration),
        }
    }
    
    async fn generate_freeform_content(
        &self,
        request: &ContentGenerationRequest
    ) -> Result<GeneratedContent, Error> {
        let generation_prompt = self.build_generation_prompt(request);
        let raw_content = self.nlp_engine.generate_text(&generation_prompt, "").await?;
        
        // Post-process the generated content
        let processed_content = self.post_process_content(&raw_content, request)?;
        
        Ok(GeneratedContent {
            text: processed_content,
            format: ContentFormat::Markdown,
            metadata: self.extract_content_metadata(&processed_content),
        })
    }
    
    fn build_generation_prompt(&self, request: &ContentGenerationRequest) -> String {
        let context_info = if let Some(context) = &request.context_nodes {
            format!("Context from existing nodes:\n{}\n\n", 
                   context.iter()
                       .map(|node| format!("- {}: {}", node.title, node.summary))
                       .collect::<Vec<_>>()
                       .join("\n"))
        } else {
            String::new()
        };
        
        let style_guidance = match request.style {
            ContentStyle::Professional => "Use professional, business-appropriate language.",
            ContentStyle::Casual => "Use friendly, conversational tone.",
            ContentStyle::Technical => "Use precise, technical language with appropriate terminology.",
            ContentStyle::Creative => "Use engaging, creative language with varied sentence structure.",
        };
        
        format!(r#"
{context_info}Generate content based on this request: "{}"

Requirements:
- {style_guidance}
- Target length: {} words
- Format: {}
- Make it practical and actionable
- Use markdown formatting for structure

Generated content:
"#, 
            request.description,
            style_guidance,
            request.target_length.unwrap_or(500),
            request.format.as_deref().unwrap_or("markdown")
        )
    }
    
    async fn suggest_node_structure(
        &self,
        content: &GeneratedContent,
        context: &ConversationContext
    ) -> Result<Vec<NodeSuggestion>, Error> {
        let structure_analysis_prompt = format!(r#"
Analyze this generated content and suggest how it should be structured as nodes:

Content:
{}

Suggest one of these structures:
1. SINGLE_NODE: Keep as one text node
2. HIERARCHICAL: Break into parent node with child sections
3. TASK_LIST: Convert to task nodes if it contains actionable items
4. ENTITY_CREATION: Create structured entities if it contains data

Consider:
- Length and complexity of content
- Presence of distinct sections
- Actionable items or data
- User's likely workflow needs

Respond with JSON:
{{
  "structure": "SINGLE_NODE" | "HIERARCHICAL" | "TASK_LIST" | "ENTITY_CREATION",
  "suggestions": [
    {{
      "node_type": "text" | "task" | "entity",
      "title": "suggested title",
      "content": "content for this node",
      "parent_id": "optional_parent_reference"
    }}
  ],
  "reasoning": "explanation"
}}
"#, content.text);
        
        let response = self.nlp_engine.generate_text(&structure_analysis_prompt, "").await?;
        let analysis: serde_json::Value = serde_json::from_str(&response)?;
        
        let suggestions: Vec<NodeSuggestion> = analysis["suggestions"]
            .as_array()
            .unwrap_or(&Vec::new())
            .iter()
            .map(|s| NodeSuggestion {
                node_type: s["node_type"].as_str().unwrap_or("text").to_string(),
                title: s["title"].as_str().unwrap_or("Untitled").to_string(),
                content: s["content"].as_str().unwrap_or("").to_string(),
                parent_id: s["parent_id"].as_str().map(|s| s.to_string()),
            })
            .collect();
        
        Ok(suggestions)
    }
    
    pub async fn create_nodes_from_suggestions(
        &self,
        suggestions: &[NodeSuggestion],
        parent_id: Option<String>
    ) -> Result<Vec<String>, Error> {
        let mut created_node_ids = Vec::new();
        let mut parent_map = std::collections::HashMap::new();
        
        for suggestion in suggestions {
            let actual_parent_id = suggestion.parent_id.clone()
                .or_else(|| parent_map.get(&suggestion.title).cloned())
                .or(parent_id.clone());
            
            let node_id = match suggestion.node_type.as_str() {
                "task" => {
                    self.node_factory.create_task_node(
                        &suggestion.title,
                        &suggestion.content,
                        actual_parent_id
                    ).await?
                },
                "entity" => {
                    // Parse entity data from content
                    let entity_data = self.parse_entity_data(&suggestion.content)?;
                    self.node_factory.create_entity_node(
                        &entity_data.entity_type,
                        entity_data.fields,
                        actual_parent_id
                    ).await?
                },
                _ => {
                    self.node_factory.create_text_node(
                        &suggestion.content,
                        actual_parent_id
                    ).await?
                }
            };
            
            created_node_ids.push(node_id.clone());
            parent_map.insert(suggestion.title.clone(), node_id);
        }
        
        Ok(created_node_ids)
    }
}

#[derive(Debug, Clone)]
pub struct ContentGenerationRequest {
    pub description: String,
    pub style: ContentStyle,
    pub target_length: Option<usize>,
    pub format: Option<String>,
    pub context_nodes: Option<Vec<ContextNode>>,
}

#[derive(Debug, Clone)]
pub enum ContentStyle {
    Professional,
    Casual,
    Technical,
    Creative,
}

#[derive(Debug, Clone)]
pub enum GenerationStrategy {
    TemplateBasedGeneration { template_id: String },
    FreeformGeneration,
    StructuredGeneration { format: String },
}

#[derive(Debug, Clone)]
pub struct ContentGenerationResponse {
    pub content: GeneratedContent,
    pub node_suggestions: Vec<NodeSuggestion>,
    pub suggested_actions: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct NodeSuggestion {
    pub node_type: String,
    pub title: String,
    pub content: String,
    pub parent_id: Option<String>,
}
```

---

## Entity CRUD Operations

### Natural Language Entity Management

```rust
pub struct EntityCRUDProcessor {
    entity_manager: Arc<EntityManager>,
    nlp_engine: Arc<dyn NLPEngine>,
    schema_registry: Arc<EntitySchemaRegistry>,
}

impl EntityCRUDProcessor {
    pub async fn process_entity_crud(
        &self,
        message: &str,
        context: &ConversationContext
    ) -> Result<EntityCRUDResponse, Error> {
        // 1. Parse the CRUD operation
        let operation = self.parse_crud_operation(message, context).await?;
        
        // 2. Execute the operation
        let result = match operation {
            EntityOperation::Create(create_req) => {
                self.handle_entity_creation(create_req).await?
            },
            EntityOperation::Read(read_req) => {
                self.handle_entity_query(read_req).await?
            },
            EntityOperation::Update(update_req) => {
                self.handle_entity_update(update_req).await?
            },
            EntityOperation::Delete(delete_req) => {
                self.handle_entity_deletion(delete_req).await?
            },
            EntityOperation::Analyze(analyze_req) => {
                self.handle_entity_analysis(analyze_req).await?
            },
        };
        
        Ok(result)
    }
    
    async fn parse_crud_operation(
        &self,
        message: &str,
        context: &ConversationContext
    ) -> Result<EntityOperation, Error> {
        // Get available entity types for context
        let available_entities = self.schema_registry.list_entity_types();
        let entity_list = available_entities.join(", ");
        
        let parsing_prompt = format!(r#"
Parse this message into a structured entity operation:

Message: "{message}"

Available entity types: {entity_list}

Determine the operation type and extract relevant information:

1. CREATE: Adding new entities
   - Examples: "Add employee John Smith", "Create a new project called NodeSpace"
   
2. READ/QUERY: Finding or listing entities
   - Examples: "Show me all employees", "Find projects due this week"
   
3. UPDATE: Modifying existing entities
   - Examples: "Update John's salary to $95,000", "Mark project as completed"
   
4. DELETE: Removing entities
   - Examples: "Delete the test project", "Remove John from the employee list"
   
5. ANALYZE: Analytical queries across entities
   - Examples: "How many employees earn over $100k?", "What's the average project duration?"

Respond with JSON:
{{
  "operation": "CREATE" | "READ" | "UPDATE" | "DELETE" | "ANALYZE",
  "entity_type": "employee" | "project" | etc.,
  "details": {{
    // Operation-specific details extracted from the message
  }},
  "confidence": 0.0-1.0
}}
"#);
        
        let response = self.nlp_engine.generate_text(&parsing_prompt, "").await?;
        let parsed: serde_json::Value = serde_json::from_str(&response)?;
        
        let operation_type = parsed["operation"].as_str().ok_or("Missing operation type")?;
        let entity_type = parsed["entity_type"].as_str().ok_or("Missing entity type")?;
        let details = parsed["details"].clone();
        
        match operation_type {
            "CREATE" => Ok(EntityOperation::Create(EntityCreateRequest {
                entity_type: entity_type.to_string(),
                extracted_fields: self.extract_fields_from_details(&details, entity_type).await?,
                original_message: message.to_string(),
            })),
            "READ" => Ok(EntityOperation::Read(EntityReadRequest {
                entity_type: entity_type.to_string(),
                filters: self.extract_filters_from_details(&details).await?,
                fields: self.extract_requested_fields(&details),
                original_message: message.to_string(),
            })),
            "UPDATE" => Ok(EntityOperation::Update(EntityUpdateRequest {
                entity_type: entity_type.to_string(),
                identifier: self.extract_entity_identifier(&details).await?,
                field_updates: self.extract_field_updates(&details, entity_type).await?,
                original_message: message.to_string(),
            })),
            "DELETE" => Ok(EntityOperation::Delete(EntityDeleteRequest {
                entity_type: entity_type.to_string(),
                identifier: self.extract_entity_identifier(&details).await?,
                original_message: message.to_string(),
            })),
            "ANALYZE" => Ok(EntityOperation::Analyze(EntityAnalyzeRequest {
                entity_types: vec![entity_type.to_string()],
                analysis_type: self.extract_analysis_type(&details),
                filters: self.extract_filters_from_details(&details).await?,
                aggregations: self.extract_aggregations(&details),
                original_message: message.to_string(),
            })),
            _ => Err(Error::UnknownOperation(operation_type.to_string())),
        }
    }
    
    async fn handle_entity_creation(
        &self,
        request: EntityCreateRequest
    ) -> Result<EntityCRUDResponse, Error> {
        // 1. Get entity schema
        let schema = self.schema_registry.get_schema(&request.entity_type)
            .ok_or_else(|| Error::UnknownEntityType(request.entity_type.clone()))?;
        
        // 2. Validate extracted fields against schema
        let validation_issues = self.validate_extracted_fields(&request.extracted_fields, &schema);
        
        if !validation_issues.is_empty() {
            return Ok(EntityCRUDResponse::ValidationRequest {
                message: self.format_validation_request(&validation_issues, &request.original_message),
                missing_fields: validation_issues.iter()
                    .filter_map(|issue| match issue {
                        ValidationIssue::MissingRequiredField { field_name } => Some(field_name.clone()),
                        _ => None,
                    })
                    .collect(),
                suggested_values: self.suggest_field_values(&validation_issues, &schema).await?,
            });
        }
        
        // 3. Create entity
        let entity = self.entity_manager.create_entity(
            &request.entity_type,
            request.extracted_fields,
            None // TODO: Determine parent from context
        ).await?;
        
        // 4. Format success response
        Ok(EntityCRUDResponse::Success {
            message: format!("Successfully created {} with ID {}", request.entity_type, entity.base.id),
            affected_entities: vec![entity.base.id.clone()],
            summary: self.format_entity_summary(&entity),
        })
    }
    
    async fn handle_entity_analysis(
        &self,
        request: EntityAnalyzeRequest
    ) -> Result<EntityCRUDResponse, Error> {
        // 1. Build query from analysis request
        let query = self.build_analysis_query(&request)?;
        
        // 2. Execute query
        let results = self.entity_manager.query_entities(&request.entity_types[0], query).await?;
        
        // 3. Perform analysis
        let analysis_result = match request.analysis_type {
            AnalysisType::Count => {
                AnalysisResult::Count {
                    total: results.entities.len(),
                    breakdown: self.calculate_breakdown(&results.entities, &request.filters),
                }
            },
            AnalysisType::Aggregation => {
                AnalysisResult::Aggregation {
                    aggregations: self.calculate_aggregations(&results.entities, &request.aggregations)?,
                }
            },
            AnalysisType::Comparison => {
                AnalysisResult::Comparison {
                    comparisons: self.calculate_comparisons(&results.entities, &request.filters)?,
                }
            },
            AnalysisType::Trend => {
                AnalysisResult::Trend {
                    trend_data: self.calculate_trends(&results.entities)?,
                }
            },
        };
        
        // 4. Generate natural language explanation
        let explanation = self.generate_analysis_explanation(&analysis_result, &request.original_message).await?;
        
        Ok(EntityCRUDResponse::Analysis {
            explanation,
            data: analysis_result,
            visualization_suggestions: self.suggest_visualizations(&analysis_result),
        })
    }
    
    async fn generate_analysis_explanation(
        &self,
        result: &AnalysisResult,
        original_question: &str
    ) -> Result<String, Error> {
        let data_summary = match result {
            AnalysisResult::Count { total, breakdown } => {
                format!("Total count: {}\nBreakdown: {:?}", total, breakdown)
            },
            AnalysisResult::Aggregation { aggregations } => {
                format!("Aggregation results: {:?}", aggregations)
            },
            AnalysisResult::Comparison { comparisons } => {
                format!("Comparison results: {:?}", comparisons)
            },
            AnalysisResult::Trend { trend_data } => {
                format!("Trend analysis: {:?}", trend_data)
            },
        };
        
        let explanation_prompt = format!(r#"
The user asked: "{original_question}"

Here's the analysis result:
{data_summary}

Provide a clear, natural language explanation of these results that:
1. Directly answers the user's question
2. Highlights key insights and patterns
3. Mentions specific numbers and metrics
4. Suggests potential implications or actions
5. Uses business-friendly language

Explanation:
"#);
        
        let explanation = self.nlp_engine.generate_text(&explanation_prompt, "").await?;
        Ok(explanation)
    }
}

#[derive(Debug, Clone)]
pub enum EntityOperation {
    Create(EntityCreateRequest),
    Read(EntityReadRequest),
    Update(EntityUpdateRequest),
    Delete(EntityDeleteRequest),
    Analyze(EntityAnalyzeRequest),
}

#[derive(Debug, Clone)]
pub struct EntityCreateRequest {
    pub entity_type: String,
    pub extracted_fields: HashMap<String, EntityValue>,
    pub original_message: String,
}

#[derive(Debug, Clone)]
pub struct EntityAnalyzeRequest {
    pub entity_types: Vec<String>,
    pub analysis_type: AnalysisType,
    pub filters: Vec<QueryFilter>,
    pub aggregations: Vec<AggregationRequest>,
    pub original_message: String,
}

#[derive(Debug, Clone)]
pub enum AnalysisType {
    Count,        // "How many X do we have?"
    Aggregation,  // "What's the average/sum/max of X?"
    Comparison,   // "Compare X and Y"
    Trend,        // "How has X changed over time?"
}

#[derive(Debug, Clone)]
pub enum EntityCRUDResponse {
    Success {
        message: String,
        affected_entities: Vec<String>,
        summary: String,
    },
    ValidationRequest {
        message: String,
        missing_fields: Vec<String>,
        suggested_values: HashMap<String, Vec<String>>,
    },
    Analysis {
        explanation: String,
        data: AnalysisResult,
        visualization_suggestions: Vec<String>,
    },
    Error {
        message: String,
        recovery_suggestions: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub enum AnalysisResult {
    Count {
        total: usize,
        breakdown: HashMap<String, usize>,
    },
    Aggregation {
        aggregations: HashMap<String, f64>,
    },
    Comparison {
        comparisons: Vec<ComparisonResult>,
    },
    Trend {
        trend_data: Vec<TrendPoint>,
    },
}
```

---

## Conversational Validation Handling

### AI-Powered Error Explanation and Resolution

```rust
pub struct ConversationalValidator {
    validation_engine: Arc<ValidationEngine>,
    nlp_engine: Arc<dyn NLPEngine>,
    resolution_engine: ResolutionEngine,
}

impl ConversationalValidator {
    pub async fn handle_validation_error(
        &self,
        validation_result: &ValidationResult,
        original_request: &str,
        context: &ConversationContext
    ) -> Result<ConversationalValidationResponse, Error> {
        if validation_result.is_valid {
            return Ok(ConversationalValidationResponse::Success);
        }
        
        // 1. Analyze validation violations for conversational explanation
        let violation_analysis = self.analyze_violations(&validation_result.violations).await?;
        
        // 2. Generate natural language explanation
        let explanation = self.generate_violation_explanation(
            &violation_analysis,
            original_request,
            context
        ).await?;
        
        // 3. Generate resolution options
        let resolution_options = self.generate_resolution_options(
            &validation_result.violations,
            context
        ).await?;
        
        Ok(ConversationalValidationResponse::ValidationError {
            explanation,
            violations: validation_result.violations.clone(),
            resolution_options,
            can_override: self.check_override_permissions(&validation_result.violations, context),
        })
    }
    
    async fn generate_violation_explanation(
        &self,
        violation_analysis: &ViolationAnalysis,
        original_request: &str,
        context: &ConversationContext
    ) -> Result<String, Error> {
        let violation_summaries: Vec<String> = violation_analysis.violations.iter()
            .map(|v| format!("- {}: {}", v.field_name, v.user_friendly_explanation))
            .collect();
        
        let explanation_prompt = format!(r#"
The user requested: "{original_request}"

This request failed validation with these issues:
{violation_summaries}

Generate a conversational, helpful explanation that:
1. Acknowledges what the user was trying to do
2. Explains why it can't be done in simple terms
3. Focuses on business rules rather than technical details
4. Uses empathetic language ("I understand you want to..." / "Unfortunately, our business rules require...")
5. Sets up for offering solutions

Example good response:
"I understand you want to set John's status to VIP. Unfortunately, I can't do that because our business rules require customers to have at least $5,000 in total purchases for VIP status, and John currently has $3,200 in purchases."

Explanation:
"#, violation_summaries = violation_summaries.join("\n"));
        
        let explanation = self.nlp_engine.generate_text(&explanation_prompt, "").await?;
        Ok(explanation)
    }
    
    async fn generate_resolution_options(
        &self,
        violations: &[ValidationViolation],
        context: &ConversationContext
    ) -> Result<Vec<ResolutionOption>, Error> {
        let mut resolution_options = Vec::new();
        
        for violation in violations {
            let options = match &violation.violation_type {
                ViolationType::RequiredFieldMissing => {
                    vec![
                        ResolutionOption {
                            id: format!("provide_{}", violation.field_name),
                            title: format!("Provide {}", violation.field_name),
                            description: format!("I can help you set the {} field", violation.field_name),
                            action_type: ResolutionActionType::RequestInput {
                                field_name: violation.field_name.clone(),
                                prompt: format!("What would you like to set {} to?", violation.field_name),
                            },
                            confidence: 0.9,
                        }
                    ]
                },
                ViolationType::BusinessRuleViolation { rule_id } => {
                    self.generate_business_rule_resolutions(rule_id, violation, context).await?
                },
                ViolationType::ReferenceConstraintViolation { target_entity } => {
                    vec![
                        ResolutionOption {
                            id: format!("create_{}", target_entity),
                            title: format!("Create the referenced {}", target_entity),
                            description: format!("I can help you create the {} that's being referenced", target_entity),
                            action_type: ResolutionActionType::CreateEntity {
                                entity_type: target_entity.clone(),
                            },
                            confidence: 0.8,
                        },
                        ResolutionOption {
                            id: format!("choose_existing_{}", target_entity),
                            title: format!("Choose a different {}", target_entity),
                            description: format!("I can show you existing {} to choose from", target_entity),
                            action_type: ResolutionActionType::ShowOptions {
                                entity_type: target_entity.clone(),
                            },
                            confidence: 0.9,
                        }
                    ]
                },
                ViolationType::TypeMismatch { expected_type, actual_type } => {
                    vec![
                        ResolutionOption {
                            id: format!("convert_type_{}", violation.field_name),
                            title: format!("Convert to {}", expected_type),
                            description: format!("I can try to convert '{}' to the expected {} format", 
                                               actual_type, expected_type),
                            action_type: ResolutionActionType::ConvertValue {
                                field_name: violation.field_name.clone(),
                                target_type: expected_type.clone(),
                            },
                            confidence: 0.7,
                        }
                    ]
                },
            };
            
            resolution_options.extend(options);
        }
        
        // Always offer override option if user has permissions
        if self.check_override_permissions(violations, context) {
            resolution_options.push(ResolutionOption {
                id: "override_validation".to_string(),
                title: "Override validation".to_string(),
                description: "Proceed anyway with manager approval".to_string(),
                action_type: ResolutionActionType::RequestOverride {
                    reason_required: true,
                },
                confidence: 0.6,
            });
        }
        
        Ok(resolution_options)
    }
    
    async fn generate_business_rule_resolutions(
        &self,
        rule_id: &str,
        violation: &ValidationViolation,
        context: &ConversationContext
    ) -> Result<Vec<ResolutionOption>, Error> {
        // Get the specific business rule
        let rule = self.validation_engine.get_validation_rule(rule_id).await?;
        
        let resolution_prompt = format!(r#"
Generate resolution options for this business rule violation:

Rule: {}
Field: {}
Violation: {}

The rule expression is: {}

Suggest practical ways the user could resolve this violation. Consider:
1. What could they change to satisfy the rule?
2. Are there alternative approaches?
3. What additional information might be needed?

Respond with JSON array of options:
[
  {{
    "title": "Increase purchase history",
    "description": "Add $1,800 more in purchases to reach the $5,000 VIP requirement",
    "action_type": "modify_related_field",
    "target_field": "total_purchases",
    "target_value": "5000",
    "confidence": 0.9
  }}
]
"#, rule.description, violation.field_name, violation.message, rule.expression);
        
        let response = self.nlp_engine.generate_text(&resolution_prompt, "").await?;
        let options: Vec<serde_json::Value> = serde_json::from_str(&response)?;
        
        let resolution_options: Vec<ResolutionOption> = options.iter()
            .map(|opt| ResolutionOption {
                id: format!("rule_resolution_{}", uuid::Uuid::new_v4()),
                title: opt["title"].as_str().unwrap_or("Unknown").to_string(),
                description: opt["description"].as_str().unwrap_or("").to_string(),
                action_type: self.parse_resolution_action_type(opt),
                confidence: opt["confidence"].as_f64().unwrap_or(0.5) as f32,
            })
            .collect();
        
        Ok(resolution_options)
    }
    
    pub async fn execute_resolution(
        &self,
        option: &ResolutionOption,
        context: &ConversationContext
    ) -> Result<ResolutionResult, Error> {
        match &option.action_type {
            ResolutionActionType::RequestInput { field_name, prompt } => {
                Ok(ResolutionResult::InputRequested {
                    prompt: prompt.clone(),
                    field_name: field_name.clone(),
                })
            },
            ResolutionActionType::CreateEntity { entity_type } => {
                let creation_result = self.initiate_entity_creation(entity_type, context).await?;
                Ok(ResolutionResult::EntityCreationStarted {
                    entity_type: entity_type.clone(),
                    next_steps: creation_result.next_steps,
                })
            },
            ResolutionActionType::ShowOptions { entity_type } => {
                let options = self.load_entity_options(entity_type, context).await?;
                Ok(ResolutionResult::OptionsShown {
                    entity_type: entity_type.clone(),
                    options,
                })
            },
            ResolutionActionType::ConvertValue { field_name, target_type } => {
                let conversion_result = self.attempt_value_conversion(field_name, target_type, context).await?;
                Ok(ResolutionResult::ValueConverted {
                    field_name: field_name.clone(),
                    new_value: conversion_result.converted_value,
                    success: conversion_result.success,
                })
            },
            ResolutionActionType::RequestOverride { reason_required } => {
                Ok(ResolutionResult::OverrideRequested {
                    reason_required: *reason_required,
                    approval_needed: true,
                })
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum ConversationalValidationResponse {
    Success,
    ValidationError {
        explanation: String,
        violations: Vec<ValidationViolation>,
        resolution_options: Vec<ResolutionOption>,
        can_override: bool,
    },
}

#[derive(Debug, Clone)]
pub struct ResolutionOption {
    pub id: String,
    pub title: String,
    pub description: String,
    pub action_type: ResolutionActionType,
    pub confidence: f32,
}

#[derive(Debug, Clone)]
pub enum ResolutionActionType {
    RequestInput {
        field_name: String,
        prompt: String,
    },
    CreateEntity {
        entity_type: String,
    },
    ShowOptions {
        entity_type: String,
    },
    ConvertValue {
        field_name: String,
        target_type: String,
    },
    RequestOverride {
        reason_required: bool,
    },
}

#[derive(Debug, Clone)]
pub enum ResolutionResult {
    InputRequested {
        prompt: String,
        field_name: String,
    },
    EntityCreationStarted {
        entity_type: String,
        next_steps: Vec<String>,
    },
    OptionsShown {
        entity_type: String,
        options: Vec<EntityOption>,
    },
    ValueConverted {
        field_name: String,
        new_value: EntityValue,
        success: bool,
    },
    OverrideRequested {
        reason_required: bool,
        approval_needed: bool,
    },
}
```

---

## AI Chat Node Implementation

### Complete Conversational Interface

```rust
impl AIChatNode {
    pub async fn process_message(
        &mut self,
        user_message: &str,
        services: &AIServices
    ) -> Result<ChatMessage, Error> {
        // 1. Add user message to conversation
        self.conversation.push(ChatMessage {
            role: MessageRole::User,
            content: user_message.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
        });
        
        // 2. Build conversation context
        let context = self.build_conversation_context(services).await?;
        
        // 3. Classify intent
        let intent_classification = services.intent_classifier
            .classify_message(user_message, &context)
            .await?;
        
        // 4. Process based on intent
        let response = match intent_classification.intent {
            ChatIntent::RAGQuery => {
                services.rag_processor
                    .process_rag_query(user_message, &context)
                    .await?
            },
            ChatIntent::ContentGeneration => {
                services.content_generator
                    .generate_content(&ContentGenerationRequest {
                        description: user_message.to_string(),
                        style: context.user_preferences.content_style.clone(),
                        target_length: None,
                        format: None,
                        context_nodes: Some(context.active_entities.clone()),
                    }, &context)
                    .await?
            },
            ChatIntent::EntityCRUD => {
                services.entity_crud_processor
                    .process_entity_crud(user_message, &context)
                    .await?
            },
            ChatIntent::ValidationQuery => {
                services.validation_explainer
                    .explain_validation_rules(user_message, &context)
                    .await?
            },
            ChatIntent::GeneralKnowledge => {
                services.general_knowledge_processor
                    .process_general_question(user_message)
                    .await?
            },
            ChatIntent::NodeManipulation => {
                services.node_manipulator
                    .process_node_operation(user_message, &context)
                    .await?
            },
            ChatIntent::SystemQuery => {
                services.system_query_processor
                    .process_system_query(user_message, &context)
                    .await?
            },
        };
        
        // 5. Create assistant response
        let assistant_message = ChatMessage {
            role: MessageRole::Assistant,
            content: response.message,
            timestamp: Utc::now(),
            metadata: response.metadata,
        };
        
        // 6. Add to conversation and update context
        self.conversation.push(assistant_message.clone());
        self.update_context_sources(response.sources);
        self.intent = intent_classification.intent;
        
        // 7. Handle any generated content or actions
        if let Some(generated_content) = response.generated_content {
            self.generated_content.push(generated_content);
        }
        
        Ok(assistant_message)
    }
    
    async fn build_conversation_context(&self, services: &AIServices) -> Result<ConversationContext, Error> {
        // Get recent conversation history
        let recent_messages: Vec<ChatMessage> = self.conversation.iter()
            .rev() // Most recent first
            .take(10) // Last 10 messages
            .cloned()
            .collect();
        
        // Determine current node context
        let current_node_context = if let Some(parent_id) = &self.base.parent_id {
            Some(parent_id.clone())
        } else {
            None
        };
        
        // Get active entities from recent context
        let active_entities = self.extract_active_entities(&recent_messages, services).await?;
        
        // Load user preferences
        let user_preferences = services.user_preferences
            .load_preferences(&self.base.id)
            .await
            .unwrap_or_default();
        
        Ok(ConversationContext {
            recent_messages,
            current_node_context,
            active_entities,
            user_preferences,
        })
    }
    
    async fn extract_active_entities(
        &self,
        recent_messages: &[ChatMessage],
        services: &AIServices
    ) -> Result<Vec<ContextNode>, Error> {
        let mut active_entities = Vec::new();
        
        // Extract entity references from recent messages
        for message in recent_messages.iter().take(5) { // Look at last 5 messages
            let entity_refs = services.entity_extractor
                .extract_entity_references(&message.content)
                .await?;
            
            for entity_ref in entity_refs {
                if let Ok(entity) = services.entity_manager.load_entity(&entity_ref.entity_id).await {
                    active_entities.push(ContextNode {
                        id: entity.base.id,
                        title: entity.base.content.clone(),
                        summary: self.summarize_entity(&entity),
                        node_type: entity.entity_type,
                    });
                }
            }
        }
        
        // Deduplicate by ID
        active_entities.sort_by(|a, b| a.id.cmp(&b.id));
        active_entities.dedup_by(|a, b| a.id == b.id);
        
        Ok(active_entities)
    }
}

#[derive(Debug, Clone)]
pub struct AIServices {
    pub intent_classifier: Arc<IntentClassifier>,
    pub rag_processor: Arc<RAGProcessor>,
    pub content_generator: Arc<ContentGenerator>,
    pub entity_crud_processor: Arc<EntityCRUDProcessor>,
    pub validation_explainer: Arc<ValidationExplainer>,
    pub general_knowledge_processor: Arc<GeneralKnowledgeProcessor>,
    pub node_manipulator: Arc<NodeManipulator>,
    pub system_query_processor: Arc<SystemQueryProcessor>,
    pub entity_manager: Arc<EntityManager>,
    pub entity_extractor: Arc<EntityExtractor>,
    pub user_preferences: Arc<UserPreferencesManager>,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, Value>,
}

#[derive(Debug, Clone)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}

#[derive(Debug, Clone)]
pub struct UserPreferences {
    pub content_style: ContentStyle,
    pub response_length: ResponseLength,
    pub technical_level: TechnicalLevel,
    pub preferred_examples: bool,
    pub auto_create_nodes: bool,
}

#[derive(Debug, Clone)]
pub enum ResponseLength {
    Brief,    // 1-2 sentences
    Standard, // 1-2 paragraphs  
    Detailed, // Multiple paragraphs with examples
}

#[derive(Debug, Clone)]
pub enum TechnicalLevel {
    Beginner,     // Avoid technical jargon
    Intermediate, // Some technical terms with explanation
    Expert,       // Full technical detail
}
```

---

## Performance and Caching

### AI Response Optimization

```rust
pub struct AIResponseCache {
    intent_cache: LruCache<String, (ChatIntent, f32)>,
    rag_cache: LruCache<String, RAGResponse>,
    content_cache: LruCache<String, GeneratedContent>,
    entity_query_cache: LruCache<String, Vec<EntityNode>>,
}

impl AIResponseCache {
    pub fn new() -> Self {
        AIResponseCache {
            intent_cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            rag_cache: LruCache::new(NonZeroUsize::new(500).unwrap()),
            content_cache: LruCache::new(NonZeroUsize::new(200).unwrap()),
            entity_query_cache: LruCache::new(NonZeroUsize::new(300).unwrap()),
        }
    }
    
    pub fn get_cached_intent(&mut self, message: &str) -> Option<(ChatIntent, f32)> {
        self.intent_cache.get(message).cloned()
    }
    
    pub fn cache_intent(&mut self, message: String, intent: ChatIntent, confidence: f32) {
        self.intent_cache.put(message, (intent, confidence));
    }
    
    pub fn invalidate_related_caches(&mut self, invalidation_event: &CacheInvalidationEvent) {
        match invalidation_event {
            CacheInvalidationEvent::EntityChanged { entity_type, entity_id } => {
                // Invalidate entity query cache for this type
                self.entity_query_cache.clear(); // Simplified - could be more targeted
                
                // Invalidate RAG cache entries that might reference this entity
                self.rag_cache.clear(); // Simplified - could scan for references
            },
            CacheInvalidationEvent::SchemaChanged { entity_type } => {
                // Clear all caches related to this entity type
                self.entity_query_cache.clear();
                self.content_cache.clear(); // Content generation might reference schema
            },
            CacheInvalidationEvent::ValidationRulesChanged => {
                // Keep intent and RAG cache, clear others
                self.entity_query_cache.clear();
                self.content_cache.clear();
            },
        }
    }
}

#[derive(Debug, Clone)]
pub enum CacheInvalidationEvent {
    EntityChanged { entity_type: String, entity_id: String },
    SchemaChanged { entity_type: String },
    ValidationRulesChanged,
}

// Background cache warming
pub struct CacheWarmer {
    cache: Arc<Mutex<AIResponseCache>>,
    nlp_engine: Arc<dyn NLPEngine>,
}

impl CacheWarmer {
    pub async fn warm_common_intents(&self) -> Result<(), Error> {
        let common_phrases = vec![
            "Show me all employees",
            "Create a new project",
            "What are the validation rules for customers?",
            "Generate a status report",
            "Help me understand this error",
        ];
        
        for phrase in common_phrases {
            let intent = self.classify_intent_for_warming(phrase).await?;
            let mut cache = self.cache.lock().await;
            cache.cache_intent(phrase.to_string(), intent.0, intent.1);
        }
        
        Ok(())
    }
}
```

---

This AI Integration Specification provides a comprehensive foundation for natural language processing across all NodeSpace operations. The system intelligently routes user requests through appropriate AI handlers while maintaining conversation context and providing helpful, actionable responses. The integration enables users to interact with complex data and validation systems using natural language, making the system truly AI-native.