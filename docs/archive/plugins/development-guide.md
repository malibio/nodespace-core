# NodeType Extension Development Guide

## Overview

NodeSpace uses "NodeType Extensions" (internally called plugins) to enable parallel development of specialized node types while maintaining architectural consistency. This system allows team members to develop custom node types that integrate seamlessly with the core platform.

**Important Note**: These are not external plugins for third-party developers, but internal extensions for our development team to work in parallel on different node types and features.

## Development Philosophy

### Internal Extension System
- **Team Collaboration**: Enable multiple engineers to work on different node types simultaneously
- **Build-Time Integration**: Extensions are compiled into the main application for optimal performance
- **Shared Infrastructure**: All extensions use the same AI engine, storage, and validation systems
- **Consistent Experience**: Extensions follow the same UI/UX patterns as core node types

### Design Principles
- **Service Injection**: Extensions receive all needed services through dependency injection
- **Real Testing**: Test with actual databases, AI models, and services (no mocks)
- **Type Safety**: Rust's type system prevents runtime errors
- **AI Integration**: Every extension can leverage AI capabilities out of the box

## Extension Architecture

### Core Extension Trait

```rust
pub trait NodeTypeExtension: Send + Sync {
    // Extension metadata
    fn extension_name(&self) -> &str;
    fn extension_version(&self) -> &str;
    fn supported_node_types(&self) -> Vec<NodeType>;
    
    // Lifecycle management
    fn initialize(&mut self, services: &ExtensionServices) -> Result<(), ExtensionError>;
    fn shutdown(&mut self) -> Result<(), ExtensionError>;
    
    // Core functionality
    fn create_node(&self, request: CreateNodeRequest) -> Result<Box<dyn Node>, ExtensionError>;
    fn update_node(&self, node: &mut dyn Node, update: NodeUpdate) -> Result<(), ExtensionError>;
    fn delete_node(&self, node_id: &str) -> Result<(), ExtensionError>;
    
    // AI integration
    fn ai_operations(&self) -> Vec<AIOperation>;
    fn execute_ai_operation(
        &self,
        operation: &AIOperation,
        node: &mut dyn Node,
        context: &AIContext
    ) -> Result<AIResponse, ExtensionError>;
    
    // UI integration
    fn ui_components(&self) -> Vec<UIComponent>;
    fn handle_ui_event(
        &self,
        event: UIEvent,
        node: &mut dyn Node
    ) -> Result<UIResponse, ExtensionError>;
    
    // Validation and business rules
    fn validation_rules(&self) -> Vec<ValidationRule>;
    fn validate_node(&self, node: &dyn Node) -> Result<ValidationResult, ExtensionError>;
}
```

### Service Injection System

```rust
pub struct ExtensionServices {
    pub storage: Arc<dyn NodeStorage>,
    pub ai_engine: Arc<dyn AIEngine>,
    pub search_engine: Arc<dyn SearchEngine>,
    pub validation_engine: Arc<dyn ValidationEngine>,
    pub event_bus: Arc<dyn EventBus>,
    pub cache_manager: Arc<dyn CacheManager>,
    pub config: Arc<dyn ConfigManager>,
}

impl ExtensionServices {
    pub fn new(core_services: &CoreServices) -> Self {
        ExtensionServices {
            storage: core_services.storage.clone(),
            ai_engine: core_services.ai_engine.clone(),
            search_engine: core_services.search_engine.clone(),
            validation_engine: core_services.validation_engine.clone(),
            event_bus: core_services.event_bus.clone(),
            cache_manager: core_services.cache_manager.clone(),
            config: core_services.config.clone(),
        }
    }
    
    // Helper methods for common operations
    pub async fn store_node(&self, node: &dyn Node) -> Result<(), StorageError> {
        self.storage.save_node(node).await
    }
    
    pub async fn generate_embeddings(&self, content: &str) -> Result<Vec<f32>, AIError> {
        self.ai_engine.generate_embeddings(content).await
    }
    
    pub async fn publish_event(&self, event: Event) -> Result<(), EventError> {
        self.event_bus.publish(event).await
    }
}
```

## Creating a New Extension

### Step 1: Extension Structure

Create a new Rust module in the `crates/extensions/` directory:

