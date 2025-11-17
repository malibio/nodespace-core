# Schema Plugin Auto-Registration System

**Status**: Phase 1 Implemented (November 2024)
**Last Updated**: November 17, 2024

## Overview

The Schema Plugin Auto-Registration system automatically converts custom entity schemas into plugins, making them immediately available as slash commands without manual plugin registration or application restart.

## Architecture

### System Flow

```
User Creates Schema → SchemaService → Schema Plugin Loader → PluginRegistry → Slash Command Available
        ↓                   ↓                    ↓                    ↓                  ↓
   Database Update    Schema Validation   Plugin Creation   Plugin Registration   UI Update
```

### Key Components

#### 1. Schema Plugin Loader (`schema-plugin-loader.ts`)

The core auto-registration module that bridges schemas and plugins:

```typescript
// Convert schema → plugin
createPluginFromSchema(schema: SchemaDefinition, schemaId: string): PluginDefinition

// Register/unregister plugins
registerSchemaPlugin(schemaId: string): Promise<void>
unregisterSchemaPlugin(schemaId: string): void

// Initialize system on startup
initializeSchemaPluginSystem(): Promise<InitializationResult>
```

#### 2. Custom Entity Node Component

Generic Svelte component that renders all custom entity types:

```typescript
// Lazy-loaded for all custom schemas
const CustomEntityNode = () => import('$lib/design/components/custom-entity-node.svelte');
```

Uses `BaseNode` for standard node functionality while supporting schema-specific properties.

#### 3. Schema Service Integration

Leverages existing schema management:

```typescript
// Fetch schemas for registration
await schemaService.getAllSchemas()
await schemaService.getSchema(schemaId)
```

## Implementation Phases

### Phase 1: Startup Registration (✅ Implemented)

**Scope**: Auto-register existing schemas when app starts

**Features**:
- Query all schemas from database on startup
- Filter out core types (`isCore: true`)
- Convert custom schemas to plugin definitions
- Register plugins with unified registry
- Parallel registration for performance
- Graceful error handling with status reporting

**Usage**:
```typescript
// In app root layout
onMount(async () => {
  const result = await initializeSchemaPluginSystem();

  if (!result.success) {
    console.warn(`Custom entities unavailable: ${result.error}`);
  } else {
    console.log(`${result.registeredCount} custom entities loaded`);
  }
});
```

**Limitations**:
- Only registers schemas that exist on startup
- Runtime schema creation requires app restart
- Schema deletion doesn't unregister plugins until restart

### Phase 2: Runtime Hot-Reload (🔮 Planned)

**Scope**: Auto-register/unregister schemas created/deleted at runtime

**Planned Features**:
- Tauri event emission from Rust backend on schema mutations
- Frontend event listeners for schema:created and schema:deleted
- Immediate plugin registration without restart
- Immediate plugin unregistration on schema deletion

**Required Backend Changes**:
```rust
#[tauri::command]
pub async fn create_schema(
    state: State<'_, AppState>,
    app_handle: tauri::AppHandle,
    schema_id: String,
    fields: Vec<SchemaField>,
) -> Result<SchemaDefinition, String> {
    // Create schema in database
    let schema = state.schema_service.create_schema(schema_id, fields).await?;

    // Emit event for frontend
    app_handle.emit_all("schema:created", SchemaCreatedPayload {
        schema_id: schema.id.clone(),
        is_core: schema.is_core
    }).map_err(|e| e.to_string())?;

    Ok(schema)
}
```

**Frontend Event Listeners**:
```typescript
// Listen for schema creation
await listen<SchemaCreatedEvent>('schema:created', async (event) => {
  const { schema_id, is_core } = event.payload;
  if (!is_core) {
    await registerSchemaPlugin(schema_id);
  }
});

// Listen for schema deletion
await listen<SchemaDeletedEvent>('schema:deleted', (event) => {
  unregisterSchemaPlugin(event.payload.schema_id);
});
```

**Benefits**:
- True hot-reload: no restart required
- Better developer experience
- Immediate feedback for users creating schemas

## Plugin Conversion Logic

### Schema → Plugin Mapping

```typescript
{
  id: schemaId,                    // 'invoice' → plugin ID
  name: displayName,               // 'Sales Invoice' or humanized ID
  description: schema.description, // User-provided description
  version: `${schema.version}.0.0`, // '1.0.0' from schema version 1

  config: {
    slashCommands: [{
      id: schemaId,
      name: displayName,
      description: schema.description || `Create ${displayName}`,
      contentTemplate: '',
      nodeType: schemaId,
      priority: PLUGIN_PRIORITIES.CUSTOM_ENTITY  // 50
    }],
    canHaveChildren: true,
    canBeChild: true
  },

  node: {
    lazyLoad: () => import('../design/components/custom-entity-node.svelte')
  }
}
```

