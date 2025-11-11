# Plugin Architecture Specification

## Overview

NodeSpace's plugin architecture enables independent development of specialized node types (PDF, Image, Code, etc.) while maintaining full integration with the core system. Using build-time compilation rather than dynamic loading, plugins achieve optimal performance while allowing parallel development across teams.

## Core Design Principles

### 1. Build-Time Compilation Strategy

Unlike traditional runtime plugin systems, NodeSpace uses **build-time compilation** for maximum performance and type safety:

```toml
# Main application Cargo.toml
[dependencies]
nodespace-pdf-node = { path = "../nodespace-pdf-node" }
nodespace-image-node = { path = "../nodespace-image-node" }
nodespace-code-node = { path = "../nodespace-code-node" }
```

**Benefits:**
- **Zero Runtime Overhead**: No dynamic loading or reflection
- **Type Safety**: Full Rust type checking across plugin boundaries
- **Optimal Performance**: All code compiled together with full optimizations
- **Simplified Distribution**: Single binary with all plugins included

### 2. Service Injection Pattern

Plugins receive all necessary services through dependency injection, enabling real implementations and comprehensive testing:

```rust
pub trait NodePlugin {
    fn new(services: PluginServices) -> Self;
    fn node_type(&self) -> &'static str;
    fn get_component_name(&self) -> &'static str;
}

#[derive(Clone)]
pub struct PluginServices {
    pub storage: Arc<dyn NodeStorage>,
    pub nlp_engine: Arc<dyn NLPEngine>,
    pub search_engine: Arc<dyn SearchEngine>,
    pub text_processor: Arc<dyn TextProcessor>,
    pub validation_engine: Arc<dyn ValidationEngine>,
}
```

### 3. Mixed Language Development

Each plugin combines Rust backend logic with Svelte frontend components:

```
nodespace-pdf-node/
├── Cargo.toml                 # Rust dependencies
├── package.json               # Svelte dependencies
├── src/
│   ├── lib.rs                # Main plugin logic
│   ├── pdf_processor.rs      # PDF-specific processing
│   ├── commands.rs           # Tauri commands
│   └── ui/                   # Svelte components
│       ├── PDFNode.svelte    # Main node component
│       ├── PDFViewer.svelte  # PDF viewer widget
│       └── PDFSettings.svelte # Configuration UI
├── tests/
│   ├── integration_tests.rs  # Full service integration
│   └── test_data/
│       └── sample.pdf
└── dist/                     # Built assets
```

---

## Plugin Implementation Pattern

### Base Plugin Structure

```rust
// nodespace-pdf-node/src/lib.rs
use nodespace_core::{
    node_types::TextNode,
    traits::{NodePlugin, NodeBehavior, NodeStorage},
    services::PluginServices,
    types::{EmbeddingData, ProcessingResult},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDFNode {
    pub base: TextNode,
    pub file_path: String,
    pub file_size: u64,
    pub page_count: u32,
    pub extracted_text: String,
    pub thumbnail_path: Option<String>,
    pub processing_status: ProcessingStatus,
    pub metadata: PDFMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PDFMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub creation_date: Option<DateTime<Utc>>,
    pub modification_date: Option<DateTime<Utc>>,
    pub encrypted: bool,
    pub pdf_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProcessingStatus {
    Pending,
    Processing,
    Completed,
    Failed { error: String },
}

pub struct PDFPlugin {
    services: PluginServices,
}

impl NodePlugin for PDFPlugin {
    fn new(services: PluginServices) -> Self {
        PDFPlugin { services }
    }
    
    fn node_type(&self) -> &'static str {
        "pdf"
    }
    
    fn get_component_name(&self) -> &'static str {
        "PDFNode"
    }
}

impl NodeBehavior for PDFNode {
    fn node_type(&self) -> &'static str { "pdf" }
    fn can_have_children(&self) -> bool { true }
    fn supports_markdown(&self) -> bool { false }
}
```

### Core Plugin Operations

