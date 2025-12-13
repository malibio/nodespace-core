# Semantic Code Search Architecture

Enable developers and AI agents to search codebases using natural language queries like "where do we validate user credentials?" instead of exact string matching.

## Overview

NodeSpace's semantic code search parses local repositories using tree-sitter, extracts symbols (functions, classes, types), creates searchable representations, and embeds them using the existing bge-small embedding infrastructure. This provides conceptual matching that grep/ripgrep cannot achieve.

### Value Proposition

| Approach | Query | Result |
|----------|-------|--------|
| **grep** | `grep "auth"` | 847 matches, mostly noise |
| **Semantic** | "where do we handle authentication?" | Top 3 relevant functions |

For AI agents via MCP, this reduces token usage by 2-5x by eliminating file-reading loops and grep false positives.

---

## Node Type Hierarchy

Semantic code search introduces three new node types that integrate with NodeSpace's existing node system:

```
CodeRepositoryNode (root container)
    │
    └── has_child → CodeFileNode (one per source file)
                        │
                        └── has_child → CodeSymbolNode (EMBEDDABLE)
```

### CodeRepositoryNode

Represents a local codebase being indexed.

```typescript
interface CodeRepositoryNode {
  // Hub fields
  id: string;
  nodeType: 'code-repository';
  content: string;              // Display name, e.g., "nodespace-core"
  version: number;
  createdAt: string;
  modifiedAt: string;

  // Spoke fields
  path: string;                 // "/Users/me/projects/nodespace-core"
  languages: string[];          // ["rust", "typescript", "svelte"]
  lastScan: string | null;      // ISO timestamp of last full scan
  fileCount: number;            // Total indexed files
  symbolCount: number;          // Total indexed symbols
  watchEnabled: boolean;        // Is file watcher active?
  scanInterval: number;         // Minutes between scans (default: 5)

  // Git info (optional)
  gitRemote?: string | null;    // "https://github.com/org/repo"
  gitBranch?: string | null;    // "main"
}
```

### CodeFileNode

Represents a single source file within a repository.

```typescript
interface CodeFileNode {
  // Hub fields
  id: string;
  nodeType: 'code-file';
  content: string;              // Relative path: "src/auth/login.rs"
  version: number;
  createdAt: string;
  modifiedAt: string;

  // Spoke fields
  language: string;             // "rust"
  fileHash: string;             // SHA256 for change detection
  lineCount: number;
  byteSize: number;
  lastModified: string;         // File mtime from filesystem
  symbolCount: number;          // Symbols extracted from this file
}
```

### CodeSymbolNode (Embeddable)

Represents an extracted code symbol (function, class, struct, etc.). This is the **embeddable** node type.

```typescript
interface CodeSymbolNode {
  // Hub fields
  id: string;
  nodeType: 'code-symbol';
  content: string;              // The actual source code (for display)
  version: number;
  createdAt: string;
  modifiedAt: string;

  // Spoke fields
  symbolName: string;           // "validate_credentials"
  symbolType: string;           // tree-sitter node type: "function_item", "class_declaration"
  symbolKind?: SymbolKind;      // Optional categorization: 'function' | 'type' | 'value' | 'container'
  startLine: number;
  endLine: number;
  language: string;             // "rust"

  // Metadata (flexible, language-specific)
  metadata?: {
    visibility?: string;        // "pub", "export", "private"
    signature?: string;         // "pub fn validate_credentials(creds: &Credentials) -> Result<User>"
    docstring?: string;         // Extracted documentation comments
    [key: string]: unknown;     // Language-specific extensions
  };
}

// High-level categorization for UI grouping (optional)
type SymbolKind = 'function' | 'type' | 'value' | 'container' | 'other';
```

---

## Extraction Pipeline

### Phase 1: Repository Scanning

```
User adds repository path
        │
        ▼
┌─────────────────────────────────────────┐
│  Scanner Service                        │
│  1. Create CodeRepositoryNode           │
│  2. Walk directory tree                 │
│  3. Respect .gitignore                  │
│  4. Detect languages by extension       │
└─────────────────────────────────────────┘
```

### Phase 2: File Processing

For each source file:

1. Compute SHA256 hash
2. Check if CodeFileNode exists with same hash → skip if unchanged
3. Select tree-sitter grammar by file extension
4. Parse file into AST
5. Extract symbols using language-specific queries
6. Create/update CodeFileNode and child CodeSymbolNodes

### Phase 3: Embedding (Existing Pipeline)

CodeSymbolNode is marked as embeddable, so the existing embedding pipeline handles it:

1. `queue_for_embedding(symbolNodeId)` called on node creation/update
2. Background processor picks up stale embeddings
3. Generates embedding from **searchable representation** (not raw code)
4. Stores in `embedding` table

---

## tree-sitter Integration

### Language Detection

