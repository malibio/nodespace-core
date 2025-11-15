# Identifier Naming Conventions - NodeSpace Development Standards

**Status**: ✅ ACTIVE STANDARD
**Effective**: November 2025
**Scope**: All code identifiers (variables, functions, types, database fields)

## Overview

This document establishes consistent naming conventions for identifiers across TypeScript, Rust, and database schemas. Proper naming ensures code clarity, reduces bugs from case mismatches, and improves cross-language interoperability.

## Language-Specific Conventions

### TypeScript/JavaScript

#### Variables and Constants

```typescript
// ✅ Variables - camelCase
const nodeId = 'abc-123';
let currentNode = null;
const schemaDefinition = await getSchema('task');

// ✅ Constants (configuration values) - camelCase
const maxRetries = 3;
const defaultTimeout = 5000;

// ✅ Constants (enums/readonly values) - SCREAMING_SNAKE_CASE
const NODE_TYPE_TEXT = 'text';
const MAX_CHILDREN = 1000;
const API_BASE_URL = 'http://localhost:3001';

// ❌ Incorrect
const NodeId = 'abc-123';  // Should be camelCase
const node_id = 'abc-123'; // Should be camelCase
const max_retries = 3;     // Should be camelCase or SCREAMING_SNAKE_CASE
```

#### Functions and Methods

```typescript
// ✅ Correct - camelCase
function getNodeById(id: string) { }
async function updateNodeProperties(nodeId: string, props: object) { }
const formatDate = (date: Date) => string;

// ❌ Incorrect
function GetNodeById(id: string) { }  // Should be camelCase
function get_node_by_id(id: string) { } // Should be camelCase
```

#### Types and Interfaces

```typescript
// ✅ Interfaces - PascalCase
interface SchemaDefinition {
  isCore: boolean;
  version: number;
  fields: SchemaField[];
}

interface NodeUpdate {
  content?: string;
  properties?: Record<string, unknown>;
}

// ✅ Type Aliases - PascalCase
type NodeType = 'text' | 'task' | 'date';
type ProtectionLevel = 'core' | 'user' | 'system';

// ✅ Enums - PascalCase with SCREAMING_SNAKE_CASE values
enum TaskStatus {
  OPEN = 'OPEN',
  IN_PROGRESS = 'IN_PROGRESS',
  DONE = 'DONE'
}

// ❌ Incorrect
interface schemaDefinition { }  // Should be PascalCase
type nodeType = string;         // Should be PascalCase
```

### Rust

#### Variables and Function Parameters

```rust
// ✅ Correct - snake_case
let node_id = "abc-123".to_string();
let schema_definition = get_schema("task").await?;
let field_type = "enum";

// ❌ Incorrect
let NodeId = "abc-123";  // Should be snake_case
let nodeId = "abc-123";  // Should be snake_case
```

#### Struct Fields

```rust
// ✅ Correct - snake_case (with serde rename for JSON)
#[derive(Serialize, Deserialize)]
pub struct SchemaField {
    pub name: String,

    #[serde(rename = "type")]  // JSON: "type", Rust: field_type
    pub field_type: String,

    pub core_values: Option<Vec<String>>,
    pub user_values: Option<Vec<String>>,
}

// ❌ Incorrect
pub struct SchemaField {
    pub Name: String,        // Should be snake_case
    pub fieldType: String,   // Should be snake_case
    pub coreValues: Option<Vec<String>>,  // Should be snake_case
}
```

#### Functions and Methods

```rust
// ✅ Correct - snake_case
pub async fn get_node(&self, id: &str) -> Result<Option<Node>> { }
pub async fn update_node_properties(&self, id: &str, props: Value) -> Result<()> { }

// ❌ Incorrect
pub async fn GetNode(&self, id: &str) { }  // Should be snake_case
pub async fn getNode(&self, id: &str) { }  // Should be snake_case
```

#### Types and Structs

```rust
// ✅ Correct - PascalCase
pub struct Node { }
pub struct SchemaDefinition { }
pub enum ProtectionLevel {
    Core,
    User,
    System,
}

// ❌ Incorrect
pub struct node { }           // Should be PascalCase
pub struct schema_definition { }  // Should be PascalCase
```

#### Constants