```rust
impl PDFPlugin {
    pub async fn create_from_file(
        &self,
        file_path: &str,
        parent_id: Option<String>
    ) -> Result<PDFNode, Error> {
        // 1. Read and validate PDF file
        let pdf_bytes = tokio::fs::read(file_path).await?;
        let file_size = pdf_bytes.len() as u64;
        
        // 2. Extract basic metadata
        let metadata = self.extract_pdf_metadata(&pdf_bytes)?;
        
        // 3. Create base node
        let node_id = uuid::Uuid::new_v4().to_string();
        let base_node = TextNode {
            id: node_id.clone(),
            content: format!("PDF: {}", metadata.title.unwrap_or("Untitled".to_string())),
            parent_id,
            children: Vec::new(),
            metadata: HashMap::new(),
            created_at: Utc::now(),
            modified_at: Ut::now(),
        };
        
        // 4. Store raw file
        let stored_path = self.services.storage
            .save_file(&node_id, &pdf_bytes, "pdf")
            .await?;
        
        // 5. Create PDF node
        let mut pdf_node = PDFNode {
            base: base_node,
            file_path: stored_path,
            file_size,
            page_count: 0, // Will be updated during processing
            extracted_text: String::new(),
            thumbnail_path: None,
            processing_status: ProcessingStatus::Pending,
            metadata,
        };
        
        // 6. Start background processing
        let processing_result = self.process_pdf_content(&mut pdf_node).await?;
        
        // 7. Save to storage
        self.services.storage.save_node(&pdf_node).await?;
        
        Ok(pdf_node)
    }
    
    async fn process_pdf_content(
        &self,
        pdf_node: &mut PDFNode
    ) -> Result<ProcessingResult, Error> {
        pdf_node.processing_status = ProcessingStatus::Processing;
        
        let pdf_bytes = tokio::fs::read(&pdf_node.file_path).await?;
        
        // 1. Extract text with page boundaries
        let text_extraction = self.extract_text_with_pages(&pdf_bytes)?;
        pdf_node.extracted_text = text_extraction.full_text.clone();
        pdf_node.page_count = text_extraction.pages.len() as u32;
        
        // 2. Generate thumbnail
        if let Ok(thumbnail_path) = self.generate_thumbnail(&pdf_bytes, &pdf_node.base.id).await {
            pdf_node.thumbnail_path = Some(thumbnail_path);
        }
        
        // 3. Create hierarchical embeddings
        let embeddings = self.generate_pdf_embeddings(&text_extraction).await?;
        
        // 4. Save embeddings to vector database
        self.services.storage
            .save_embeddings(&pdf_node.base.id, embeddings)
            .await?;
        
        // 5. Index for full-text search
        self.services.search_engine
            .index_document(&pdf_node.base.id, &text_extraction.full_text)
            .await?;
        
        pdf_node.processing_status = ProcessingStatus::Completed;
        
        Ok(ProcessingResult {
            embeddings_generated: true,
            search_indexed: true,
            thumbnail_created: pdf_node.thumbnail_path.is_some(),
        })
    }
    
    async fn generate_pdf_embeddings(
        &self,
        text_extraction: &TextExtraction
    ) -> Result<Vec<EmbeddingData>, Error> {
        let mut embeddings = Vec::new();
        
        // 1. Document-level embedding
        let doc_embedding = self.services.nlp_engine
            .embed_text(&text_extraction.full_text)
            .await?;
        
        embeddings.push(EmbeddingData {
            id: format!("{}_document", text_extraction.document_id),
            embedding: doc_embedding,
            metadata: EmbeddingMetadata {
                level: EmbeddingLevel::Document,
                content_type: "pdf_document".to_string(),
                page_number: None,
                chunk_index: None,
            },
        });
        
        // 2. Page-level embeddings
        for (page_num, page_text) in text_extraction.pages.iter().enumerate() {
            if !page_text.trim().is_empty() {
                let page_embedding = self.services.nlp_engine
                    .embed_text(page_text)
                    .await?;
                
                embeddings.push(EmbeddingData {
                    id: format!("{}_page_{}", text_extraction.document_id, page_num + 1),
                    embedding: page_embedding,
                    metadata: EmbeddingMetadata {
                        level: EmbeddingLevel::Page,
                        content_type: "pdf_page".to_string(),
                        page_number: Some(page_num + 1),
                        chunk_index: None,
                    },
                });
            }
        }
        
        // 3. Chunk-level embeddings for semantic search
        for (page_num, page_text) in text_extraction.pages.iter().enumerate() {
            let chunks = self.chunk_text_semantically(page_text, 500)?;
            
            for (chunk_idx, chunk) in chunks.iter().enumerate() {
                let chunk_embedding = self.services.nlp_engine
                    .embed_text(chunk)
                    .await?;
                
                embeddings.push(EmbeddingData {
                    id: format!("{}_page_{}_chunk_{}", 
                               text_extraction.document_id, page_num + 1, chunk_idx),
                    embedding: chunk_embedding,
                    metadata: EmbeddingMetadata {
                        level: EmbeddingLevel::Chunk,
                        content_type: "pdf_chunk".to_string(),
                        page_number: Some(page_num + 1),
                        chunk_index: Some(chunk_idx),
                    },
                });
            }
        }
        
        Ok(embeddings)
    }
    
    fn extract_text_with_pages(&self, pdf_bytes: &[u8]) -> Result<TextExtraction, Error> {
        use pdf_extract::extract_text_from_mem;
        
        // Use pdf-extract crate for text extraction
        let full_text = extract_text_from_mem(pdf_bytes)
            .map_err(|e| Error::PDFProcessing(format!("Text extraction failed: {}", e)))?;
        
        // Split into pages (simplified - real implementation would use proper PDF parsing)
        let pages: Vec<String> = full_text
            .split("\n\n--- Page Break ---\n\n")
            .map(|s| s.to_string())
            .collect();
        
        Ok(TextExtraction {
            document_id: uuid::Uuid::new_v4().to_string(),
            full_text,
            pages,
        })
    }
    
    async fn generate_thumbnail(&self, pdf_bytes: &[u8], node_id: &str) -> Result<String, Error> {
        use pdf_render::render_page_to_image;
        
        // Render first page as thumbnail
        let thumbnail_bytes = render_page_to_image(pdf_bytes, 0, 200, 300)?;
        
        // Save thumbnail
        let thumbnail_path = self.services.storage
            .save_file(node_id, &thumbnail_bytes, "png")
            .await?;
        
        Ok(thumbnail_path)
    }
}

#[derive(Debug, Clone)]
pub struct TextExtraction {
    pub document_id: String,
    pub full_text: String,
    pub pages: Vec<String>,
}

#[derive(Debug, Clone)]
pub enum EmbeddingLevel {
    Document,
    Page,
    Chunk,
}
```