```rust
fn get_language(extension: &str) -> Option<Language> {
    match extension {
        // Rust
        "rs" => Some(tree_sitter_rust::LANGUAGE.into()),

        // TypeScript / JavaScript
        "ts" | "mts" | "cts" => Some(tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into()),
        "tsx" => Some(tree_sitter_typescript::LANGUAGE_TSX.into()),
        "js" | "mjs" | "cjs" | "jsx" => Some(tree_sitter_javascript::LANGUAGE.into()),

        // Svelte
        "svelte" => Some(tree_sitter_svelte::LANGUAGE.into()),

        // Others
        "py" => Some(tree_sitter_python::LANGUAGE.into()),
        "go" => Some(tree_sitter_go::LANGUAGE.into()),

        _ => None, // Unsupported - skip file
    }
}
```

### Rust Dependencies

```toml
[dependencies]
tree-sitter = "0.24"
tree-sitter-rust = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-python = "0.23"
tree-sitter-svelte = "0.11"
tree-sitter-go = "0.23"
```

### Symbol Extraction Queries

Each language has a query file defining what to extract. Example for Rust:

```scheme
;; Functions
(function_item
  (visibility_modifier)? @visibility
  name: (identifier) @name
  parameters: (parameters) @params
  return_type: (_)? @return_type
) @function

;; Structs
(struct_item
  (visibility_modifier)? @visibility
  name: (type_identifier) @name
) @struct

;; Impl blocks
(impl_item
  type: (type_identifier) @type
) @impl

;; Traits
(trait_item
  (visibility_modifier)? @visibility
  name: (type_identifier) @name
) @trait
```

---

## Searchable Representation

**Critical insight**: We don't embed raw code. We embed a humanized, searchable representation.

### Why?

Query: `"where do we validate user credentials?"`

Raw code won't match well:
```rust
pub fn validate_credentials(creds: &Credentials) -> Result<User, AuthError> {
    let user = find_user_by_email(&creds.email)?;
```

Humanized representation matches better:
```
validate credentials
Validates user credentials against stored hash
pub fn validate_credentials(creds: &Credentials) -> Result<User, AuthError>
auth credentials
find user by email verify password
```

### Transformation Logic

```typescript
function buildSearchableContent(symbol: CodeSymbol): string {
  const parts: string[] = [];

  // Humanize symbol name: "validate_credentials" → "validate credentials"
  parts.push(humanizeIdentifier(symbol.symbolName));

  // Include docstring (already natural language)
  if (symbol.metadata?.docstring) {
    parts.push(symbol.metadata.docstring);
  }

  // Include signature (somewhat readable)
  if (symbol.metadata?.signature) {
    parts.push(symbol.metadata.signature);
  }

  // Humanize file path for context: "src/auth/credentials.rs" → "auth credentials"
  parts.push(humanizeFilePath(symbol.filePath));

  // Extract and humanize identifiers from code body
  const identifiers = extractIdentifiers(symbol.content);
  parts.push(identifiers.map(humanizeIdentifier).join(' '));

  return parts.join('\n');
}

function humanizeIdentifier(id: string): string {
  // snake_case: validate_credentials → validate credentials
  // camelCase: validateCredentials → validate credentials
  // PascalCase: ValidateCredentials → validate credentials
  return id
    .replace(/_/g, ' ')
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .toLowerCase();
}
```

### Storage

```typescript
interface CodeSymbolNode {
  content: string;              // Actual code (for display, copy/paste)
  // The searchable representation is computed at embedding time,
  // not stored separately. The embedding service transforms content
  // before generating the vector.
}
```

---

## File Watching

### notify Crate (Recommended)

```rust
use notify::{Watcher, RecursiveMode, recommended_watcher, Event};
use std::time::Duration;

fn watch_repository(path: &Path, tx: Sender<PathBuf>) -> Result<()> {
    let mut watcher = recommended_watcher(move |res: Result<Event, _>| {
        if let Ok(event) = res {
            for path in event.paths {
                let _ = tx.send(path);
            }
        }
    })?;

    watcher.watch(path, RecursiveMode::Recursive)?;
    Ok(())
}
```

### Debouncing

File changes are debounced before triggering re-indexing:

```rust
struct FileChangeQueue {
    pending: HashMap<PathBuf, Instant>,
    debounce_duration: Duration,  // Default: 2 seconds
}
```

### Polling Fallback

For simplicity or when notify has issues, polling every N minutes:

```rust
async fn poll_repository(repo: &CodeRepository, interval: Duration) {
    loop {
        tokio::time::sleep(interval).await;
        scan_for_changes(repo).await;
    }
}
```

---

## Search Flow

### Query Processing

```
┌─────────────────────────────────────────────────────────────────┐
│  User: "where do we validate user credentials?"                 │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Embedding Model (bge-small-en-v1.5)                            │
│  Query → [384-dim vector]                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  SurrealDB Vector Search                                        │
│  cosine_similarity(query_vec, stored_vec) > threshold           │
│  Filter: node_type = 'code-symbol'                              │
│  ORDER BY similarity DESC LIMIT 10                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Hydrate Results                                                │
│  CodeSymbolNode → CodeFileNode → CodeRepositoryNode             │
│  Build: file path, line numbers, snippet                        │
└─────────────────────────────────────────────────────────────────┘
```