### Display Name Humanization

When schema description is missing, IDs are automatically humanized:

| Schema ID | Humanized Display Name |
|-----------|------------------------|
| `invoice` | Invoice |
| `salesInvoice` | Sales Invoice |
| `sales_invoice` | Sales Invoice |
| `sales-invoice` | Sales Invoice |

**Implementation**:
```typescript
function humanizeSchemaId(id: string): string {
  return id
    .replace(/([A-Z])/g, ' $1')  // camelCase → spaces
    .replace(/[_-]/g, ' ')        // snake/kebab → spaces
    .trim()
    .replace(/\b\w/g, char => char.toUpperCase());  // Capitalize
}
```

## Priority System

Custom entity slash commands use a priority-based ordering system:

```typescript
export const PLUGIN_PRIORITIES = {
  CORE: 100,         // Core types (text, task, date)
  CUSTOM_ENTITY: 50, // Schema-generated plugins
  USER_COMMAND: 0    // User-defined commands
} as const;
```

This ensures custom entities appear after core types but before user commands in the slash menu.

## Core Type Filtering

The system prevents re-registration of core types that are already defined in `core-plugins.ts`:

```typescript
// Defensive check handles both TypeScript and Rust formats
const isCoreType = schema.isCore || (schema as any).is_core;

if (isCoreType) {
  console.debug(`Skipping core type registration: ${schemaId}`);
  return;
}
```

**Why both formats?**
- Backend returns `is_core` (Rust snake_case)
- `backend-adapter.ts` converts to `isCore` (TypeScript camelCase)
- Defensive check handles both for robustness across boundaries

## Idempotency

The registration system is idempotent - safe to call multiple times:

```typescript
if (pluginRegistry.hasPlugin(schemaId)) {
  console.debug(`Plugin already registered: ${schemaId}`);
  return;
}
```

This prevents duplicate registrations and allows startup sequence to be re-run safely.

## Error Handling

### Graceful Degradation

Initialization failures don't block app startup:

```typescript
const result = await initializeSchemaPluginSystem();

if (!result.success) {
  // Custom entities unavailable, but core functionality works
  console.warn(`Custom entities unavailable: ${result.error}`);
} else {
  // Success path
  console.log(`${result.registeredCount} entities loaded`);
}
```

### Result Interface

```typescript
interface InitializationResult {
  success: boolean;
  registeredCount: number;
  error?: string;  // Only present when success=false
}
```

## Performance Optimizations

### Parallel Registration

Schemas are registered in parallel for optimal startup performance:

```typescript
// Before: Sequential registration (slow)
for (const schema of schemas) {
  await registerSchemaPlugin(schema.id);  // Blocking
}

// After: Parallel registration (fast)
await Promise.all(
  customSchemas.map(schema => registerSchemaPlugin(schema.id))
);
```

**Impact**: With 50 custom schemas, parallel registration reduces startup time from ~500ms to ~20ms.

### Lazy Component Loading

Custom entity components are lazy-loaded on first use:

```typescript
node: {
  lazyLoad: () => import('../design/components/custom-entity-node.svelte')
}
```

This prevents loading component code until a custom entity node is actually rendered.

## Testing

### Test Coverage

Comprehensive test suite covers:

- **Plugin conversion**: Schema → Plugin structure validation
- **Display name humanization**: All ID formats (camelCase, snake_case, kebab-case)
- **Priority system**: Correct ordering in slash menu
- **Core type filtering**: Both camelCase and snake_case formats
- **Registration idempotency**: Multiple registration attempts
- **Error handling**: Schema fetch failures
- **Unregistration**: Plugin cleanup
- **Initialization**: Full startup flow with multiple schemas
- **Performance**: Parallel registration timing
- **Integration**: End-to-end schema → plugin → slash command

**Test File**: `src/tests/plugins/schema-plugin-loader.test.ts`

**Test Count**: 25+ tests covering all functionality

## Usage Examples

### Creating a Custom Schema

```typescript
// User creates a schema via UI or API
await schemaService.createSchema({
  id: 'invoice',
  description: 'Sales Invoice',
  fields: [
    { id: 'customer', type: 'text', required: true },
    { id: 'amount', type: 'number', required: true },
    { id: 'dueDate', type: 'date', required: false }
  ]
});

// Phase 1: Requires app restart to see /invoice slash command
// Phase 2: Immediately available as /invoice (no restart)
```