```rust
// crates/extensions/presentation/src/lib.rs

use nodespace_core::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationNode {
    // Base node properties
    pub id: String,
    pub title: String,
    pub parent_id: Option<String>,
    pub children: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub metadata: HashMap<String, String>,
    
    // Presentation-specific properties
    pub slides: Vec<Slide>,
    pub theme: PresentationTheme,
    pub duration_minutes: Option<u32>,
    pub presenter_notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Slide {
    pub id: String,
    pub title: String,
    pub content: String,
    pub slide_type: SlideType,
    pub speaker_notes: String,
    pub duration_seconds: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SlideType {
    Title,
    Content,
    TwoColumn,
    Image,
    Code,
    Chart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PresentationTheme {
    pub name: String,
    pub primary_color: String,
    pub secondary_color: String,
    pub font_family: String,
}
```

### Step 2: Implement Core Node Trait

```rust
impl Node for PresentationNode {
    fn id(&self) -> &str { &self.id }
    fn node_type(&self) -> NodeType { NodeType::Extension("presentation".to_string()) }
    fn parent_id(&self) -> Option<&str> { self.parent_id.as_deref() }
    fn children(&self) -> &[String] { &self.children }
    fn created_at(&self) -> DateTime<Utc> { self.created_at }
    fn modified_at(&self) -> DateTime<Utc> { self.modified_at }
    fn metadata(&self) -> &HashMap<String, String> { &self.metadata }
    
    fn generate_embeddings(&self, ai_engine: &dyn AIEngine) -> Result<Vec<f32>, AIError> {
        // Generate embeddings for presentation content
        let content = self.slides.iter()
            .map(|slide| format!("{}: {}", slide.title, slide.content))
            .collect::<Vec<_>>()
            .join("\n");
        
        ai_engine.generate_embeddings(&content)
    }
    
    fn to_json(&self) -> Result<serde_json::Value, SerializationError> {
        serde_json::to_value(self).map_err(SerializationError::from)
    }
    
    fn from_json(value: serde_json::Value) -> Result<Self, SerializationError> {
        serde_json::from_value(value).map_err(SerializationError::from)
    }
}
```

### Step 3: Implement Extension Trait

```rust
pub struct PresentationExtension {
    services: Option<ExtensionServices>,
}

impl PresentationExtension {
    pub fn new() -> Self {
        PresentationExtension { services: None }
    }
}

impl NodeTypeExtension for PresentationExtension {
    fn extension_name(&self) -> &str { "presentation" }
    fn extension_version(&self) -> &str { "1.0.0" }
    fn supported_node_types(&self) -> Vec<NodeType> {
        vec![NodeType::Extension("presentation".to_string())]
    }
    
    fn initialize(&mut self, services: &ExtensionServices) -> Result<(), ExtensionError> {
        self.services = Some(services.clone());
        
        // Register custom validation rules
        let validation_rules = vec![
            ValidationRule::new(
                "presentation_has_slides",
                "Presentations must have at least one slide",
                |node: &dyn Node| {
                    if let Some(pres) = node.as_any().downcast_ref::<PresentationNode>() {
                        !pres.slides.is_empty()
                    } else {
                        true
                    }
                }
            ),
        ];
        
        for rule in validation_rules {
            services.validation_engine.register_rule(rule)?;
        }
        
        Ok(())
    }
    
    fn shutdown(&mut self) -> Result<(), ExtensionError> {
        self.services = None;
        Ok(())
    }
    
    fn create_node(&self, request: CreateNodeRequest) -> Result<Box<dyn Node>, ExtensionError> {
        let services = self.services.as_ref().ok_or(ExtensionError::NotInitialized)?;
        
        let presentation = PresentationNode {
            id: uuid::Uuid::new_v4().to_string(),
            title: request.title.unwrap_or_else(|| "New Presentation".to_string()),
            parent_id: request.parent_id,
            children: Vec::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
            metadata: request.metadata.unwrap_or_default(),
            slides: vec![
                Slide {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: "Title Slide".to_string(),
                    content: "Welcome to the presentation".to_string(),
                    slide_type: SlideType::Title,
                    speaker_notes: String::new(),
                    duration_seconds: Some(30),
                }
            ],
            theme: PresentationTheme {
                name: "Default".to_string(),
                primary_color: "#007acc".to_string(),
                secondary_color: "#ffffff".to_string(),
                font_family: "Arial, sans-serif".to_string(),
            },
            duration_minutes: None,
            presenter_notes: String::new(),
        };
        
        Ok(Box::new(presentation))
    }
    
    // Implementation continues...
}
```

### Step 4: AI Operations