### Response Structure

```typescript
interface CodeSearchResult {
  query: string;
  results: CodeSearchHit[];
  totalMatches: number;
  searchTimeMs: number;
}

interface CodeSearchHit {
  score: number;              // Similarity score (0-1)

  // Symbol info
  symbolName: string;
  symbolType: string;
  symbolKind?: SymbolKind;

  // Location
  repository: string;         // Repository display name
  filePath: string;           // Relative path
  startLine: number;
  endLine: number;

  // Content
  snippet: string;            // First ~10 lines or signature
  fullContent: string;        // Complete code

  // Context
  docstring?: string;
  signature?: string;
}
```

---

## MCP Integration

### Tools

```typescript
// Semantic search across indexed repositories
{
  name: "search_code",
  description: "Search code using natural language",
  parameters: {
    query: string,            // "where do we handle authentication?"
    repository?: string,      // Filter to specific repo
    language?: string,        // Filter by language
    symbolKind?: SymbolKind,  // Filter: 'function' | 'type' | 'class'
    limit?: number,           // Max results (default: 10)
    threshold?: number,       // Min similarity (default: 0.5)
  }
}

// Get symbol by name (exact match)
{
  name: "get_symbol",
  description: "Find symbol definition by exact name",
  parameters: {
    name: string,             // "validate_credentials"
    repository?: string,
  }
}

// List symbols in a file
{
  name: "get_file_symbols",
  description: "List all symbols in a file",
  parameters: {
    filePath: string,         // "src/auth/login.rs"
    repository?: string,
  }
}

// Index a new repository
{
  name: "add_code_repository",
  description: "Add a local repository for indexing",
  parameters: {
    path: string,             // "/Users/me/projects/my-app"
    watchEnabled?: boolean,   // Enable file watching (default: true)
  }
}
```

---

## Scale Considerations

### Default Limits

```rust
struct ScanConfig {
    max_files: usize,              // Default: 10,000
    max_file_size_bytes: usize,    // Default: 1MB (skip huge generated files)
    max_depth: usize,              // Default: 20 directory levels
    max_symbols_per_file: usize,   // Default: 500
}
```

### Skip Patterns (Default)

```
node_modules/
target/
dist/
build/
.git/
__pycache__/
vendor/
*.min.js
*.bundle.js
```

### Indexing Performance Targets

| Repo Size | Files | Est. Time |
|-----------|-------|-----------|
| Small | ~500 | ~30 seconds |
| Medium | ~2,000 | ~2 minutes |
| Large | ~10,000 | ~10 minutes |

Indexing runs in background; search works on already-indexed content while scan continues.

---

## Phased Roadmap

### Phase 1: Core Symbol Search (MVP)

- [ ] CodeRepository, CodeFile, CodeSymbol node types
- [ ] tree-sitter integration (Rust, TypeScript, JavaScript, Svelte, Python)
- [ ] Searchable representation generation
- [ ] Integration with existing embedding pipeline
- [ ] MCP tools: `search_code`, `get_symbol`, `get_file_symbols`
- [ ] Basic file watching (notify crate)
- [ ] UI: Add repository, view indexed repos, search

### Phase 2: Relationships & Graph

- [ ] Call graph extraction (function A calls function B)
- [ ] Type usage edges (function uses struct)
- [ ] Import/dependency tracking
- [ ] MCP tools: `get_callers`, `get_callees`, `get_dependencies`
- [ ] "What would break if I change X?" queries

### Phase 3: Enhanced Intelligence

- [ ] LLM-generated summaries (optional, local model)
- [ ] Cross-repository search
- [ ] Symbol rename tracking across commits
- [ ] "Explain this function" via local LLM
- [ ] IDE integration (VS Code extension)

---

## Integration with Existing Architecture

### Reuses

| Component | How It's Used |
|-----------|---------------|
| Node system | CodeSymbol is just another node type |
| has_child edges | Repository → File → Symbol hierarchy |
| Embedding pipeline | CodeSymbol marked embeddable, uses existing flow |
| bge-small model | No model change needed (humanized content is natural language) |
| SurrealDB vector search | Existing cosine similarity queries |
| MCP server | New tools added to existing handler |

### New Components

| Component | Purpose |
|-----------|---------|
| Scanner service | File walking, hash comparison, orchestration |
| tree-sitter wrapper | Multi-language parsing |
| Language queries | Per-language symbol extraction |
| Searchable transformer | Code → humanized representation |
| File watcher | notify crate integration |

---

## References

- [tree-sitter](https://tree-sitter.github.io/tree-sitter/) - Parsing library
- [notify crate](https://docs.rs/notify/) - Filesystem watching
- [Google CodeWiki](https://developers.googleblog.com/en/introducing-code-wiki-accelerating-your-code-understanding/) - Similar approach (tree-sitter + embeddings)
- [Serena](https://github.com/oraios/serena) - Alternative approach (LSP-based)
- [mgrep](https://github.com/mixedbread-ai/mgrep) - Semantic grep tool

---

*Document created: December 2025*
*Status: Design Phase*
