# NodeSpace Architecture Specification

## Executive Summary

NodeSpace is an AI-native knowledge management system built around a hierarchical block-node interface. The architecture combines Rust backend services with a Svelte frontend, packaged as a Tauri desktop application. The system supports multiple node types (Text, Task, AI Chat, Entity, PDF, Query) with real-time updates, natural language interactions, and sophisticated validation rules.

### Key Architectural Decisions

- **Framework**: Svelte + Tauri (migrating from React for simplicity and performance)
- **Backend**: Rust with trait-based plugin system
- **Node Types**: Core types (Text, Task, AI Chat, Entity, Query) + extensible plugin system
- **AI Integration**: Native LLM integration for CRUD operations, validation, and content generation
- **Build Strategy**: Build-time plugin compilation for parallel development
- **Real-time Updates**: Live query nodes with automatic result synchronization

---

## Core Architecture

### Technology Stack

```
┌─────────────────────────────────────────────────────────────┐
│                    Tauri Desktop App                        │
├─────────────────────────────────────────────────────────────┤
│  Frontend (Svelte)           │  Backend (Rust)              │
│  ├── Node Components         │  ├── Core Services           │
│  ├── Query Views             │  ├── Entity Management       │
│  ├── AI Chat Interface       │  ├── Validation Engine       │
│  └── Plugin Loader           │  └── Plugin Registry         │
├─────────────────────────────────────────────────────────────┤
│                   Node Type System                          │
│  ┌─────────┬─────────┬─────────┬─────────┬─────────┬──────────┐ │
│  │  Text   │  Task   │AIChat   │ Entity  │  Query  │ Plugins  │ │  
│  │  Node   │  Node   │ Node    │  Node   │  Node   │(PDF,etc) │ │
│  └─────────┴─────────┴─────────┴─────────┴─────────┴──────────┘ │
├─────────────────────────────────────────────────────────────┤
│                     Data Layer                              │
│  ├── Unified Database (LanceDB)                             │
│  ├── File System (raw content)                              │
│  └── NLP Engine (mistral.rs)                                │
└─────────────────────────────────────────────────────────────┘
```

### Repository Structure

```
nodespace-core/                    # Main repository with workspace
├── Cargo.toml                     # Root workspace configuration
├── package.json                   # Bun workspace root
├── src/
│   ├── lib.rs
│   ├── traits/                    # Plugin interfaces
│   ├── nlp/                       # NLP engine (migrated)
│   ├── storage/                   # Data store (migrated)
│   ├── services/                  # Service implementations
│   │   ├── entity_manager.rs      # Entity CRUD operations
│   │   ├── query_result_manager.rs # Real-time query updates
│   │   └── calculation_engine.rs  # Field calculations
│   ├── validation/                # Validation subsystem
│   │   ├── validation_engine.rs   # Core validation logic
│   │   ├── formula_engine.rs      # Expression evaluation
│   │   └── rule_generator.rs      # Natural language to rules
│   ├── node_types/                # Core node implementations
│   │   ├── text_node.rs           # Foundation text node
│   │   ├── task_node.rs           # Task management
│   │   ├── ai_chat_node.rs        # AI interaction hub
│   │   ├── entity_node.rs         # Custom structured entities
│   │   └── query_node.rs          # Live data views
│   └── ui/                        # Core Svelte components
│       ├── TextNode.svelte        # Text node UI
│       ├── TaskNode.svelte        # Task node UI
│       ├── AIChatNode.svelte      # AI chat UI
│       ├── EntityNode.svelte      # Entity node UI
│       ├── QueryNode.svelte       # Query node UI
│       └── BaseNodeWrapper.svelte # Shared node behaviors
├── nodespace-app/                 # Tauri desktop application
│   ├── src-tauri/                 # Rust backend
│   │   ├── Cargo.toml
│   │   ├── src/
│   │   │   ├── main.rs
│   │   │   ├── commands.rs        # Tauri command handlers
│   │   │   └── plugin_registry.rs # Plugin management
│   ├── src/                       # Svelte frontend
│   │   ├── App.svelte
│   │   ├── main.js
│   │   └── components/
│   │       ├── NodeRenderer.svelte    # Component dispatcher
│   │       ├── NodeTree.svelte        # Hierarchy manager
│   │       └── QueryView.svelte       # Query results display
│   └── package.json
└── README.md

nodespace-pdf-node/                # PDF plugin repository
├── Cargo.toml
├── package.json
├── src/
│   ├── lib.rs                     # Rust plugin logic
│   └── ui/                        # Svelte components
│       ├── PDFNode.svelte
│       └── PDFViewer.svelte
└── dist/                          # Built plugin assets

nodespace-image-node/              # Image plugin repository
nodespace-code-node/               # Code plugin repository
```