### Plugin Validation Integration

```rust
impl PDFPlugin {
    pub async fn validate_pdf_node(
        &self,
        pdf_node: &PDFNode
    ) -> Result<ValidationResult, Error> {
        let mut violations = Vec::new();
        
        // 1. File existence validation
        if !tokio::fs::metadata(&pdf_node.file_path).await.is_ok() {
            violations.push(ValidationViolation {
                field_name: "file_path".to_string(),
                message: "PDF file not found on disk".to_string(),
                severity: ValidationSeverity::Error,
            });
        }
        
        // 2. File size validation
        if pdf_node.file_size > 100_000_000 { // 100MB limit
            violations.push(ValidationViolation {
                field_name: "file_size".to_string(),
                message: "PDF file exceeds 100MB size limit".to_string(),
                severity: ValidationSeverity::Warning,
            });
        }
        
        // 3. Processing status validation
        if matches!(pdf_node.processing_status, ProcessingStatus::Failed { .. }) {
            violations.push(ValidationViolation {
                field_name: "processing_status".to_string(),
                message: "PDF processing failed - content may not be searchable".to_string(),
                severity: ValidationSeverity::Warning,
            });
        }
        
        // 4. Content validation
        if pdf_node.extracted_text.trim().is_empty() && 
           matches!(pdf_node.processing_status, ProcessingStatus::Completed) {
            violations.push(ValidationViolation {
                field_name: "extracted_text".to_string(),
                message: "No text content extracted from PDF - may be image-only".to_string(),
                severity: ValidationSeverity::Info,
            });
        }
        
        Ok(ValidationResult {
            is_valid: violations.iter().all(|v| v.severity != ValidationSeverity::Error),
            violations,
            warnings: Vec::new(),
        })
    }
}
```

---

## Svelte Component Integration

### Main Node Component

```svelte
<!-- nodespace-pdf-node/src/ui/PDFNode.svelte -->
<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import PDFViewer from './PDFViewer.svelte';
  import PDFSettings from './PDFSettings.svelte';
  import type { PDFNode } from '../lib.rs';
  
  export let node: PDFNode;
  export let isSelected: boolean = false;
  export let isEditing: boolean = false;
  
  const dispatch = createEventDispatcher();
  
  let showViewer = false;
  let showSettings = false;
  let processingProgress = 0;
  
  $: isProcessing = node.processing_status === 'Processing';
  $: hasError = typeof node.processing_status === 'object' && 
                'Failed' in node.processing_status;
  
  async function openPDF() {
    showViewer = true;
    dispatch('node-action', { 
      type: 'pdf-opened',
      nodeId: node.base.id 
    });
  }
  
  async function reprocessPDF() {
    dispatch('node-action', {
      type: 'reprocess-pdf',
      nodeId: node.base.id
    });
  }
  
  function formatFileSize(bytes: number): string {
    const units = ['B', 'KB', 'MB', 'GB'];
    let size = bytes;
    let unitIndex = 0;
    
    while (size >= 1024 && unitIndex < units.length - 1) {
      size /= 1024;
      unitIndex++;
    }
    
    return `${size.toFixed(1)} ${units[unitIndex]}`;
  }
</script>

<div class="pdf-node" class:selected={isSelected} class:processing={isProcessing}>
  <div class="pdf-header">
    <div class="pdf-icon">
      📄
    </div>
    
    <div class="pdf-info">
      <h3 class="pdf-title">{node.metadata.title || 'PDF Document'}</h3>
      <div class="pdf-meta">
        {node.page_count} pages • {formatFileSize(node.file_size)}
        {#if node.metadata.author}
          • by {node.metadata.author}
        {/if}
      </div>
    </div>
    
    <div class="pdf-actions">
      {#if isProcessing}
        <div class="processing-indicator">
          <div class="spinner"></div>
          Processing...
        </div>
      {:else if hasError}
        <button class="retry-btn" on:click={reprocessPDF}>
          🔄 Retry
        </button>
      {:else}
        <button class="open-btn" on:click={openPDF}>
          👁️ View
        </button>
      {/if}
      
      <button class="settings-btn" on:click={() => showSettings = true}>
        ⚙️
      </button>
    </div>
  </div>
  
  {#if node.thumbnail_path}
    <div class="pdf-thumbnail" on:click={openPDF}>
      <img src={node.thumbnail_path} alt="PDF thumbnail" />
    </div>
  {/if}
  
  {#if node.extracted_text && !showViewer}
    <div class="pdf-preview">
      <p>{node.extracted_text.substring(0, 200)}...</p>
    </div>
  {/if}
  
  {#if hasError}
    <div class="error-message">
      ⚠️ Processing failed: {node.processing_status.Failed.error}
    </div>
  {/if}
</div>

{#if showViewer}
  <PDFViewer
    node={node}
    on:close={() => showViewer = false}
    on:search={(event) => dispatch('search-pdf', event.detail)}
  />
{/if}

{#if showSettings}
  <PDFSettings
    node={node}
    on:close={() => showSettings = false}
    on:update={(event) => dispatch('node-update', event.detail)}
  />
{/if}

<style>
  .pdf-node {
    border: 1px solid #e1e5e9;
    border-radius: 8px;
    padding: 16px;
    margin: 8px 0;
    background: white;
    transition: all 0.2s ease;
  }
  
  .pdf-node.selected {
    border-color: #007acc;
    box-shadow: 0 0 0 2px rgba(0, 122, 204, 0.2);
  }
  
  .pdf-node.processing {
    background: #f8f9fa;
  }
  
  .pdf-header {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 12px;
  }
  
  .pdf-icon {
    font-size: 24px;
    width: 40px;
    height: 40px;
    display: flex;
    align-items: center;
    justify-content: center;
    background: #f1f3f4;
    border-radius: 6px;
  }
  
  .pdf-info {
    flex: 1;
  }
  
  .pdf-title {
    margin: 0;
    font-size: 16px;
    font-weight: 600;
    color: #1a1a1a;
  }
  
  .pdf-meta {
    font-size: 14px;
    color: #666;
    margin-top: 4px;
  }
  
  .pdf-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  
  .processing-indicator {
    display: flex;
    align-items: center;
    gap: 8px;
    color: #666;
    font-size: 14px;
  }
  
  .spinner {
    width: 16px;
    height: 16px;
    border: 2px solid #e1e5e9;
    border-top: 2px solid #007acc;
    border-radius: 50%;
    animation: spin 1s linear infinite;
  }
  
  @keyframes spin {
    0% { transform: rotate(0deg); }
    100% { transform: rotate(360deg); }
  }
  
  .pdf-thumbnail {
    margin: 12px 0;
    cursor: pointer;
  }
  
  .pdf-thumbnail img {
    max-width: 200px;
    max-height: 300px;
    border: 1px solid #e1e5e9;
    border-radius: 4px;
  }
  
  .pdf-preview {
    background: #f8f9fa;
    padding: 12px;
    border-radius: 4px;
    font-size: 14px;
    line-height: 1.5;
    color: #666;
  }
  
  .error-message {
    background: #fff3cd;
    border: 1px solid #ffeaa7;
    color: #856404;
    padding: 8px 12px;
    border-radius: 4px;
    font-size: 14px;
    margin-top: 8px;
  }
  
  button {
    padding: 6px 12px;
    border: 1px solid #e1e5e9;
    border-radius: 4px;
    background: white;
    cursor: pointer;
    font-size: 14px;
    transition: all 0.2s ease;
  }
  
  button:hover {
    background: #f8f9fa;
  }
  
  .open-btn {
    border-color: #007acc;
    color: #007acc;
  }
  
  .open-btn:hover {
    background: #007acc;
    color: white;
  }
  
  .retry-btn {
    border-color: #dc3545;
    color: #dc3545;
  }
  
  .retry-btn:hover {
    background: #dc3545;
    color: white;
  }
</style>
```