```rust
// ✅ Correct - SCREAMING_SNAKE_CASE
const MAX_RETRIES: u32 = 3;
const DEFAULT_PORT: u16 = 3001;
const NODE_TYPE_TEXT: &str = "text";

// ❌ Incorrect
const maxRetries: u32 = 3;    // Should be SCREAMING_SNAKE_CASE
const max_retries: u32 = 3;   // lowercase - should be SCREAMING
```

### Database and JSON

#### Field Names in Database

```
✅ Correct - snake_case
- node_type
- created_at
- updated_at
- parent_id
- container_node_id
- field_type
- core_values

❌ Incorrect
- nodeType    // Should be snake_case
- createdAt   // Should be snake_case
- parentId    // Should be snake_case
```

**Rationale**:
- SQL convention uses snake_case
- SurrealDB supports both but snake_case is database standard
- Consistent with Rust struct field names (pre-serialization)

## Cross-Language Serialization

### Rust ↔ JSON ↔ TypeScript

**Standard Pattern:**

```rust
// Rust struct (snake_case fields)
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]  // ✅ Auto-converts to camelCase JSON
pub struct Node {
    pub node_type: String,      // → JSON: "nodeType"
    pub created_at: String,     // → JSON: "createdAt"
    pub parent_id: Option<String>,  // → JSON: "parentId"
}

// Special cases - explicit rename
#[derive(Serialize, Deserialize)]
pub struct SchemaField {
    #[serde(rename = "type")]  // Avoid "field_type" in JSON
    pub field_type: String,
}
```

**TypeScript interface (camelCase fields):**

```typescript
interface Node {
  nodeType: string;     // Matches JSON from Rust
  createdAt: string;    // Matches JSON from Rust
  parentId?: string;    // Matches JSON from Rust
}

interface SchemaField {
  type: string;  // Matches Rust's #[serde(rename = "type")]
}
```

**JSON representation (camelCase):**

```json
{
  "nodeType": "text",
  "createdAt": "2025-01-15T10:30:00Z",
  "parentId": "abc-123",
  "type": "enum"
}
```

## NodeSpace-Specific Terminology Standards

### Property vs Field

**Rule**: Use **field** for schema definitions, **property** for node data values.

```typescript
// ✅ Correct - Schema context
interface SchemaField {
  name: string;
  type: string;
}
const fields = schema.fields;

// ✅ Correct - Node data context
interface Node {
  properties: Record<string, unknown>;
}
const taskPriority = node.properties.priority;

// ❌ Incorrect - Mixed terminology
const schemaProperties = schema.properties;  // Should be "fields"
const nodeFields = node.fields;  // Should be "properties"
```

**Rationale**: Clear distinction between schema structure (fields) and node data (properties).

### Type vs NodeType

**Rule**: Always use **nodeType** (full qualifier) to avoid ambiguity with TypeScript's `type` keyword and schema field types.

```typescript
// ✅ Correct - Unambiguous
interface Node {
  nodeType: string;  // "text", "task", "date"
}

const node = await getNode(id);
if (node.nodeType === 'task') { }

// ✅ Correct - Schema field type context
interface SchemaField {
  type: string;  // "enum", "date", "string", "number"
}

// ❌ Incorrect - Ambiguous
interface Node {
  type: string;  // Confusing - what kind of type?
}

// ❌ Exception that exists but should be avoided in new code
const { type } = node;  // Destructuring 'type' from node is ambiguous
```

**Current State**: The codebase has some legacy `type` usage (see markdown-renderer.svelte). New code should use `nodeType` consistently.

**Migration Strategy**: Accept existing `type` usage as technical debt. New code must use `nodeType`.

### ID Naming

**Rule**: Always suffix with `Id` (camelCase in TS, snake_case in Rust).

```typescript
// ✅ Correct
const nodeId = 'abc-123';
const parentId = node.parentId;
const schemaId = 'task';
const containerId = container.id;

// ❌ Incorrect
const node_id = 'abc-123';  // Should be camelCase in TypeScript
const parent = node.parent;  // Ambiguous - is it ID or object?
const schema = 'task';       // Unclear if ID or object
```

```rust
// ✅ Correct
let node_id = "abc-123".to_string();
let parent_id = node.parent_id;
let schema_id = "task";

// ❌ Incorrect
let nodeId = "abc-123";  // Should be snake_case in Rust
```

### Timestamps

**Rule**: Use `created_at` / `updated_at` pattern (snake_case in Rust/DB, camelCase in TS).