---

## Node Type System

### Core Node Types (Built-in)

The system includes five essential node types that form the foundation of the knowledge management system:

#### 1. TextNode - Foundation Node Type
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextNode {
    pub id: String,
    pub content: String,                    // Markdown-supported text
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub metadata: HashMap<String, Value>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

impl NodeBehavior for TextNode {
    fn node_type(&self) -> &'static str { "text" }
    fn can_have_children(&self) -> bool { true }
    fn supports_markdown(&self) -> bool { true }
}
```

**Features:**
- In-place editing with auto-save
- Markdown rendering when not focused
- Hierarchical nesting with visual indentation
- Keyboard navigation (arrows, Enter, Backspace)
- Auto-resize textareas

#### 2. TaskNode - Todo Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub base: TextNode,
    pub status: TaskStatus,                 // Pending, InProgress, Completed
    pub due_date: Option<DateTime<Utc>>,
    pub priority: TaskPriority,             // Low, Medium, High, Critical
    pub assignee: Option<String>,
    pub estimated_hours: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    Pending,
    InProgress, 
    Completed,
    Cancelled,
}
```

**Features:**
- Status tracking with visual indicators
- Due date management with overdue highlighting
- Priority-based sorting and filtering
- Assignment to users
- Time estimation and tracking

#### 3. AIChatNode - AI Interaction Hub
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIChatNode {
    pub base: TextNode,
    pub conversation: Vec<ChatMessage>,
    pub context_sources: Vec<ContextSource>,
    pub intent: ChatIntent,
    pub generated_content: Vec<GeneratedContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ChatIntent {
    RAGQuery,           // "What is our deployment process?"
    ContentGeneration,  // "Draft an onboarding checklist"
    GeneralKnowledge,   // "What is Rust ownership?"
    EntityCRUD,         // "Add a new employee named John"
    NodeManipulation,   // "Create a task for code review"
}
```

**Features:**
- Multi-mode AI interaction (RAG, generation, general knowledge, CRUD)
- Intent classification for appropriate response handling
- Context-aware responses with source attribution
- Content generation with node creation capabilities
- Conversational validation error handling

#### 4. EntityNode - Structured Data Management
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityNode {
    pub base: TextNode,
    pub entity_type: String,                    // "employee", "customer", "project"
    pub stored_fields: HashMap<String, EntityValue>,
    pub calculated_fields: HashMap<String, CalculatedField>,
    pub schema: EntitySchema,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntitySchema {
    pub name: String,
    pub stored_fields: Vec<FieldDefinition>,
    pub calculated_fields: Vec<CalculatedField>,
    pub validation_rules: Vec<ValidationRule>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FieldType {
    Text,
    Number,
    Date,
    Boolean,
    Email,
    Phone,
    Reference(String),              // Reference to another entity type
    List(Box<FieldType>),          // Array of field type
}
```

**Entity Examples:**
- **Employee**: name, role, salary, start_date, manager_id
- **Customer**: name, email, total_purchases, vip_status, registration_date
- **Project**: title, description, start_date, end_date, team_members, budget
- **Invoice**: customer_id, amount, due_date, line_items, payment_status

#### Calculated Fields - Excel-like Formulas
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalculatedField {
    pub name: String,
    pub formula: String,                    // "=first_name + ' ' + last_name"
    pub return_type: FieldType,
    pub dependencies: Vec<String>,          // Fields this formula depends on
    pub cache_value: Option<EntityValue>,   // Cached result
}

// Examples:
// full_name = first_name + " " + last_name
// total_compensation = salary + bonus
// years_employed = ROUND(DAYS(start_date, TODAY()) / 365.25, 1)
// days_overdue = DAYS(due_date, TODAY())
// status = IF(days_overdue > 0, "Overdue", "Current")
```

**Calculation Features:**
- **Reactive Updates**: Automatically recalculate when dependencies change
- **Excel-like Functions**: ROUND, DAYS, IF, CONCATENATE, SUM, COUNT
- **Cross-field Dependencies**: Complex business logic across multiple fields
- **Caching**: Performance optimization with intelligent cache invalidation

#### 5. QueryNode - Live Data Views
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryNode {
    pub base: TextNode,
    pub query: QueryDefinition,
    pub view_calculated_fields: Vec<ViewCalculatedField>,
    pub results: Vec<String>,               // Cached result node IDs
    pub last_executed: DateTime<Utc>,
    pub auto_refresh: bool,
    pub refresh_triggers: Vec<RefreshTrigger>,
}
```

**Features:**
- **Real-time Updates**: Automatically refresh when underlying data changes
- **Complex Queries**: Filter, sort, and aggregate across multiple entity types
- **View-Specific Calculations**: Rankings, percentiles, and aggregate metrics
- **Live Synchronization**: Changes to source nodes instantly update query results

### External Node Types (Plugin System)

#### PDF Node Plugin
```rust
// nodespace-pdf-node/src/lib.rs
pub struct PDFNode {
    pub base: TextNode,
    pub file_path: String,
    pub page_count: u32, 
    pub extracted_text: String,
    pub thumbnail_path: Option<String>,
}

impl NodePlugin for PDFNode {
    fn get_component_name(&self) -> &'static str { "PDFNode" }
    
    async fn save(&self, services: &Services) -> Result<(), Error> {
        // 1. Save raw PDF file
        let file_path = services.storage.save_file(&self.id, &pdf_bytes, "pdf").await?;
        
        // 2. Extract text content  
        let text_content = extract_text(&pdf_bytes)?;
        
        // 3. Generate multi-level embeddings
        let embeddings = self.generate_pdf_embeddings(&text_content, services).await?;
        
        // 4. Save to vector database
        services.storage.save_embeddings(&self.id, embeddings).await?;
        Ok(())
    }
}
```

**Plugin Features:**
- **Self-contained**: Each plugin repo contains Rust backend + Svelte UI
- **Service Injection**: Access to NLP, search, storage services  
- **Independent Development**: Build and test with real service implementations
- **Full Integration**: Complete access to validation, calculation, and real-time systems

---

## Real-Time Query System

### Query Architecture Overview
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryNode {
    pub base: TextNode,
    pub query: QueryDefinition,
    pub view_calculated_fields: Vec<ViewCalculatedField>,
    pub results: Vec<String>,               // Cached result node IDs
    pub last_executed: DateTime<Utc>,
    pub auto_refresh: bool,
    pub refresh_triggers: Vec<RefreshTrigger>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryDefinition {
    pub name: String,                       // "Tasks due this week"
    pub entity_types: Vec<String>,          // ["task"]
    pub filters: Vec<QueryFilter>,
    pub sort_by: Option<String>,
    pub limit: Option<usize>,
    pub fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RefreshTrigger {
    FieldChange { entity_type: String, field: String },
    NodeCreate { entity_type: String },
    NodeDelete { entity_type: String },
    TimeInterval { minutes: u32 },
    Manual,
}
```

### Real-time Update System
```rust
pub struct QueryResultManager {
    active_queries: HashMap<String, ActiveQuery>,
    change_listeners: HashMap<String, Vec<String>>,
}

impl QueryResultManager {
    pub async fn handle_node_change(
        &mut self,
        changed_node_id: &str,
        field_changes: &HashMap<String, EntityValue>,
        services: &Services
    ) -> Result<Vec<QueryUpdate>, Error> {
        // 1. Find affected queries
        let affected_queries = self.find_affected_queries(changed_node_id, field_changes);
        
        // 2. Re-execute affected queries
        // 3. Calculate incremental changes
        // 4. Notify UI components
        
        Ok(updates)
    }
}
```

**Query Examples:**
- **"Tasks due this week"**: Auto-refreshes when task due dates change
- **"Overdue invoices"**: Updates when payment status changes
- **"Top performers by salary"**: Recalculates when salary fields update
- **"Recent chat conversations"**: Updates when new AI chat messages added

### Query-Level Calculated Fields
```rust
// Different from entity-level calculated fields
// These are context-specific to the query/view

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CalculationScope {
    PerRow,        // Calculate for each result row: urgency_score = days_overdue * amount
    Aggregate,     // Calculate across all results: total_revenue = SUM(amount)
    Window,        // Ranking/windowing: rank_by_salary = RANK() OVER (ORDER BY salary DESC)
}

// Examples for "Employee Salary Report" query:
// - rank_by_salary = RANK() OVER (ORDER BY total_compensation DESC)
// - salary_percentile = PERCENTILE(total_compensation)  
// - deviation_from_avg = total_compensation - AVG(total_compensation)
```

---

## AI Integration

### Intent Classification System
```rust
impl AIChatNode {
    async fn classify_intent(&self, user_message: &str, nlp: &dyn NLPEngine) -> Result<ChatIntent, Error> {
        let classification_prompt = format!(r#"
Classify this user message into one of these categories:

1. RAG_QUERY: User wants information from their existing knowledge base
2. CONTENT_GENERATION: User wants to create new content/documents  
3. GENERAL_KNOWLEDGE: User asks about general topics not in their knowledge base
4. ENTITY_CRUD: User wants to create/modify structured data entities
5. NODE_MANIPULATION: User wants to create/modify nodes directly

User message: "{}"

Respond with just the category name.
"#, user_message);

        let response = nlp.generate_text(&classification_prompt, "").await?;
        // Parse and return intent
    }
}
```

### Multi-Mode AI Processing

#### 1. RAG Query Processing
```rust
async fn handle_rag_query(&mut self, query: &str, services: &Services) -> Result<ChatMessage, Error> {
    // 1. Search knowledge base
    let query_embedding = services.nlp_engine.embed_query(query).await?;
    let search_results = services.search_engine.similarity_search(&query_embedding).await?;
    
    // 2. Generate response with context
    let rag_prompt = format!(r#"
Answer the user's question using only the provided context.
Question: {}
Context: {}
"#, query, context);
    
    let response = services.nlp_engine.generate_text(&rag_prompt, &context).await?;
    // Return with source attribution
}
```

#### 2. Content Generation
```rust
async fn handle_content_generation(&mut self, request: &str, services: &Services) -> Result<ChatMessage, Error> {
    // 1. Generate content
    let generated_content = services.nlp_engine.generate_text(&request, "").await?;
    
    // 2. Offer to create nodes
    let response = format!(r#"
I've generated this content for you:

{}

Would you like me to:
1. Create a new text node with this content
2. Create multiple nodes (if this should be broken down)  
3. Just keep it in this chat
"#, generated_content);
    
    Ok(response)
}
```

#### 3. Entity CRUD Processing
```rust
async fn handle_entity_crud(&mut self, message: &str, services: &Services) -> Result<ChatMessage, Error> {
    // 1. Detect operation type (CREATE, READ, UPDATE, DELETE, ANALYZE)
    let operation = self.parse_entity_operation(message).await?;
    
    match operation {
        EntityOperation::Create => {
            // Extract field values from natural language
            // Validate against schema
            // Create entity
        },
        EntityOperation::Analyze => {
            // "How many invoices are past due over 90 days?"
            // Parse analytical query
            // Execute with aggregations
            // Format results naturally
        },
        // ... other operations
    }
}
```

### Conversational Validation Handling

When AI operations fail validation, the system provides conversational feedback:

```
User: "Set John's status to VIP"

AI processes → Validation fails (insufficient purchases)

AI Response: "I can't set John's status to VIP because he only has $3,200 in total purchases, and our business rule requires at least $5,000 for VIP status.

Here are a few options:
1. **Increase his purchase history**: Add $1,800 more in purchases  
2. **Override the rule**: Set him to VIP anyway with manager approval
3. **Use a different status**: Set him to 'Premium' instead

Would you like me to help with any of these options?"
```

---

## Validation System

### Natural Language Validation Rules

Users can define validation rules in plain English, which the AI converts to executable logic:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationRule {
    pub id: String,
    pub field_name: String,
    pub description: String,               // User's natural language description
    pub rule_type: ValidationType,
    pub expression: String,                // Generated validation expression
    pub error_message: String,            // User-friendly error message
    pub severity: ValidationSeverity,
}

// User: "If a user's total purchases are below 5000, they cannot be marked as 'VIP' status"
// Generates:
ValidationRule {
    field_name: "status",
    description: "VIP status requires minimum $5000 in purchases",
    rule_type: ValidationType::Conditional,
    expression: "total_purchases < 5000 IMPLIES status != 'VIP'",
    error_message: "Users with less than $5000 in purchases cannot have VIP status",
    severity: ValidationSeverity::Error,
}
```

### Cross-Field Validation Types

#### 1. Conditional Logic
```rust
// "If project status is 'Complete', end date must be filled in"
condition_expression: "status == 'Complete'"
validation_expression: "end_date != null AND end_date != ''"
```

#### 2. Relationship Validation  
```rust
// "Manager salary must be higher than their team members"
condition_expression: "role == 'Manager'"
validation_expression: "salary > MAX(team_members.salary)"
```

#### 3. Business Rule Enforcement
```rust
// "Premium customers must have all premium features enabled"
condition_expression: "customer_tier == 'Premium'"
validation_expression: "premium_features_enabled == true"
```

#### 4. Temporal Validation
```rust
// "End date must be after start date and in the future"
validation_expression: "end_date > start_date AND IS_FUTURE(end_date)"
```

### Validation Engine
```rust
pub struct ValidationEngine {
    formula_engine: FormulaEngine,
}

impl ValidationEngine {
    pub async fn validate_entity(
        &self,
        entity: &EntityNode,
        changed_fields: Option<&[String]>,
        context: &ValidationContext
    ) -> Result<ValidationResult, Error> {
        // 1. Get all field values (including calculated fields)
        // 2. Run applicable validation rules
        // 3. Return structured results with violation details
    }
}
```

---

## Plugin Architecture

### Build-Time Plugin System

The system uses **build-time compilation** rather than dynamic loading for simplicity and performance:

```toml
# Main app Cargo.toml
[dependencies]
nodespace-pdf-node = { path = "../nodespace-pdf-node" }
nodespace-image-node = { path = "../nodespace-image-node" }
nodespace-code-node = { path = "../nodespace-code-node" }
```

### Plugin Development Pattern

#### 1. Service Injection
```rust
pub trait NodePlugin {
    fn new(
        storage: Arc<dyn NodeStorage>,
        nlp_engine: Arc<dyn NLPEngine>,
        search_engine: Arc<dyn SearchEngine>,
        text_processor: Arc<dyn TextProcessor>,
    ) -> Self;
}
```

#### 2. Mixed Rust + Svelte Structure
```
nodespace-pdf-node/
├── Cargo.toml               # Rust dependencies
├── package.json             # Svelte dependencies  
├── src/
│   ├── lib.rs              # Rust plugin logic
│   ├── pdf_commands.rs     # Tauri commands
│   └── ui/                 # Svelte components
│       ├── PDFNode.svelte
│       └── PDFViewer.svelte
└── dist/                   # Built assets
```

#### 3. Independent Development with Real Services
```rust
// Plugin testing with real service implementations
impl PDFPlugin {
    #[cfg(test)]
    pub fn new_for_testing() -> Self {
        let services = create_test_services(); // Real implementations
        Self::new(
            services.storage,
            services.nlp_engine,
            services.search_engine,
            services.text_processor,
        )
    }
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_pdf_processing() {
        let plugin = PDFPlugin::new_for_testing();
        let pdf_bytes = include_bytes!("../test_data/sample.pdf");
        
        // Test with real PDF processing and storage
        let result = plugin.save("test_node", pdf_bytes).await;
        assert!(result.is_ok());
        
        // Verify real text extraction and embeddings
        let extracted = plugin.extract_text("test_node").await?;
        assert!(extracted.contains("expected content"));
    }
}
```

### Plugin Registration
```rust
// Main app registers all plugins at compile time
pub fn register_all_plugins() -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    
    registry.register("pdf", Box::new(PDFPlugin::new(
        services.storage.clone(),
        services.nlp_engine.clone(),
        services.search_engine.clone(),
        services.text_processor.clone(),
    )));
    
    registry.register("image", Box::new(ImagePlugin::new(/* ... */)));
    registry.register("code", Box::new(CodePlugin::new(/* ... */)));
    
    registry
}
```

---

## Migration Strategy

### From Current React Codebase

#### Current State Assessment
- **nodespace-core-ui**: ~8,000 lines React/TypeScript, 69 files
- **Complex patterns**: useRef management, custom hooks, prop drilling
- **Heavy dependencies**: react-textarea-autosize, react-markdown

#### Migration Benefits
- **~80% code reduction** with Svelte's reactive approach
- **Elimination of complex patterns**: refs, useEffect, callback hell
- **Better performance**: compilation vs runtime framework
- **Cleaner architecture**: built-in state management

#### Development Stages

**Stage 1: Core Architecture**
- Set up Svelte + Tauri project structure
- Implement base node system with reactive stores
- Convert TextNode and TaskNode components
- Create basic hierarchy rendering

**Stage 2: AI Integration**
- Implement AIChatNode with intent classification
- Add entity CRUD operations via AI
- Create validation system with natural language rules
- Build conversational error handling

**Stage 3: Advanced Features**
- Implement QueryNode with real-time updates
- Add calculated fields system
- Create plugin architecture
- Build PDF/Image node plugins

**Stage 4: Production Readiness**
- Performance optimization
- Comprehensive testing
- Documentation and deployment

### Repository Migration Plan

#### Consolidation Strategy
```
MERGE INTO nodespace-core/:
✅ nodespace-core-logic → src/services/
✅ nodespace-nlp-engine → src/nlp/
✅ nodespace-data-store → src/storage/

KEEP SEPARATE:
✅ nodespace-core-ui → Reference for UI patterns
🆕 nodespace-pdf-node → First plugin example
🆕 nodespace-image-node → Second plugin
```

---

## Development Workflow

### Build System Integration

#### Bun-Based Build Process
```json
// Root package.json
{
  "scripts": {
    "dev": "bun run dev:plugins && bun run dev:app", 
    "dev:plugins": "bun run --parallel dev:pdf-plugin dev:image-plugin",
    "build": "bun run build:plugins && bun run build:app",
    "test": "bun run test:plugins && bun run test:app"
  },
  "workspaces": [
    "nodespace-app",
    "nodespace-pdf-node",
    "nodespace-image-node"
  ]
}
```

#### Plugin Build Script
```javascript
// nodespace-pdf-node/build.js
#!/usr/bin/env bun

console.log("🔨 Building PDF Plugin...");

try {
  // Build Svelte components
  await $`bun run vite build`;
  
  // Build Rust plugin
  await $`cargo build --release`;
  
  // Copy artifacts
  await $`cp target/release/libnodespace_pdf_node.* dist/`;
  
  console.log("✅ PDF Plugin build complete!");
} catch (error) {
  console.error("❌ Build failed:", error);
  process.exit(1);
}
```

### Testing Strategy

#### Multi-Level Testing
```rust
// Entity-level tests
#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_calculated_field_updates() {
        let mut employee = create_test_employee();
        
        // Update base salary
        employee.update_field("salary", EntityValue::Number(95000.0));
        
        // Verify calculated fields update
        let total_comp = employee.get_field_value("total_compensation")?;
        assert_eq!(total_comp, EntityValue::Number(110000.0)); // salary + bonus
    }
    
    #[tokio::test] 
    async fn test_cross_field_validation() {
        let employee = create_test_employee();
        let validation_result = validate_vip_status_rule(&employee).await?;
        
        assert!(!validation_result.is_valid);
        assert!(validation_result.violations[0].message.contains("$5000"));
    }
}
```

#### Real Integration Testing
```rust
// Testing with real services and database operations
#[cfg(test)]
mod integration_tests {
    use super::*;
    
    #[tokio::test]
    async fn test_pdf_full_integration() {
        let services = create_real_test_services().await;
        let pdf_plugin = PDFPlugin::new(
            services.storage.clone(),
            services.nlp_engine.clone(),
            services.search_engine.clone(),
            services.text_processor.clone(),
        );
        
        let pdf_bytes = include_bytes!("../test_data/sample.pdf");
        
        // Test real PDF processing
        let result = pdf_plugin.save("test_node", pdf_bytes).await;
        assert!(result.is_ok());
        
        // Verify real embeddings were generated
        let embeddings = services.storage.get_embeddings("test_node").await?;
        assert!(!embeddings.is_empty());
        
        // Test real search functionality
        let search_results = services.search_engine
            .similarity_search(&embeddings[0].embedding, Default::default())
            .await?;
        assert!(!search_results.is_empty());
    }
    
    #[tokio::test]
    async fn test_validation_integration() {
        let services = create_real_test_services().await;
        
        // Test that validation rules work with actual PDF content
        let validation_result = services.validation_engine
            .validate_pdf_node(&invalid_pdf_node)
            .await?;
            
        assert!(!validation_result.is_valid);
        assert!(validation_result.violations[0].message.contains("PDF"));
    }
}
```

### Performance Considerations

#### Optimization Strategies
1. **Incremental Calculation**: Only recalculate changed dependencies
2. **Query Result Caching**: Cache query results with smart invalidation
3. **Lazy Loading**: Load plugin components on demand
4. **Batch Updates**: Group multiple field changes for efficient validation
5. **Background Processing**: Heavy operations (PDF text extraction) in background

#### Monitoring Points
- Query execution times
- Validation rule performance
- Plugin load times
- Memory usage for large documents
- Real-time update latency

---

## Technical Implementation Details

### Core Traits System
```rust
// Foundation traits that all node types implement
pub trait NodeBehavior {
    fn node_type(&self) -> &'static str;
    fn can_have_children(&self) -> bool;
    fn supports_markdown(&self) -> bool;
}

pub trait NodeRenderer {
    fn render_preview(&self) -> String;
    fn get_component_name(&self) -> &'static str;
}

pub trait NodeStorage {
    async fn save(&self, storage: &dyn Storage) -> Result<(), Error>;
    async fn load(id: &str, storage: &dyn Storage) -> Result<Self, Error> where Self: Sized;
    async fn generate_embeddings(&self, nlp: &dyn NLPEngine) -> Result<Vec<EmbeddingData>, Error>;
}
```

### Formula Engine Integration
```rust
// Rhai-based formula engine for calculated fields and validations
pub struct FormulaEngine {
    engine: rhai::Engine,
}

impl FormulaEngine {
    pub fn new() -> Self {
        let mut engine = rhai::Engine::new();
        
        // Register business functions
        engine.register_fn("DAYS", |start: i64, end: i64| (end - start) / 86400);
        engine.register_fn("IS_EMAIL", |email: String| validate_email(&email));
        engine.register_fn("ROUND", |num: f64, digits: i64| round_to_digits(num, digits));
        
        Self { engine }
    }
    
    pub fn evaluate(&self, formula: &str, field_values: &HashMap<String, EntityValue>) -> Result<EntityValue, Error> {
        let mut scope = rhai::Scope::new();
        
        // Add field values to scope
        for (name, value) in field_values {
            self.add_field_to_scope(&mut scope, name, value);
        }
        
        // Evaluate formula
        let result = self.engine.eval_with_scope(&mut scope, formula)?;
        self.convert_to_entity_value(result)
    }
}
```

### Real-time Update Architecture
```rust
// Event-driven update system
pub struct UpdateCoordinator {
    query_manager: QueryResultManager,
    validation_engine: ValidationEngine,
    event_subscribers: Vec<UpdateSubscriber>,
}

impl UpdateCoordinator {
    pub async fn handle_entity_change(&mut self, change: EntityChange) -> Result<(), Error> {
        // 1. Validate the change
        let validation_result = self.validation_engine.validate_change(&change).await?;
        
        if !validation_result.is_valid {
            return Err(ValidationError::from(validation_result));
        }
        
        // 2. Apply the change
        self.apply_entity_change(&change).await?;
        
        // 3. Update affected queries
        let query_updates = self.query_manager.handle_node_change(
            &change.node_id,
            &change.field_changes
        ).await?;
        
        // 4. Notify subscribers
        for update in query_updates {
            self.notify_subscribers(&update).await?;
        }
        
        Ok(())
    }
}
```

---

## Conclusion

This architecture provides a robust foundation for an AI-native knowledge management system with the following key benefits:

### For Users
- **Natural language interaction** for all operations
- **Real-time updates** across all views and queries
- **Flexible data modeling** with custom entities and validation
- **Powerful search and analysis** capabilities

### For Developers  
- **Clean separation of concerns** with trait-based architecture
- **Independent plugin development** with full testing capabilities
- **Type-safe interfaces** throughout the Rust backend
- **Reactive UI updates** with Svelte's efficient reactivity

### For the Business
- **Rapid feature development** through plugin architecture  
- **Scalable validation system** that grows with business rules
- **AI-native design** that leverages LLM capabilities effectively
- **Desktop-first experience** optimized for knowledge work

The migration from React to Svelte, combined with the Rust backend and plugin architecture, creates a maintainable, performant, and extensible system that can evolve with changing requirements while maintaining a consistent user experience.

---

*This specification serves as the foundation for architectural review and implementation planning. Each section can be expanded with additional technical details as needed during development.*