### PDF Viewer Component

```svelte
<!-- nodespace-pdf-node/src/ui/PDFViewer.svelte -->
<script lang="ts">
  import { createEventDispatcher, onMount } from 'svelte';
  import type { PDFNode } from '../lib.rs';
  
  export let node: PDFNode;
  
  const dispatch = createEventDispatcher();
  
  let currentPage = 1;
  let searchQuery = '';
  let searchResults = [];
  let zoomLevel = 100;
  
  onMount(() => {
    // Initialize PDF.js or similar PDF rendering library
    loadPDFViewer();
  });
  
  async function loadPDFViewer() {
    // Implementation would use PDF.js or similar
    // This is a simplified example
  }
  
  function nextPage() {
    if (currentPage < node.page_count) {
      currentPage++;
    }
  }
  
  function prevPage() {
    if (currentPage > 1) {
      currentPage--;
    }
  }
  
  function searchInPDF() {
    dispatch('search', {
      query: searchQuery,
      nodeId: node.base.id
    });
  }
  
  function closeViewer() {
    dispatch('close');
  }
</script>

<div class="pdf-viewer-overlay">
  <div class="pdf-viewer">
    <div class="pdf-toolbar">
      <div class="toolbar-section">
        <button on:click={closeViewer}>✕ Close</button>
        <span class="pdf-title">{node.metadata.title || 'PDF Document'}</span>
      </div>
      
      <div class="toolbar-section">
        <input
          type="text"
          placeholder="Search in PDF..."
          bind:value={searchQuery}
          on:keydown={(e) => e.key === 'Enter' && searchInPDF()}
        />
        <button on:click={searchInPDF}>🔍</button>
      </div>
      
      <div class="toolbar-section">
        <button on:click={prevPage} disabled={currentPage <= 1}>‹</button>
        <span class="page-info">
          {currentPage} / {node.page_count}
        </span>
        <button on:click={nextPage} disabled={currentPage >= node.page_count}>›</button>
        
        <select bind:value={zoomLevel}>
          <option value={50}>50%</option>
          <option value={75}>75%</option>
          <option value={100}>100%</option>
          <option value={125}>125%</option>
          <option value={150}>150%</option>
        </select>
      </div>
    </div>
    
    <div class="pdf-content">
      <!-- PDF rendering would go here -->
      <div class="pdf-page" style="zoom: {zoomLevel}%">
        <iframe src={`/api/pdf/${node.base.id}/page/${currentPage}`} />
      </div>
    </div>
  </div>
</div>

<style>
  .pdf-viewer-overlay {
    position: fixed;
    top: 0;
    left: 0;
    right: 0;
    bottom: 0;
    background: rgba(0, 0, 0, 0.8);
    z-index: 1000;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  
  .pdf-viewer {
    width: 90vw;
    height: 90vh;
    background: white;
    border-radius: 8px;
    display: flex;
    flex-direction: column;
  }
  
  .pdf-toolbar {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 12px 16px;
    border-bottom: 1px solid #e1e5e9;
    gap: 16px;
  }
  
  .toolbar-section {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  
  .pdf-title {
    font-weight: 600;
    font-size: 16px;
  }
  
  .pdf-content {
    flex: 1;
    overflow: auto;
    padding: 16px;
    display: flex;
    justify-content: center;
  }
  
  .pdf-page iframe {
    width: 100%;
    height: 100%;
    border: none;
  }
  
  input[type="text"] {
    padding: 6px 12px;
    border: 1px solid #e1e5e9;
    border-radius: 4px;
    width: 200px;
  }
  
  button, select {
    padding: 6px 12px;
    border: 1px solid #e1e5e9;
    border-radius: 4px;
    background: white;
    cursor: pointer;
  }
  
  button:hover {
    background: #f8f9fa;
  }
  
  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
```