```rust
impl PresentationExtension {
    fn ai_operations(&self) -> Vec<AIOperation> {
        vec![
            AIOperation {
                name: "generate_outline".to_string(),
                description: "Generate presentation outline from topic".to_string(),
                parameters: vec![
                    AIParameter::new("topic", "Topic for the presentation", AIParameterType::String),
                    AIParameter::new("duration", "Duration in minutes", AIParameterType::Integer),
                    AIParameter::new("audience", "Target audience", AIParameterType::String),
                ],
            },
            AIOperation {
                name: "create_slides_from_outline".to_string(),
                description: "Generate slides from outline".to_string(),
                parameters: vec![
                    AIParameter::new("outline", "Presentation outline", AIParameterType::String),
                ],
            },
            AIOperation {
                name: "improve_slide_content".to_string(),
                description: "Improve content of specific slide".to_string(),
                parameters: vec![
                    AIParameter::new("slide_id", "ID of slide to improve", AIParameterType::String),
                ],
            },
        ]
    }
    
    fn execute_ai_operation(
        &self,
        operation: &AIOperation,
        node: &mut dyn Node,
        context: &AIContext
    ) -> Result<AIResponse, ExtensionError> {
        let services = self.services.as_ref().ok_or(ExtensionError::NotInitialized)?;
        let presentation = node.as_any_mut()
            .downcast_mut::<PresentationNode>()
            .ok_or(ExtensionError::InvalidNodeType)?;
        
        match operation.name.as_str() {
            "generate_outline" => {
                self.ai_generate_outline(presentation, &context.parameters, services)
            }
            "create_slides_from_outline" => {
                self.ai_create_slides_from_outline(presentation, &context.parameters, services)
            }
            "improve_slide_content" => {
                self.ai_improve_slide_content(presentation, &context.parameters, services)
            }
            _ => Err(ExtensionError::UnknownOperation(operation.name.clone()))
        }
    }
    
    async fn ai_generate_outline(
        &self,
        presentation: &mut PresentationNode,
        parameters: &HashMap<String, String>,
        services: &ExtensionServices
    ) -> Result<AIResponse, ExtensionError> {
        let topic = parameters.get("topic").ok_or(ExtensionError::MissingParameter("topic"))?;
        let duration = parameters.get("duration")
            .and_then(|d| d.parse::<u32>().ok())
            .unwrap_or(30);
        let audience = parameters.get("audience").unwrap_or("general");
        
        let prompt = format!(
            r#"Create a presentation outline for the topic: "{}"
            
            Duration: {} minutes
            Audience: {}
            
            Return a JSON object with:
            {{
                "title": "presentation title",
                "slides": [
                    {{
                        "title": "slide title",
                        "content": "slide content summary",
                        "type": "Title|Content|TwoColumn|Image|Code|Chart",
                        "duration_seconds": estimated_duration
                    }}
                ]
            }}
            
            Guidelines:
            - Include title slide and conclusion
            - Each content slide should have 2-3 key points
            - Balance slide count with available time
            - Consider audience knowledge level"#,
            topic, duration, audience
        );
        
        let ai_response = services.ai_engine.generate_text(&prompt, "").await?;
        let outline_data: serde_json::Value = serde_json::from_str(&ai_response)?;
        
        // Update presentation with AI-generated outline
        presentation.title = outline_data["title"].as_str()
            .unwrap_or(topic)
            .to_string();
        
        presentation.duration_minutes = Some(duration);
        
        if let Some(slides_array) = outline_data["slides"].as_array() {
            presentation.slides = slides_array.iter().enumerate().map(|(i, slide_data)| {
                Slide {
                    id: format!("{}-slide-{}", presentation.id, i),
                    title: slide_data["title"].as_str().unwrap_or("Slide").to_string(),
                    content: slide_data["content"].as_str().unwrap_or("").to_string(),
                    slide_type: match slide_data["type"].as_str().unwrap_or("Content") {
                        "Title" => SlideType::Title,
                        "TwoColumn" => SlideType::TwoColumn,
                        "Image" => SlideType::Image,
                        "Code" => SlideType::Code,
                        "Chart" => SlideType::Chart,
                        _ => SlideType::Content,
                    },
                    speaker_notes: String::new(),
                    duration_seconds: slide_data["duration_seconds"].as_u64().map(|d| d as u32),
                }
            }).collect();
        }
        
        presentation.modified_at = Utc::now();
        
        Ok(AIResponse {
            success: true,
            message: format!("Generated outline with {} slides", presentation.slides.len()),
            data: Some(ai_response),
        })
    }
}
```

