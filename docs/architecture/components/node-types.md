# Node Types Architecture

## Overview

NodeSpace's core functionality is built around five fundamental node types that represent different ways of organizing and interacting with information. Each node type is designed to integrate seamlessly with AI capabilities while maintaining clear separation of concerns and extensibility through the plugin system.

## Core Node Type Hierarchy

```rust
pub trait Node: Send + Sync {
    fn id(&self) -> &str;
    fn node_type(&self) -> NodeType;
    fn parent_id(&self) -> Option<&str>;
    fn children(&self) -> &[String];
    fn created_at(&self) -> DateTime<Utc>;
    fn modified_at(&self) -> DateTime<Utc>;
    fn metadata(&self) -> &HashMap<String, String>;
    
    // AI Integration
    fn generate_embeddings(&self, ai_engine: &dyn AIEngine) -> Result<Vec<f32>, AIError>;
    fn supports_natural_language_operations(&self) -> bool { true }
    
    // Serialization
    fn to_json(&self) -> Result<serde_json::Value, SerializationError>;
    fn from_json(value: serde_json::Value) -> Result<Self, SerializationError> where Self: Sized;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NodeType {
    Text,
    Task,
    AIChat,
    Entity,
    Query,
}
```

## 1. TextNode

The foundation of content management in NodeSpace, supporting rich markdown content with AI-assisted editing capabilities.

### Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TextNode {
    pub id: String,
    pub content: String,                    // Markdown content
    pub content_type: TextContentType,      // Note, Document, Code, etc.
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub tags: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub ai_annotations: Vec<AIAnnotation>,  // AI-generated insights
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TextContentType {
    Note,           // General note-taking
    Document,       // Formal documentation
    Code,           // Code snippets with syntax highlighting
    Meeting,        // Meeting notes with structured format
    Journal,        // Personal journal entries
    Research,       // Research notes with citations
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIAnnotation {
    pub annotation_type: AnnotationType,
    pub content: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnnotationType {
    Summary,        // AI-generated summary
    KeyPoints,      // Extracted key points
    Questions,      // Suggested questions
    Related,        // Related content suggestions
    Actions,        // Identified action items
}
```

### AI Integration Features

```rust
impl TextNode {
    pub async fn ai_enhance_content(&mut self, ai_engine: &dyn AIEngine) -> Result<(), AIError> {
        // Generate summary
        let summary_prompt = format!("Summarize this content: {}", self.content);
        let summary = ai_engine.generate_text(&summary_prompt, "").await?;
        
        self.ai_annotations.push(AIAnnotation {
            annotation_type: AnnotationType::Summary,
            content: summary,
            confidence: 0.8,
            created_at: Utc::now(),
        });
        
        // Extract key points
        let key_points_prompt = format!("Extract key points from: {}", self.content);
        let key_points = ai_engine.generate_text(&key_points_prompt, "").await?;
        
        self.ai_annotations.push(AIAnnotation {
            annotation_type: AnnotationType::KeyPoints,
            content: key_points,
            confidence: 0.7,
            created_at: Utc::now(),
        });
        
        Ok(())
    }
    
    pub async fn ai_suggest_improvements(&self, ai_engine: &dyn AIEngine) -> Result<Vec<String>, AIError> {
        let prompt = format!(
            "Suggest improvements for this {} content: {}",
            self.content_type.to_string().to_lowercase(),
            self.content
        );
        
        let suggestions = ai_engine.generate_text(&prompt, "").await?;
        Ok(suggestions.lines().map(|s| s.trim().to_string()).collect())
    }
}
```

### Use Cases
- **Note Taking**: Personal and professional notes with AI enhancement
- **Documentation**: Technical documentation with automatic summarization
- **Research**: Academic research with AI-assisted analysis
- **Meeting Notes**: Structured meeting records with action item extraction

## 2. TaskNode

Project management and task tracking with AI-assisted planning and natural language task creation.

### Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskNode {
    pub id: String,
    pub title: String,
    pub description: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub due_date: Option<DateTime<Utc>>,
    pub assigned_to: Option<String>,
    pub parent_id: Option<String>,
    pub children: Vec<String>,              // Subtasks
    pub dependencies: Vec<String>,          // Other tasks this depends on
    pub tags: Vec<String>,
    pub time_estimate: Option<Duration>,
    pub time_spent: Duration,
    pub completion_percentage: f32,
    pub ai_suggestions: Vec<TaskSuggestion>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskStatus {
    NotStarted,
    InProgress,
    Blocked,
    Review,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskPriority {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSuggestion {
    pub suggestion_type: TaskSuggestionType,
    pub content: String,
    pub confidence: f32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TaskSuggestionType {
    Breakdown,      // Suggest subtasks
    Timeline,       // Suggest timeline adjustments
    Dependencies,   // Suggest task dependencies
    Resources,      // Suggest required resources
    Similar,        // Suggest similar completed tasks
}
```

### AI-Powered Task Management

```rust
impl TaskNode {
    pub async fn ai_break_down_task(&mut self, ai_engine: &dyn AIEngine) -> Result<Vec<TaskNode>, AIError> {
        let prompt = format!(
            "Break down this task into subtasks: '{}' - {}. 
            Return as JSON array with title, description, and estimated duration for each subtask.",
            self.title, self.description
        );
        
        let response = ai_engine.generate_text(&prompt, "").await?;
        let subtask_data: Vec<serde_json::Value> = serde_json::from_str(&response)?;
        
        let mut subtasks = Vec::new();
        for (i, subtask_json) in subtask_data.iter().enumerate() {
            let subtask = TaskNode {
                id: format!("{}-subtask-{}", self.id, i),
                title: subtask_json["title"].as_str().unwrap_or("Subtask").to_string(),
                description: subtask_json["description"].as_str().unwrap_or("").to_string(),
                status: TaskStatus::NotStarted,
                priority: self.priority.clone(),
                due_date: self.due_date,
                assigned_to: self.assigned_to.clone(),
                parent_id: Some(self.id.clone()),
                children: Vec::new(),
                dependencies: Vec::new(),
                tags: self.tags.clone(),
                time_estimate: subtask_json["duration"]
                    .as_str()
                    .and_then(|d| parse_duration(d).ok()),
                time_spent: Duration::zero(),
                completion_percentage: 0.0,
                ai_suggestions: Vec::new(),
                metadata: HashMap::new(),
                created_at: Utc::now(),
                modified_at: Utc::now(),
            };
            
            subtasks.push(subtask);
        }
        
        // Update parent task with subtask references
        self.children = subtasks.iter().map(|t| t.id.clone()).collect();
        
        Ok(subtasks)
    }
    
    pub async fn ai_estimate_completion_time(&self, ai_engine: &dyn AIEngine) -> Result<Duration, AIError> {
        let prompt = format!(
            "Estimate completion time for this task: '{}' - {}. 
            Consider complexity, dependencies, and similar tasks. 
            Return duration in hours as a number.",
            self.title, self.description
        );
        
        let response = ai_engine.generate_text(&prompt, "").await?;
        let hours: f32 = response.trim().parse().unwrap_or(1.0);
        
        Ok(Duration::hours(hours as i64))
    }
    
    pub async fn ai_suggest_next_actions(&self, ai_engine: &dyn AIEngine) -> Result<Vec<String>, AIError> {
        let prompt = format!(
            "Given this task status: '{}' ({}% complete), suggest next actions: {}",
            self.title, self.completion_percentage, self.description
        );
        
        let response = ai_engine.generate_text(&prompt, "").await?;
        Ok(response.lines().map(|s| s.trim().to_string()).collect())
    }
}
```

### Natural Language Task Creation

```rust
pub async fn create_task_from_natural_language(
    description: &str,
    ai_engine: &dyn AIEngine
) -> Result<TaskNode, AIError> {
    let extraction_prompt = format!(
        r#"Extract task information from: "{}"
        
        Return JSON with:
        {{
            "title": "task title",
            "description": "detailed description", 
            "priority": "Low|Medium|High|Critical",
            "due_date": "ISO date or null",
            "time_estimate_hours": number or null,
            "tags": ["tag1", "tag2"]
        }}"#,
        description
    );
    
    let response = ai_engine.generate_text(&extraction_prompt, "").await?;
    let task_data: serde_json::Value = serde_json::from_str(&response)?;
    
    let task = TaskNode {
        id: uuid::Uuid::new_v4().to_string(),
        title: task_data["title"].as_str().unwrap_or("New Task").to_string(),
        description: task_data["description"].as_str().unwrap_or("").to_string(),
        status: TaskStatus::NotStarted,
        priority: match task_data["priority"].as_str().unwrap_or("Medium") {
            "Low" => TaskPriority::Low,
            "High" => TaskPriority::High,
            "Critical" => TaskPriority::Critical,
            _ => TaskPriority::Medium,
        },
        due_date: task_data["due_date"].as_str()
            .and_then(|d| DateTime::parse_from_rfc3339(d).ok())
            .map(|d| d.with_timezone(&Utc)),
        assigned_to: None,
        parent_id: None,
        children: Vec::new(),
        dependencies: Vec::new(),
        tags: task_data["tags"].as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).map(|s| s.to_string()).collect())
            .unwrap_or_default(),
        time_estimate: task_data["time_estimate_hours"].as_f64()
            .map(|h| Duration::hours(h as i64)),
        time_spent: Duration::zero(),
        completion_percentage: 0.0,
        ai_suggestions: Vec::new(),
        metadata: HashMap::new(),
        created_at: Utc::now(),
        modified_at: Utc::now(),
    };
    
    Ok(task)
}
```

## 3. AIChatNode

Conversational AI interfaces with context awareness and memory management.

### Structure

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AIChatNode {
    pub id: String,
    pub title: String,
    pub conversation_history: Vec<ChatMessage>,
    pub context_nodes: Vec<String>,         // Related nodes for context
    pub ai_model: String,                   // Which AI model to use
    pub system_prompt: String,              // Custom system instructions
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub metadata: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub id: String,
    pub role: MessageRole,
    pub content: String,
    pub timestamp: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    pub referenced_nodes: Vec<String>,      // Nodes referenced in this message
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MessageRole {
    User,
    Assistant,
    System,
}
```

### Context-Aware Conversations

```rust
impl AIChatNode {
    pub async fn send_message(
        &mut self,
        user_message: &str,
        ai_engine: &dyn AIEngine,
        context_retriever: &dyn ContextRetriever
    ) -> Result<String, AIError> {
        // Add user message to history
        let user_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::User,
            content: user_message.to_string(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            referenced_nodes: Vec::new(),
        };
        self.conversation_history.push(user_msg);
        
        // Retrieve relevant context
        let context = context_retriever.get_relevant_context(
            user_message,
            &self.context_nodes,
            5 // max context items
        ).await?;
        
        // Build contextual prompt
        let mut prompt = self.system_prompt.clone();
        
        if !context.is_empty() {
            prompt.push_str("\n\nRelevant context:\n");
            for ctx in &context {
                prompt.push_str(&format!("- {}\n", ctx.summary));
            }
        }
        
        // Add conversation history
        prompt.push_str("\n\nConversation history:\n");
        for msg in self.conversation_history.iter().rev().take(10).rev() {
            prompt.push_str(&format!("{:?}: {}\n", msg.role, msg.content));
        }
        
        // Generate AI response
        let ai_response = ai_engine.generate_text(&prompt, user_message).await?;
        
        // Add AI response to history
        let ai_msg = ChatMessage {
            id: uuid::Uuid::new_v4().to_string(),
            role: MessageRole::Assistant,
            content: ai_response.clone(),
            timestamp: Utc::now(),
            metadata: HashMap::new(),
            referenced_nodes: context.iter().map(|c| c.node_id.clone()).collect(),
        };
        self.conversation_history.push(ai_msg);
        
        self.modified_at = Utc::now();
        
        Ok(ai_response)
    }
    
    pub fn get_conversation_summary(&self, ai_engine: &dyn AIEngine) -> Result<String, AIError> {
        let conversation_text = self.conversation_history.iter()
            .map(|msg| format!("{:?}: {}", msg.role, msg.content))
            .collect::<Vec<_>>()
            .join("\n");
        
        let prompt = format!("Summarize this conversation:\n{}", conversation_text);
        
        // This would be implemented as an async call in practice
        // Simplified for documentation
        Ok("Conversation summary".to_string())
    }
}
```

## 4. EntityNode

Structured data management with calculated fields and natural language operations. (See [Entity Management](entity-management.md) for detailed documentation.)

### Key Features
- **Dynamic Schema**: User-defined field types and structures
- **Calculated Fields**: Excel-like formulas with dependency tracking
- **Natural Language CRUD**: Create and modify entities through conversation
- **Validation Rules**: Business rules expressed in natural language
- **Relationship Management**: Links between entities with referential integrity

## 5. QueryNode

Live data queries with real-time updates and AI-enhanced result processing. (See [Real-Time Updates](real-time-updates.md) for detailed documentation.)

### Key Features
- **Live Results**: Automatic updates when underlying data changes
- **AI Enhancement**: Natural language query construction and result interpretation
- **Performance Optimization**: Intelligent caching and query optimization
- **Subscription Management**: Event-driven update notifications
- **Complex Aggregations**: Multi-table joins and analytics

## Node Type Extensibility

### Plugin Node Types

The plugin system allows for custom node types while maintaining consistency with the core architecture:

```rust
pub trait PluginNode: Node {
    fn plugin_name(&self) -> &str;
    fn plugin_version(&self) -> &str;
    
    // Custom functionality
    fn custom_operations(&self) -> Vec<CustomOperation>;
    fn execute_custom_operation(
        &mut self,
        operation: &CustomOperation,
        context: &OperationContext
    ) -> Result<OperationResult, PluginError>;
    
    // AI integration hooks
    fn custom_ai_prompts(&self) -> HashMap<String, String>;
    fn process_ai_response(
        &mut self,
        operation_type: &str,
        ai_response: &str
    ) -> Result<(), PluginError>;
}

#[derive(Debug, Clone)]
pub struct CustomOperation {
    pub name: String,
    pub description: String,
    pub parameters: HashMap<String, ParameterDefinition>,
    pub ai_enabled: bool,
}
```

### Example Plugin Node: PresentationNode

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationNode {
    pub base: TextNode,
    pub slides: Vec<Slide>,
    pub theme: PresentationTheme,
    pub duration_minutes: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub title: String,
    pub content: String,
    pub slide_type: SlideType,
    pub notes: String,
}

impl PluginNode for PresentationNode {
    fn plugin_name(&self) -> &str { "presentation" }
    fn plugin_version(&self) -> &str { "1.0.0" }
    
    fn custom_operations(&self) -> Vec<CustomOperation> {
        vec![
            CustomOperation {
                name: "generate_slides_from_outline".to_string(),
                description: "Generate slides from outline using AI".to_string(),
                parameters: HashMap::new(),
                ai_enabled: true,
            },
            CustomOperation {
                name: "optimize_slide_timing".to_string(),
                description: "Optimize slide timing for target duration".to_string(),
                parameters: HashMap::new(),
                ai_enabled: true,
            },
        ]
    }
    
    fn execute_custom_operation(
        &mut self,
        operation: &CustomOperation,
        context: &OperationContext
    ) -> Result<OperationResult, PluginError> {
        match operation.name.as_str() {
            "generate_slides_from_outline" => {
                self.ai_generate_slides_from_outline(context.ai_engine).await
            }
            "optimize_slide_timing" => {
                self.ai_optimize_timing(context.ai_engine).await
            }
            _ => Err(PluginError::UnknownOperation(operation.name.clone()))
        }
    }
}
```

## Node Relationships and Hierarchy

### Parent-Child Relationships

```rust
pub struct NodeHierarchy {
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub depth: u32,
    pub path: Vec<String>,           // Path from root to this node
}

impl NodeHierarchy {
    pub fn get_ancestors(&self, storage: &dyn NodeStorage) -> Result<Vec<Box<dyn Node>>, StorageError> {
        let mut ancestors = Vec::new();
        for ancestor_id in &self.path {
            let node = storage.load_node(ancestor_id)?;
            ancestors.push(node);
        }
        Ok(ancestors)
    }
    
    pub fn get_descendants(&self, storage: &dyn NodeStorage) -> Result<Vec<Box<dyn Node>>, StorageError> {
        let mut descendants = Vec::new();
        let mut queue = self.children.clone();
        
        while let Some(child_id) = queue.pop() {
            let child = storage.load_node(&child_id)?;
            queue.extend_from_slice(child.children());
            descendants.push(child);
        }
        
        Ok(descendants)
    }
}
```

### Cross-Node References

```rust
pub struct NodeReference {
    pub source_node_id: String,
    pub target_node_id: String,
    pub reference_type: ReferenceType,
    pub metadata: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReferenceType {
    Related,            // General relationship
    Dependency,         // Source depends on target
    Reference,          // Source references target
    Embed,             // Source embeds target content
    Link,              // Hyperlink-style connection
}
```

## Performance Considerations

### Node Loading Strategies

```rust
pub enum LoadingStrategy {
    Eager,              // Load all data immediately
    Lazy,               // Load data on access
    Partial,            // Load metadata only, content on demand
    Cached,             // Use cached version if available
}

pub struct NodeLoader {
    storage: Arc<dyn NodeStorage>,
    cache: Arc<dyn NodeCache>,
    loading_strategy: LoadingStrategy,
}

impl NodeLoader {
    pub async fn load_node_with_strategy(
        &self,
        node_id: &str,
        strategy: LoadingStrategy
    ) -> Result<Box<dyn Node>, LoadError> {
        match strategy {
            LoadingStrategy::Cached => {
                if let Some(cached_node) = self.cache.get(node_id).await? {
                    Ok(cached_node)
                } else {
                    let node = self.storage.load_node(node_id).await?;
                    self.cache.store(node_id, &node).await?;
                    Ok(node)
                }
            }
            LoadingStrategy::Partial => {
                let metadata = self.storage.load_node_metadata(node_id).await?;
                Ok(self.create_partial_node(metadata))
            }
            _ => self.storage.load_node(node_id).await
        }
    }
}
```

### Memory Management

```rust
pub struct NodeMemoryManager {
    max_nodes_in_memory: usize,
    eviction_policy: EvictionPolicy,
    loaded_nodes: LruCache<String, Box<dyn Node>>,
}

#[derive(Debug, Clone)]
pub enum EvictionPolicy {
    LRU,                // Least Recently Used
    LFU,                // Least Frequently Used
    TimeToLive(Duration), // Time-based expiration
    Size(usize),        // Size-based limits
}
```

---

This node type architecture provides a flexible, extensible foundation for knowledge management while maintaining consistency and performance across all node types. The AI integration ensures that every node type can benefit from intelligent assistance while preserving clear architectural boundaries.