---

## Plugin Testing Strategy

### Unit Tests

```rust
// nodespace-pdf-node/tests/unit_tests.rs
#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_core::testing::create_test_services;
    
    #[tokio::test]
    async fn test_pdf_metadata_extraction() {
        let services = create_test_services().await;
        let plugin = PDFPlugin::new(services);
        
        let pdf_bytes = include_bytes!("../test_data/sample.pdf");
        let metadata = plugin.extract_pdf_metadata(pdf_bytes).unwrap();
        
        assert_eq!(metadata.title, Some("Sample PDF Document".to_string()));
        assert_eq!(metadata.author, Some("Test Author".to_string()));
        assert!(!metadata.encrypted);
    }
    
    #[tokio::test]
    async fn test_text_extraction() {
        let services = create_test_services().await;
        let plugin = PDFPlugin::new(services);
        
        let pdf_bytes = include_bytes!("../test_data/sample.pdf");
        let extraction = plugin.extract_text_with_pages(pdf_bytes).unwrap();
        
        assert!(!extraction.full_text.is_empty());
        assert_eq!(extraction.pages.len(), 3); // Sample has 3 pages
        assert!(extraction.pages[0].contains("This is page 1"));
    }
    
    #[tokio::test]
    async fn test_embedding_generation() {
        let services = create_test_services().await;
        let plugin = PDFPlugin::new(services);
        
        let text_extraction = TextExtraction {
            document_id: "test_doc".to_string(),
            full_text: "Sample PDF content for testing".to_string(),
            pages: vec!["Sample PDF content for testing".to_string()],
        };
        
        let embeddings = plugin.generate_pdf_embeddings(&text_extraction).await.unwrap();
        
        // Should have document + page + chunk embeddings
        assert!(embeddings.len() >= 3);
        
        // Verify different embedding levels
        let doc_embedding = embeddings.iter()
            .find(|e| matches!(e.metadata.level, EmbeddingLevel::Document))
            .unwrap();
        assert_eq!(doc_embedding.id, "test_doc_document");
    }
    
    #[tokio::test]
    async fn test_validation_rules() {
        let services = create_test_services().await;
        let plugin = PDFPlugin::new(services);
        
        // Test valid PDF node
        let valid_node = create_test_pdf_node();
        let result = plugin.validate_pdf_node(&valid_node).await.unwrap();
        assert!(result.is_valid);
        
        // Test invalid PDF node (missing file)
        let mut invalid_node = create_test_pdf_node();
        invalid_node.file_path = "/nonexistent/file.pdf".to_string();
        
        let result = plugin.validate_pdf_node(&invalid_node).await.unwrap();
        assert!(!result.is_valid);
        assert!(result.violations[0].message.contains("not found"));
    }
}
```

### Integration Tests