## Testing Extensions

### Integration Testing Philosophy

Following NodeSpace's "real services" testing approach, test extensions with actual services:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_test_utils::*;
    
    #[tokio::test]
    async fn test_presentation_creation_and_ai_enhancement() {
        // Create real test services (LanceDB, AI model)
        let test_services = create_test_services().await;
        let extension_services = ExtensionServices::new(&test_services);
        
        // Initialize extension
        let mut presentation_ext = PresentationExtension::new();
        presentation_ext.initialize(&extension_services).unwrap();
        
        // Test node creation
        let create_request = CreateNodeRequest {
            title: Some("Test Presentation".to_string()),
            parent_id: None,
            metadata: None,
        };
        
        let mut node = presentation_ext.create_node(create_request).unwrap();
        
        // Test AI operations
        let ai_context = AIContext {
            parameters: hashmap! {
                "topic".to_string() => "Machine Learning Basics".to_string(),
                "duration".to_string() => "45".to_string(),
                "audience".to_string() => "software engineers".to_string(),
            },
        };
        
        let outline_operation = AIOperation {
            name: "generate_outline".to_string(),
            description: "Generate presentation outline".to_string(),
            parameters: vec![],
        };
        
        let ai_response = presentation_ext.execute_ai_operation(
            &outline_operation,
            node.as_mut(),
            &ai_context
        ).unwrap();
        
        assert!(ai_response.success);
        
        // Verify presentation was updated
        let presentation = node.as_any().downcast_ref::<PresentationNode>().unwrap();
        assert!(!presentation.slides.is_empty());
        assert_eq!(presentation.duration_minutes, Some(45));
        assert!(presentation.title.contains("Machine Learning"));
        
        // Test storage and retrieval
        extension_services.store_node(node.as_ref()).await.unwrap();
        
        let loaded_node = extension_services.storage
            .load_node(&presentation.id)
            .await
            .unwrap();
        
        assert_eq!(loaded_node.id(), presentation.id);
    }
    
    #[tokio::test]
    async fn test_presentation_validation() {
        let test_services = create_test_services().await;
        let extension_services = ExtensionServices::new(&test_services);
        
        let mut presentation_ext = PresentationExtension::new();
        presentation_ext.initialize(&extension_services).unwrap();
        
        // Create presentation without slides (should fail validation)
        let mut empty_presentation = PresentationNode {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Empty Presentation".to_string(),
            parent_id: None,
            children: Vec::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
            metadata: HashMap::new(),
            slides: Vec::new(),  // Empty slides should fail validation
            theme: PresentationTheme {
                name: "Default".to_string(),
                primary_color: "#007acc".to_string(),
                secondary_color: "#ffffff".to_string(),
                font_family: "Arial".to_string(),
            },
            duration_minutes: None,
            presenter_notes: String::new(),
        };
        
        let validation_result = presentation_ext.validate_node(&empty_presentation).unwrap();
        assert!(!validation_result.is_valid);
        assert!(validation_result.errors.iter()
            .any(|e| e.contains("must have at least one slide")));
    }
    
    #[tokio::test]
    async fn test_presentation_search_and_embeddings() {
        let test_services = create_test_services().await;
        let extension_services = ExtensionServices::new(&test_services);
        
        // Create presentation with content
        let presentation = PresentationNode {
            id: uuid::Uuid::new_v4().to_string(),
            title: "Machine Learning Overview".to_string(),
            parent_id: None,
            children: Vec::new(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
            metadata: HashMap::new(),
            slides: vec![
                Slide {
                    id: uuid::Uuid::new_v4().to_string(),
                    title: "Introduction to Neural Networks".to_string(),
                    content: "Neural networks are computational models inspired by biological neural networks".to_string(),
                    slide_type: SlideType::Content,
                    speaker_notes: String::new(),
                    duration_seconds: Some(120),
                }
            ],
            theme: PresentationTheme::default(),
            duration_minutes: Some(30),
            presenter_notes: String::new(),
        };
        
        // Generate embeddings
        let embeddings = presentation.generate_embeddings(
            extension_services.ai_engine.as_ref()
        ).unwrap();
        
        assert!(!embeddings.is_empty());
        assert_eq!(embeddings.len(), 768); // Typical embedding dimension
        
        // Store in vector database
        extension_services.search_engine.store_embeddings(
            &presentation.id,
            &format!("{}: {}", presentation.title, presentation.slides[0].content),
            embeddings
        ).await.unwrap();
        
        // Test semantic search
        let search_results = extension_services.search_engine.search(
            "deep learning artificial intelligence",
            5
        ).await.unwrap();
        
        assert!(!search_results.is_empty());
        assert!(search_results.iter().any(|r| r.node_id == presentation.id));
    }
}
```

## Extension Registration

### Build-Time Registration

Extensions are registered at compile time through the build system:

```rust
// crates/core/src/extensions/registry.rs