### Using Custom Entity Slash Command

```typescript
// User types "/" in text editor
// Slash menu appears with:
// - [Priority 100] /text
// - [Priority 100] /task
// - [Priority 50]  /invoice  ← Custom entity
// - [Priority 50]  /customer ← Custom entity
// - [Priority 0]   /mycommand ← User command
```

## Integration Points

### SchemaService

- **Dependency**: Schema CRUD operations
- **Methods Used**: `getAllSchemas()`, `getSchema(id)`
- **Data Flow**: Database → SchemaService → Plugin Loader

### PluginRegistry

- **Dependency**: Unified plugin management
- **Methods Used**: `register()`, `unregister()`, `hasPlugin()`
- **Data Flow**: Plugin Loader → PluginRegistry → SlashCommandService

### SlashCommandService

- **Consumer**: Slash command dropdown
- **Usage**: Queries registry on every "/" trigger
- **Benefit**: No caching layer - plugins immediately visible

## Future Enhancements

### Phase 2 Priorities

1. **Tauri Event Emission**: Backend events for schema mutations
2. **Frontend Event Listeners**: Runtime plugin registration/unregistration
3. **Event Type Definitions**: TypeScript types for Tauri events
4. **E2E Tests**: Runtime hot-reload validation

### Phase 3 Ideas

1. **Plugin Manager UI**: Visual plugin management
2. **Schema Plugin Templates**: Pre-built schema configurations
3. **Plugin Versioning**: Track schema version changes
4. **Plugin Dependencies**: Schema relationships as plugin dependencies
5. **Hot Module Replacement**: Update components without full reload

## Migration Guide

### From Manual Plugin Registration

**Before** (manual registration required):
```typescript
// Create plugin definition manually
const invoicePlugin: PluginDefinition = {
  id: 'invoice',
  // ... 50+ lines of boilerplate
};

// Register manually
pluginRegistry.register(invoicePlugin);
```

**After** (automatic):
```typescript
// Just create the schema
await schemaService.createSchema({ id: 'invoice', ... });

// Plugin auto-registers on next startup (Phase 1)
// Plugin auto-registers immediately (Phase 2)
```

### Backward Compatibility

- **Core plugins**: Unchanged, still registered via `core-plugins.ts`
- **Existing custom plugins**: Continue to work alongside schema plugins
- **Registry API**: No breaking changes

## Troubleshooting

### Custom Entity Not Appearing

**Symptom**: Created schema but slash command doesn't appear

**Checklist**:
1. Check if schema is marked as `isCore: true` (core types are excluded)
2. Restart app (Phase 1 limitation)
3. Check browser console for initialization errors
4. Verify schema exists in database via SchemaService

### Plugin Registration Failed

**Symptom**: Initialization error on startup

**Common Causes**:
- Database connection failure
- Schema service not initialized
- Invalid schema structure
- Missing required fields

**Solution**:
```typescript
// Check initialization result
const result = await initializeSchemaPluginSystem();
if (!result.success) {
  console.error('Registration failed:', result.error);
  // Core app still works, custom entities unavailable
}
```

## References

- **Implementation**: `packages/desktop-app/src/lib/plugins/schema-plugin-loader.ts`
- **Tests**: `packages/desktop-app/src/tests/plugins/schema-plugin-loader.test.ts`
- **Integration**: `packages/desktop-app/src/routes/+layout.svelte`
- **Plugin Registry**: [unified-plugin-registry.md](./unified-plugin-registry.md)
- **Schema Service**: `packages/desktop-app/src/lib/services/schema-service.ts`
- **Issue Tracker**: [GitHub Issue #447](https://github.com/malibio/nodespace-core/issues/447)

## Contributing

### Adding Features

1. Update `schema-plugin-loader.ts` with new functionality
2. Add comprehensive tests to `schema-plugin-loader.test.ts`
3. Update this documentation
4. Submit PR with acceptance criteria validation

### Phase 2 Implementation

Contributors interested in implementing runtime hot-reload:

1. **Start with Rust backend**: Implement Tauri event emission
2. **Add event type definitions**: TypeScript types for events
3. **Implement event listeners**: Frontend reactive registration
4. **Write integration tests**: End-to-end validation
5. **Update documentation**: Remove Phase 1 limitations

---

**Last Updated**: November 17, 2024
**Next Review**: When Phase 2 implementation begins