```rust
// nodespace-pdf-node/tests/integration_tests.rs
#[cfg(test)]
mod integration_tests {
    use super::*;
    use nodespace_core::testing::{create_real_test_services, TestDatabase};
    
    #[tokio::test]
    async fn test_full_pdf_processing_pipeline() {
        let services = create_real_test_services().await;
        let plugin = PDFPlugin::new(services.clone());
        
        // Create temporary test file
        let test_pdf_path = "/tmp/test_document.pdf";
        let pdf_bytes = include_bytes!("../test_data/sample.pdf");
        tokio::fs::write(test_pdf_path, pdf_bytes).await.unwrap();
        
        // Process PDF through full pipeline
        let pdf_node = plugin.create_from_file(test_pdf_path, None).await.unwrap();
        
        // Verify node was created correctly
        assert_eq!(pdf_node.node_type(), "pdf");
        assert!(pdf_node.page_count > 0);
        assert!(!pdf_node.extracted_text.is_empty());
        
        // Verify storage integration
        let stored_node = services.storage.load_node(&pdf_node.base.id).await.unwrap();
        assert_eq!(stored_node.base.id, pdf_node.base.id);
        
        // Verify embeddings were saved
        let embeddings = services.storage.get_embeddings(&pdf_node.base.id).await.unwrap();
        assert!(!embeddings.is_empty());
        
        // Verify search integration
        let search_results = services.search_engine
            .similarity_search(&embeddings[0].embedding, Default::default())
            .await.unwrap();
        assert!(!search_results.is_empty());
        
        // Cleanup
        tokio::fs::remove_file(test_pdf_path).await.unwrap();
    }
    
    #[tokio::test]
    async fn test_pdf_search_functionality() {
        let services = create_real_test_services().await;
        let plugin = PDFPlugin::new(services.clone());
        
        // Create PDF with known content
        let pdf_node = create_test_pdf_with_content("Machine learning algorithms").await;
        
        // Search for content
        let search_query = "machine learning";
        let query_embedding = services.nlp_engine.embed_query(search_query).await.unwrap();
        
        let search_results = services.search_engine
            .similarity_search(&query_embedding, Default::default())
            .await.unwrap();
        
        // Should find the PDF content
        assert!(!search_results.is_empty());
        let top_result = &search_results[0];
        assert!(top_result.content.to_lowercase().contains("machine learning"));
    }
    
    #[tokio::test]
    async fn test_validation_integration() {
        let services = create_real_test_services().await;
        let plugin = PDFPlugin::new(services.clone());
        
        // Create PDF node with processing error
        let mut pdf_node = create_test_pdf_node();
        pdf_node.processing_status = ProcessingStatus::Failed {
            error: "Text extraction failed".to_string()
        };
        
        // Test validation integration
        let validation_result = services.validation_engine
            .validate_node(&pdf_node)
            .await.unwrap();
        
        assert!(!validation_result.violations.is_empty());
        assert!(validation_result.violations[0].message.contains("processing failed"));
    }
    
    #[tokio::test]
    async fn test_real_time_updates() {
        let services = create_real_test_services().await;
        let plugin = PDFPlugin::new(services.clone());
        
        // Create query that should match PDF content
        let query_node = create_test_query_node("entity_type = 'pdf'");
        
        // Create PDF node - should trigger query update
        let pdf_node = plugin.create_from_file("/tmp/test.pdf", None).await.unwrap();
        
        // Verify query results updated
        let query_results = services.query_manager
            .execute_query(&query_node.query)
            .await.unwrap();
        
        assert!(!query_results.entities.is_empty());
        assert!(query_results.entities.iter().any(|e| e.id == pdf_node.base.id));
    }
}
```

---

## Plugin Registration and Discovery

### Main Application Integration

```rust
// nodespace-app/src-tauri/src/plugin_registry.rs
use std::collections::HashMap;
use std::sync::Arc;

pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn NodePlugin>>,
    components: HashMap<String, String>, // component_name -> plugin_type
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: HashMap::new(),
            components: HashMap::new(),
        }
    }
    
    pub fn register_plugin<P: NodePlugin + 'static>(&mut self, plugin: P) {
        let node_type = plugin.node_type().to_string();
        let component_name = plugin.get_component_name().to_string();
        
        self.components.insert(component_name, node_type.clone());
        self.plugins.insert(node_type, Box::new(plugin));
    }
    
    pub fn get_plugin(&self, node_type: &str) -> Option<&dyn NodePlugin> {
        self.plugins.get(node_type).map(|p| p.as_ref())
    }
    
    pub fn get_component_for_node_type(&self, node_type: &str) -> Option<&str> {
        self.plugins.get(node_type)
            .map(|p| p.get_component_name())
    }
    
    pub fn list_available_node_types(&self) -> Vec<String> {
        self.plugins.keys().cloned().collect()
    }
}

pub fn create_plugin_registry(services: PluginServices) -> PluginRegistry {
    let mut registry = PluginRegistry::new();
    
    // Register core plugins
    registry.register_plugin(nodespace_pdf_node::PDFPlugin::new(services.clone()));
    registry.register_plugin(nodespace_image_node::ImagePlugin::new(services.clone()));
    registry.register_plugin(nodespace_code_node::CodePlugin::new(services.clone()));
    
    registry
}
```

### Tauri Command Integration

```rust
// nodespace-app/src-tauri/src/commands.rs
use tauri::State;

#[tauri::command]
pub async fn create_pdf_node(
    file_path: String,
    parent_id: Option<String>,
    plugin_registry: State<'_, PluginRegistry>
) -> Result<PDFNode, String> {
    let pdf_plugin = plugin_registry.get_plugin("pdf")
        .ok_or("PDF plugin not found")?;
    
    let pdf_plugin = pdf_plugin.as_any()
        .downcast_ref::<PDFPlugin>()
        .ok_or("Invalid plugin type")?;
    
    pdf_plugin.create_from_file(&file_path, parent_id)
        .await
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn validate_node(
    node_id: String,
    node_type: String,
    plugin_registry: State<'_, PluginRegistry>,
    validation_engine: State<'_, ValidationEngine>
) -> Result<ValidationResult, String> {
    match node_type.as_str() {
        "pdf" => {
            let pdf_plugin = plugin_registry.get_plugin("pdf")
                .ok_or("PDF plugin not found")?;
            
            let pdf_node = load_pdf_node(&node_id).await?;
            pdf_plugin.validate_pdf_node(&pdf_node)
                .await
                .map_err(|e| e.to_string())
        },
        "image" => {
            // Similar pattern for image plugin
            todo!("Image validation")
        },
        _ => {
            // Use generic validation engine
            validation_engine.validate_node_by_id(&node_id)
                .await
                .map_err(|e| e.to_string())
        }
    }
}
```