```rust
// ✅ Rust/Database
pub struct Node {
    pub created_at: String,
    pub updated_at: String,
}
```

```typescript
// ✅ TypeScript (after serialization)
interface Node {
  createdAt: string;
  updatedAt: string;
}
```

### Service and State Suffixes

```typescript
// ✅ Services - always end with "Service"
const nodeService = new NodeService();
const schemaService = new SchemaService();
const embeddingService = new EmbeddingService();

// ✅ Stores - always end with "Store"
const sharedNodeStore = new SharedNodeStore();
const navigationStore = new NavigationStore();

// ✅ Controllers - always end with "Controller"
const textareaController = new TextareaController();
const focusController = new FocusController();

// ❌ Incorrect
const nodeManager = new NodeService();  // Inconsistent - should be nodeService
const nodes = new SharedNodeStore();    // Unclear - should be nodesStore or sharedNodeStore
```

## Case Style Reference Table

| Context | Case Style | Example |
|---------|-----------|---------|
| **TypeScript Variables** | camelCase | `nodeId`, `schemaDefinition` |
| **TypeScript Constants (config)** | camelCase | `maxRetries`, `defaultPort` |
| **TypeScript Constants (readonly)** | SCREAMING_SNAKE_CASE | `MAX_CHILDREN`, `API_BASE_URL` |
| **TypeScript Functions** | camelCase | `getNode`, `updateSchema` |
| **TypeScript Interfaces/Types** | PascalCase | `Node`, `SchemaField` |
| **TypeScript Enum Values** | SCREAMING_SNAKE_CASE | `TaskStatus.IN_PROGRESS` |
| **Rust Variables** | snake_case | `node_id`, `schema_definition` |
| **Rust Functions** | snake_case | `get_node`, `update_schema` |
| **Rust Structs/Enums** | PascalCase | `Node`, `SchemaField` |
| **Rust Constants** | SCREAMING_SNAKE_CASE | `MAX_RETRIES`, `DEFAULT_PORT` |
| **Database Fields** | snake_case | `node_type`, `created_at` |
| **JSON (from Rust)** | camelCase | `nodeType`, `createdAt` |
| **File Names** | kebab-case | `schema-service.ts`, `node_service.rs` |

## Serialization Conventions

### Automatic Case Conversion

**Standard Pattern**: Use serde attributes for automatic conversion.

```rust
// ✅ Preferred - Automatic conversion
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Node {
    pub node_type: String,      // JSON: "nodeType"
    pub created_at: String,     // JSON: "createdAt"
    pub parent_id: Option<String>,  // JSON: "parentId"
}
```

### Explicit Field Renames

**Use when**: Avoiding reserved keywords or improving API clarity.

```rust
// ✅ Explicit rename to avoid "field_type" in JSON
#[derive(Serialize, Deserialize)]
pub struct SchemaField {
    #[serde(rename = "type")]
    pub field_type: String,  // JSON: "type", Rust: field_type
}
```

**Rationale**: `type` is clearer in JSON/TypeScript than `fieldType`, and avoids the redundant "field" prefix since it's already in a field context.

### HTTP Adapter Consistency

**Critical Rule**: HTTP adapters must match the actual JSON serialization from Rust.

```typescript
// ✅ Correct - Matches Rust's #[serde(rename = "type")]
const schema = await invoke<{
  fields: Array<{
    type: string;  // Rust sends "type", not "field_type"
  }>
}>('get_schema_definition', { schemaId });

// ❌ Incorrect - Expects wrong field name
const schema = await invoke<{
  fields: Array<{
    field_type: string;  // Rust sends "type", not "field_type"!
  }>
}>('get_schema_definition', { schemaId });
```

## NodeSpace Domain Terminology

### Core Concepts - Standardized Names

| Concept | TypeScript | Rust | Database | Notes |
|---------|-----------|------|----------|-------|
| **Node category** | `nodeType` | `node_type` | `node_type` | ALWAYS qualified (never just "type") |
| **Schema field category** | `type` | `field_type` (renamed to "type" in JSON) | `type` | Context makes it clear |
| **Node identifier** | `nodeId` or `id` | `node_id` or `id` | `id` | Use `id` when context is clear |
| **Parent reference** | `parentId` | `parent_id` | `parent_id` | ALWAYS suffixed with Id |
| **Container reference** | `containerId` or `containerNodeId` | `container_id` or `container_node_id` | `container_node_id` | Full name in DB for clarity |
| **Schema identifier** | `schemaId` | `schema_id` | `id` (in schema nodes) | Use `id` when context is schema |
| **Timestamps** | `createdAt`, `updatedAt` | `created_at`, `updated_at` | `created_at`, `updated_at` | Standard timestamp naming |