use crate::extensions::*;

pub struct ExtensionRegistry {
    extensions: Vec<Box<dyn NodeTypeExtension>>,
}

impl ExtensionRegistry {
    pub fn new() -> Self {
        let mut registry = ExtensionRegistry {
            extensions: Vec::new(),
        };
        
        // Register all built-in extensions
        registry.register_extension(Box::new(PresentationExtension::new()));
        registry.register_extension(Box::new(SpreadsheetExtension::new()));
        registry.register_extension(Box::new(KanbanExtension::new()));
        registry.register_extension(Box::new(CalendarExtension::new()));
        
        registry
    }
    
    pub fn register_extension(&mut self, extension: Box<dyn NodeTypeExtension>) {
        self.extensions.push(extension);
    }
    
    pub fn initialize_all(&mut self, services: &ExtensionServices) -> Result<(), ExtensionError> {
        for extension in &mut self.extensions {
            extension.initialize(services)?;
        }
        Ok(())
    }
    
    pub fn get_extension(&self, name: &str) -> Option<&dyn NodeTypeExtension> {
        self.extensions.iter()
            .find(|ext| ext.extension_name() == name)
            .map(|ext| ext.as_ref())
    }
    
    pub fn supported_node_types(&self) -> Vec<NodeType> {
        self.extensions.iter()
            .flat_map(|ext| ext.supported_node_types())
            .collect()
    }
}
```

### Cargo Workspace Configuration

```toml
# Cargo.toml (workspace root)
[workspace]
members = [
    "crates/core",
    "crates/storage",
    "crates/ai",
    "crates/search",
    "crates/validation",
    "crates/extensions/presentation",
    "crates/extensions/spreadsheet",
    "crates/extensions/kanban",
    "crates/extensions/calendar",
]

# crates/extensions/presentation/Cargo.toml
[package]
name = "nodespace-presentation-extension"
version = "1.0.0"
edition = "2021"