### Frontend Component Registration

```typescript
// nodespace-app/src/lib/ComponentRegistry.ts
import type { SvelteComponent } from 'svelte';

// Import plugin components
import PDFNode from '../../../nodespace-pdf-node/src/ui/PDFNode.svelte';
import ImageNode from '../../../nodespace-image-node/src/ui/ImageNode.svelte';
import CodeNode from '../../../nodespace-code-node/src/ui/CodeNode.svelte';

export interface NodeComponentRegistry {
  [nodeType: string]: typeof SvelteComponent;
}

export const nodeComponents: NodeComponentRegistry = {
  'pdf': PDFNode,
  'image': ImageNode,
  'code': CodeNode,
};

export function getComponentForNodeType(nodeType: string): typeof SvelteComponent | null {
  return nodeComponents[nodeType] || null;
}

export function registerNodeComponent(nodeType: string, component: typeof SvelteComponent) {
  nodeComponents[nodeType] = component;
}
```

```svelte
<!-- nodespace-app/src/components/NodeRenderer.svelte -->
<script lang="ts">
  import { getComponentForNodeType } from '../lib/ComponentRegistry';
  import type { BaseNode } from '../types/Node';
  
  export let node: BaseNode;
  export let isSelected: boolean = false;
  export let isEditing: boolean = false;
  
  $: Component = getComponentForNodeType(node.node_type);
</script>

{#if Component}
  <svelte:component 
    this={Component} 
    {node} 
    {isSelected}
    {isEditing}
    on:node-update
    on:node-action
  />
{:else}
  <div class="unknown-node-type">
    <p>Unknown node type: {node.node_type}</p>
    <p>Component not registered or plugin not loaded.</p>
  </div>
{/if}

<style>
  .unknown-node-type {
    padding: 16px;
    border: 1px dashed #ccc;
    border-radius: 4px;
    color: #666;
    text-align: center;
  }
</style>
```

---

## Build Process Integration

### Workspace Configuration

```toml
# Root Cargo.toml
[workspace]
members = [
    "nodespace-app/src-tauri",
    "nodespace-pdf-node",
    "nodespace-image-node", 
    "nodespace-code-node"
]

[workspace.dependencies]
nodespace-core = { path = "./nodespace-core" }
tokio = { version = "1.0", features = ["full"] }
serde = { version = "1.0", features = ["derive"] }
```

```json
// Root package.json
{
  "name": "nodespace-workspace",
  "private": true,
  "workspaces": [
    "nodespace-app",
    "nodespace-pdf-node",
    "nodespace-image-node",
    "nodespace-code-node"
  ],
  "scripts": {
    "dev": "bun run dev:plugins && bun run dev:app",
    "dev:plugins": "bun run --parallel dev:pdf dev:image dev:code",
    "dev:pdf": "cd nodespace-pdf-node && bun run dev",
    "dev:image": "cd nodespace-image-node && bun run dev", 
    "dev:code": "cd nodespace-code-node && bun run dev",
    "dev:app": "cd nodespace-app && bun run tauri dev",
    
    "build": "bun run build:plugins && bun run build:app",
    "build:plugins": "bun run --parallel build:pdf build:image build:code",
    "build:pdf": "cd nodespace-pdf-node && bun run build",
    "build:image": "cd nodespace-image-node && bun run build",
    "build:code": "cd nodespace-code-node && bun run build", 
    "build:app": "cd nodespace-app && bun run tauri build",
    
    "test": "bun run test:plugins && bun run test:app",
    "test:plugins": "bun run --parallel test:pdf test:image test:code",
    "test:pdf": "cd nodespace-pdf-node && cargo test && bun test",
    "test:image": "cd nodespace-image-node && cargo test && bun test",
    "test:code": "cd nodespace-code-node && cargo test && bun test",
    "test:app": "cd nodespace-app && cargo test && bun test"
  }
}
```

### Plugin Build Script

```javascript
// build-plugins.js
#!/usr/bin/env bun

import { $ } from "bun";
import fs from "fs";
import path from "path";

const PLUGINS = [
  "nodespace-pdf-node",
  "nodespace-image-node", 
  "nodespace-code-node"
];

console.log("🔨 Building NodeSpace Plugins...");

for (const plugin of PLUGINS) {
  console.log(`\n📦 Building ${plugin}...`);
  
  try {
    // Build Rust plugin
    await $`cd ${plugin} && cargo build --release`;
    console.log(`✅ ${plugin} Rust build complete`);
    
    // Build Svelte components
    await $`cd ${plugin} && bun run build`;
    console.log(`✅ ${plugin} Svelte build complete`);
    
    // Copy build artifacts
    const distDir = path.join(plugin, "dist");
    const targetDir = path.join("nodespace-app", "src", "plugins", plugin);
    
    if (fs.existsSync(distDir)) {
      await $`cp -r ${distDir} ${targetDir}`;
      console.log(`✅ ${plugin} artifacts copied`);
    }
    
  } catch (error) {
    console.error(`❌ ${plugin} build failed:`, error);
    process.exit(1);
  }
}

console.log("\n🎉 All plugins built successfully!");
```

### Development Workflow