### Terminology Consistency Rules

#### 1. Always Use nodeType (Never Bare "type" for Nodes)

```typescript
// ✅ Correct - Unambiguous
interface Node {
  nodeType: string;  // Clear: this is the type of node
}

if (node.nodeType === 'task') {
  // Handle task node
}

// ❌ Incorrect - Ambiguous
interface Node {
  type: string;  // Unclear - type of what?
}

// ❌ Deprecated pattern (exists in legacy code)
const { type } = node;  // Ambiguous - avoid in new code
```

**Rationale**:
- Avoids collision with TypeScript's `type` keyword
- Disambiguates from schema field `type`
- Makes code more readable and searchable

#### 2. Schema Field "type" is Acceptable

```typescript
// ✅ Correct - Context makes it clear
interface SchemaField {
  name: string;
  type: string;  // Field type - context is schema field
}

// Schema field context is always clear
if (field.type === 'enum') {
  // Render dropdown
}
```

**Rationale**: Within schema context, "type" is unambiguous and clearer than "fieldType".

#### 3. Reference Naming - Always Suffix with Id

```typescript
// ✅ Correct - Clear these are IDs
parentId: string;
containerId: string;
beforeSiblingId: string;
mentionedById: string[];

// ❌ Incorrect - Ambiguous
parent: string;     // Is this an ID or a Node object?
container: string;  // Unclear
sibling: string;    // Unclear
```

#### 4. Avoid Redundant Prefixes

```typescript
// ✅ Correct - Context already provides meaning
interface SchemaField {
  type: string;       // Not "fieldType" - context is field
  name: string;       // Not "fieldName" - context is field
  protection: string; // Not "fieldProtection"
}

// ❌ Incorrect - Redundant
interface SchemaField {
  fieldType: string;       // Redundant - we're in a field
  fieldName: string;       // Redundant
  fieldProtection: string; // Redundant
}
```

**Exception**: When field is used outside its natural context:

```typescript
// ✅ Acceptable - Outside schema context
function getFieldType(field: SchemaField): string {
  return field.type;  // Qualification needed in function name
}
```

## Abbreviations and Acronyms

### Standard Abbreviations

```typescript
// ✅ Correct
id, nodeId, schemaId     // "id" is universally understood
db, dbPath               // "db" for database
nlp, nlpEngine           // "nlp" for natural language processing
mcp, mcpServer           // "mcp" for Model Context Protocol
api, apiUrl              // "api" for application programming interface
url, baseUrl             // "url" not "Url" in camelCase
html, htmlContent        // "html" not "Html"

// ❌ Incorrect
identifier               // Too verbose - use "id"
database                 // Use "db" for variables/functions
naturalLanguageProcessor // Use "nlp"
```

### Acronym Casing

```typescript
// ✅ Correct - First letter only capitalized in PascalCase
class HttpAdapter { }
class ApiClient { }
class HtmlParser { }
type UrlPattern = string;

// ❌ Incorrect
class HTTPAdapter { }  // Should be HttpAdapter
class APIClient { }    // Should be ApiClient
class HTMLParser { }   // Should be HtmlParser
type URLPattern = string;  // Should be UrlPattern
```

## Examples by Use Case

### Creating a New Node

```typescript
// ✅ TypeScript - All camelCase
const nodeId = crypto.randomUUID();
const nodeType = 'task';
const parentId = currentContainer.id;
const createdAt = new Date().toISOString();

await createNode({
  id: nodeId,
  nodeType,
  parentId,
  content: '',
  properties: {}
});
```

```rust
// ✅ Rust - All snake_case
let node_id = Uuid::new_v4().to_string();
let node_type = "task".to_string();
let parent_id = Some(current_container.id.clone());
let created_at = Utc::now().to_rfc3339();

node_service.create_node(Node {
    id: node_id,
    node_type,
    parent_id,
    content: String::new(),
    properties: serde_json::Value::Object(Map::new()),
    ..Default::default()
}).await?;
```