[dependencies]
nodespace-core = { path = "../../core" }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"
uuid = { version = "1.0", features = ["v4"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1.0"
tokio = { version = "1.0", features = ["full"] }

[dev-dependencies]
nodespace-test-utils = { path = "../../test-utils" }
```

## UI Integration

### Svelte Component Generation

Extensions can define UI components that are automatically generated:

```rust
impl PresentationExtension {
    fn ui_components(&self) -> Vec<UIComponent> {
        vec![
            UIComponent {
                name: "PresentationEditor".to_string(),
                component_type: UIComponentType::NodeEditor,
                props: vec![
                    UIProp::new("slides", UIDataType::Array),
                    UIProp::new("theme", UIDataType::Object),
                    UIProp::new("duration", UIDataType::Number),
                ],
                events: vec![
                    UIEvent::new("slide-added", vec!["slide"]),
                    UIEvent::new("slide-deleted", vec!["slideId"]),
                    UIEvent::new("theme-changed", vec!["theme"]),
                ],
            },
            UIComponent {
                name: "SlideEditor".to_string(),
                component_type: UIComponentType::SubEditor,
                props: vec![
                    UIProp::new("slide", UIDataType::Object),
                    UIProp::new("editable", UIDataType::Boolean),
                ],
                events: vec![
                    UIEvent::new("content-changed", vec!["content"]),
                    UIEvent::new("speaker-notes-changed", vec!["notes"]),
                ],
            },
        ]
    }
}
```

This generates corresponding Svelte components:

```svelte
<!-- src/lib/components/extensions/PresentationEditor.svelte -->
<script>
  import { createEventDispatcher } from 'svelte';
  import SlideEditor from './SlideEditor.svelte';
  
  export let slides = [];
  export let theme = {};
  export let duration = 30;
  
  const dispatch = createEventDispatcher();
  
  function addSlide() {
    const newSlide = {
      id: crypto.randomUUID(),
      title: 'New Slide',
      content: '',
      type: 'Content',
      speakerNotes: '',
      duration: 60
    };
    
    slides = [...slides, newSlide];
    dispatch('slide-added', { slide: newSlide });
  }
  
  function deleteSlide(slideId) {
    slides = slides.filter(s => s.id !== slideId);
    dispatch('slide-deleted', { slideId });
  }
</script>

<div class="presentation-editor">
  <header class="presentation-header">
    <h2>Presentation Editor</h2>
    <div class="duration-control">
      <label>Duration: 
        <input type="number" bind:value={duration} min="1" max="480" />
        minutes
      </label>
    </div>
  </header>
  
  <div class="slides-container">
    {#each slides as slide, index}
      <div class="slide-wrapper">
        <div class="slide-number">{index + 1}</div>
        <SlideEditor 
          {slide} 
          editable={true}
          on:content-changed={(e) => slide.content = e.detail.content}
          on:speaker-notes-changed={(e) => slide.speakerNotes = e.detail.notes}
        />
        <button class="delete-slide" on:click={() => deleteSlide(slide.id)}>
          Delete
        </button>
      </div>
    {/each}
  </div>
  
  <button class="add-slide" on:click={addSlide}>
    Add Slide
  </button>
</div>

<style>
  .presentation-editor {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    padding: 1rem;
  }
  
  .presentation-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    border-bottom: 1px solid var(--border-color);
    padding-bottom: 1rem;
  }
  
  .slides-container {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  
  .slide-wrapper {
    display: flex;
    align-items: flex-start;
    gap: 1rem;
    padding: 1rem;
    border: 1px solid var(--border-color);
    border-radius: 0.5rem;
  }
  
  .slide-number {
    font-weight: bold;
    min-width: 2rem;
    text-align: center;
  }
  
  .add-slide {
    align-self: flex-start;
    padding: 0.5rem 1rem;
    background: var(--primary-color);
    color: white;
    border: none;
    border-radius: 0.25rem;
    cursor: pointer;
  }
</style>
```

## Development Best Practices

### Code Organization

```
crates/extensions/my-extension/
├── src/
│   ├── lib.rs              # Extension entry point
│   ├── nodes/              # Node type definitions
│   │   ├── mod.rs
│   │   └── my_node.rs
│   ├── operations/         # AI and custom operations
│   │   ├── mod.rs
│   │   ├── ai_operations.rs
│   │   └── validation.rs
│   ├── ui/                 # UI component definitions
│   │   ├── mod.rs
│   │   └── components.rs
│   └── tests/              # Integration tests
│       ├── mod.rs
│       ├── node_tests.rs
│       ├── ai_tests.rs
│       └── integration_tests.rs
├── Cargo.toml
└── README.md
```

### Error Handling

```rust
#[derive(Debug, thiserror::Error)]
pub enum ExtensionError {
    #[error("Extension not initialized")]
    NotInitialized,
    
    #[error("Invalid node type for this extension")]
    InvalidNodeType,
    
    #[error("Unknown operation: {0}")]
    UnknownOperation(String),
    
    #[error("Missing required parameter: {0}")]
    MissingParameter(String),
    
    #[error("Storage error: {0}")]
    Storage(#[from] StorageError),
    
    #[error("AI error: {0}")]
    AI(#[from] AIError),
    
    #[error("Validation error: {0}")]
    Validation(#[from] ValidationError),
}
```

### Configuration Management

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtensionConfig {
    pub enabled: bool,
    pub settings: HashMap<String, serde_json::Value>,
    pub ui_overrides: HashMap<String, UIOverride>,
}

impl ExtensionConfig {
    pub fn get_setting<T>(&self, key: &str) -> Option<T> 
    where
        T: for<'de> Deserialize<'de>
    {
        self.settings.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }
    
    pub fn set_setting<T>(&mut self, key: &str, value: T) -> Result<(), serde_json::Error>
    where
        T: Serialize
    {
        self.settings.insert(key.to_string(), serde_json::to_value(value)?);
        Ok(())
    }
}
```

## Example: Complete Kanban Extension

See `docs/architecture/plugins/examples/kanban-extension.md` for a complete example of implementing a Kanban board extension with:
- Custom KanbanNode implementation
- AI-powered task prioritization
- Drag-and-drop UI components
- Sprint planning automation
- Burndown chart generation

---

This development guide provides the foundation for creating powerful, AI-integrated extensions that maintain consistency with NodeSpace's architecture while enabling rapid parallel development across the team.