```bash
# Start development environment
bun run dev

# This runs:
# 1. All plugins in watch mode (Rust + Svelte)
# 2. Main Tauri app in dev mode
# 3. Hot reload for both backend and frontend changes

# Run tests across all plugins
bun run test

# Build for production
bun run build

# Test specific plugin
cd nodespace-pdf-node
cargo test          # Rust tests
bun test           # Svelte/TypeScript tests
```

---

## Performance Considerations

### Plugin Loading Optimization

```rust
// Lazy initialization of heavy plugin resources
pub struct PDFPlugin {
    services: PluginServices,
    text_extractor: OnceCell<Arc<TextExtractor>>,
    thumbnail_generator: OnceCell<Arc<ThumbnailGenerator>>,
}

impl PDFPlugin {
    fn get_text_extractor(&self) -> Arc<TextExtractor> {
        self.text_extractor.get_or_init(|| {
            Arc::new(TextExtractor::new())
        }).clone()
    }
    
    async fn process_pdf_lazy(&self, pdf_bytes: &[u8]) -> Result<ProcessingResult, Error> {
        // Only initialize heavy resources when actually needed
        let extractor = self.get_text_extractor();
        extractor.extract_text(pdf_bytes).await
    }
}
```

### Caching Strategy

```rust
// Plugin-level caching for expensive operations
pub struct PluginCache {
    text_extractions: LruCache<String, TextExtraction>,
    thumbnails: LruCache<String, String>, // file_hash -> thumbnail_path
    embeddings: LruCache<String, Vec<EmbeddingData>>,
}

impl PDFPlugin {
    async fn extract_text_cached(&self, pdf_bytes: &[u8]) -> Result<TextExtraction, Error> {
        let file_hash = calculate_hash(pdf_bytes);
        
        if let Some(cached) = self.cache.text_extractions.get(&file_hash) {
            return Ok(cached.clone());
        }
        
        let extraction = self.extract_text_with_pages(pdf_bytes)?;
        self.cache.text_extractions.put(file_hash, extraction.clone());
        
        Ok(extraction)
    }
}
```

### Memory Management

```rust
// Stream processing for large files
impl PDFPlugin {
    async fn process_large_pdf(&self, file_path: &str) -> Result<ProcessingResult, Error> {
        use tokio::io::{AsyncReadExt, BufReader};
        
        let file = tokio::fs::File::open(file_path).await?;
        let mut reader = BufReader::new(file);
        
        // Process PDF in chunks to avoid loading entire file into memory
        let mut buffer = vec![0; 1024 * 1024]; // 1MB chunks
        let mut processor = StreamingPDFProcessor::new();
        
        loop {
            let bytes_read = reader.read(&mut buffer).await?;
            if bytes_read == 0 { break; }
            
            processor.process_chunk(&buffer[..bytes_read]).await?;
        }
        
        processor.finalize().await
    }
}
```

---

## Security Considerations

### Plugin Sandboxing

```rust
// Validation of plugin operations
impl PluginSandbox {
    pub fn validate_file_operation(&self, plugin_id: &str, file_path: &str) -> Result<(), Error> {
        // Ensure plugins can only access their designated directories
        let allowed_path = format!("/app/data/plugins/{}", plugin_id);
        
        if !file_path.starts_with(&allowed_path) {
            return Err(Error::SecurityViolation(
                format!("Plugin {} attempted to access unauthorized path: {}", plugin_id, file_path)
            ));
        }
        
        Ok(())
    }
    
    pub fn validate_network_request(&self, plugin_id: &str, url: &str) -> Result<(), Error> {
        // Restrict network access for plugins
        let allowed_domains = self.get_allowed_domains(plugin_id);
        
        if !allowed_domains.iter().any(|domain| url.contains(domain)) {
            return Err(Error::SecurityViolation(
                format!("Plugin {} attempted unauthorized network request: {}", plugin_id, url)
            ));
        }
        
        Ok(())
    }
}
```

### Input Validation

```rust
impl PDFPlugin {
    fn validate_pdf_input(&self, pdf_bytes: &[u8]) -> Result<(), Error> {
        // Check file size limits
        if pdf_bytes.len() > 100_000_000 { // 100MB
            return Err(Error::FileTooLarge);
        }
        
        // Verify PDF header
        if !pdf_bytes.starts_with(b"%PDF-") {
            return Err(Error::InvalidFileFormat);
        }
        
        // Basic malware detection
        if self.contains_suspicious_patterns(pdf_bytes) {
            return Err(Error::SuspiciousContent);
        }
        
        Ok(())
    }
    
    fn contains_suspicious_patterns(&self, pdf_bytes: &[u8]) -> bool {
        // Check for embedded JavaScript, suspicious URLs, etc.
        let content = String::from_utf8_lossy(pdf_bytes);
        
        content.contains("/JavaScript") || 
        content.contains("/JS") ||
        content.contains("eval(") ||
        content.contains("unescape(")
    }
}
```

---

This plugin architecture specification provides a comprehensive foundation for building extensible, performant, and secure plugins within NodeSpace. The system enables independent development while maintaining full integration with core features like validation, search, and real-time updates.

The build-time compilation approach ensures optimal performance while the service injection pattern enables thorough testing with real implementations rather than mocks. This architecture supports the parallel development workflow necessary for building complex, specialized node types while maintaining a consistent user experience across all plugins.