### Schema Field Operations

```typescript
// ✅ TypeScript
const field: SchemaField = {
  name: 'priority',
  type: 'number',  // Schema field type (not nodeType!)
  protection: 'user',
  indexed: false,
  required: false
};
```

```rust
// ✅ Rust
let field = SchemaField {
    name: "priority".to_string(),
    field_type: "number".to_string(),  // Will serialize to "type" in JSON
    protection: ProtectionLevel::User,
    indexed: false,
    required: Some(false),
    ..Default::default()
};
```

## Migration and Legacy Code

### Current State

**Well-Standardized**:
- ✅ Rust code follows snake_case/PascalCase consistently
- ✅ TypeScript largely follows camelCase/PascalCase
- ✅ Database uses snake_case

**Needs Improvement**:
- ⚠️ Some legacy code uses `type` instead of `nodeType` for nodes
- ⚠️ Inconsistent qualification (e.g., `type` vs `nodeType` vs `node.type`)

### Migration Rules

**New Code** (MANDATORY):
- ✅ Use `nodeType` for node categories (never bare `type`)
- ✅ Use `type` for schema field categories (context is clear)
- ✅ Follow all case conventions for the language

**Legacy Code** (TECHNICAL DEBT):
- ⏸️ Accept existing `type` usage in old files
- ⏸️ Fix during major refactoring only
- ⏸️ Don't rename during small bug fixes

**When to Fix**:
- ✅ When refactoring a module completely
- ✅ When the ambiguity causes actual bugs
- ✅ When touching >50% of a file's lines

## Enforcement

### ESLint Rules

**Custom Rules** (as of Issue #507):
- ✅ **nodeType Enforcement**: Custom ESLint rule warns when Node interfaces use `type` instead of `nodeType`
- ✅ **Filename Convention**: Enforces kebab-case for all TypeScript and Svelte files
- ✅ **Unused Variables**: Warns about unused variables (allows underscore prefix for intentionally unused)

**Standard Rules**:
- ✅ **camelCase variables**: Enforced by TypeScript and oxlint
- ✅ **PascalCase interfaces**: Enforced by TypeScript conventions
- ✅ **No explicit any**: Warns about unsafe `any` types

### Quality Gates

```bash
# MANDATORY before PR:
bun run quality:fix

# Checks:
# ✅ ESLint - catches some naming issues
# ✅ TypeScript - catches type mismatches from naming errors
# ✅ Prettier - enforces consistent formatting
```

### Code Review Checklist

Reviewers should verify:
- [ ] Variables use camelCase (TS) or snake_case (Rust)
- [ ] Types/Interfaces use PascalCase
- [ ] Constants use SCREAMING_SNAKE_CASE (when appropriate)
- [ ] **Node interfaces use `nodeType` not `type`** (enforced by ESLint rule `nodespace/enforce-nodetype`)
- [ ] Schema field interfaces use `type` not `fieldType` (acceptable in schema context)
- [ ] IDs always suffixed with `Id` / `_id`
- [ ] Serde rename attributes match TypeScript expectations
- [ ] No ESLint `nodespace/enforce-nodetype` warnings in PR

## Quick Reference

### TypeScript
```typescript
const myVariable = value;           // camelCase
const MY_CONSTANT = 42;            // SCREAMING_SNAKE_CASE (config)
function doSomething() { }          // camelCase
interface MyType { }                // PascalCase
type MyUnion = A | B;              // PascalCase
enum Status { OPEN, DONE }         // PascalCase, SCREAMING values
```

### Rust
```rust
let my_variable = value;           // snake_case
const MY_CONSTANT: u32 = 42;       // SCREAMING_SNAKE_CASE
fn do_something() { }              // snake_case
struct MyType { }                  // PascalCase
enum Status { Open, Done }         // PascalCase, PascalCase values
pub field_name: String             // snake_case
```

### Database/JSON
```
snake_case for storage             // node_type, created_at
camelCase in JSON (via serde)      // nodeType, createdAt
```

---

**Document Version**: 1.0
**Last Updated**: November 2025
**Next Review**: January 2026
**Owner**: Development Team

**Related Documents**:
- [File Naming Conventions](file-naming-conventions.md) - File name standards
- [Code Quality Standards](code-quality.md) - Quality gates and linting policy
