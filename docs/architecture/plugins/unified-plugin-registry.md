# Unified Plugin Registry System

**Status**: Implemented (December 2024)
**Last Updated**: December 17, 2024

## Overview

The NodeSpace plugin system has been consolidated from multiple fragmented registries into a unified system that provides consistent plugin management across the entire application.

## Architecture Changes

### Before: Fragmented System

Previously, NodeSpace had multiple independent plugin registries:

- **ViewerRegistry**: Managed node viewer components in `/lib/components/viewers/`
- **NODE_REFERENCE_COMPONENTS**: Managed reference components in `/lib/components/references/`
- **BasicNodeTypeRegistry**: Managed slash commands and node type definitions in `/lib/registry/`

Each system operated independently, leading to:
- Duplicate plugin management logic
- Inconsistent APIs for plugin functionality
- No unified lifecycle management
- Difficult to track plugin state across systems

### After: Unified Registry

The new system consolidates all plugin functionality into a single `PluginRegistry` class:

```typescript
// Single unified registry for all plugin functionality
export class PluginRegistry {
  private plugins = new Map<string, PluginDefinition>();
  private loadedViewers = new Map<string, NodeViewerComponent>();

  // Plugin lifecycle management
  register(plugin: PluginDefinition): void
  unregister(pluginId: string): void
  setEnabled(pluginId: string, enabled: boolean): void

  // Component resolution
  async getViewer(nodeType: string): Promise<NodeViewerComponent | null>
  getReferenceComponent(nodeType: string): NodeReferenceComponent | null

  // Slash command management
  getAllSlashCommands(): SlashCommandDefinition[]
  findSlashCommand(commandId: string): SlashCommandDefinition | null
  filterSlashCommands(query: string): SlashCommandDefinition[]

  // Statistics and introspection
  getStats(): PluginRegistryStats
  hasPlugin(pluginId: string): boolean
  isEnabled(pluginId: string): boolean
}
```

## Plugin Definition Interface

### Core Plugin Structure

```typescript
export interface PluginDefinition {
  // Plugin metadata
  id: string;
  name: string;
  description: string;
  version: string;

  // Node type configuration
  config: {
    slashCommands: SlashCommandDefinition[];
    canHaveChildren?: boolean;
    canBeChild?: boolean;
  };

  // Component registrations
  viewer?: {
    component?: NodeViewerComponent;
    lazyLoad?: () => Promise<{ default: NodeViewerComponent }>;
    priority?: number;
  };

  reference?: {
    component: NodeReferenceComponent;
    priority?: number;
  };
}
```

### Slash Command Definition

```typescript
export interface SlashCommandDefinition {
  id: string;
  name: string;
  description: string;
  contentTemplate: string;
  shortcut?: string;
  priority?: number;
}
```

## Core Plugins

### Current Core Plugin Set

```typescript
export const corePlugins = [
  textNodePlugin,        // Rich text with headers (# ## ###)
  taskNodePlugin,        // Task management ([ ])
  aiChatNodePlugin,      // AI conversations (⌘+k)
  dateNodePlugin,        // Date/time references
  userNodePlugin,        // User references (reference-only)
  documentNodePlugin     // Document references (reference-only)
];
```

### Text Node Plugin

The most comprehensive core plugin with multiple slash commands:

```typescript
export const textNodePlugin: PluginDefinition = {
  id: 'text',
  name: 'Text Node',
  description: 'Rich text editing with markdown support',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'text',
        name: 'Text',
        description: 'Create a text node',
        contentTemplate: ''
      },
      {
        id: 'header1',
        name: 'Header 1',
        description: 'Create a level 1 header',
        contentTemplate: '# ',
        shortcut: '#'
      },
      {
        id: 'header2',
        name: 'Header 2',
        description: 'Create a level 2 header',
        contentTemplate: '## ',
        shortcut: '##'
      },
      {
        id: 'header3',
        name: 'Header 3',
        description: 'Create a level 3 header',
        contentTemplate: '### ',
        shortcut: '###'
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },
  viewer: {
    lazyLoad: () => import('$lib/components/viewers/TextNodeViewer.svelte'),
    priority: 1
  },
  reference: {
    component: TextNodeReference,
    priority: 1
  }
};
```

### Task Node Plugin

```typescript
export const taskNodePlugin: PluginDefinition = {
  id: 'task',
  name: 'Task Node',
  description: 'Task management with completion tracking',
  version: '1.0.0',
  config: {
    slashCommands: [
      {
        id: 'task',
        name: 'Task',
        description: 'Create a task with checkbox',
        contentTemplate: '- [ ] ',
        shortcut: '[ ]'
      }
    ],
    canHaveChildren: true,
    canBeChild: true
  },
  viewer: {
    lazyLoad: () => import('$lib/components/viewers/TaskNodeViewer.svelte'),
    priority: 1
  },
  reference: {
    component: TaskNodeReference,
    priority: 1
  }
};
```

### Reference-Only Plugins

Some plugins only provide reference components without viewers:

```typescript
// User reference plugin (no viewer, reference-only)
export const userNodePlugin: PluginDefinition = {
  id: 'user',
  name: 'User Reference',
  description: 'Reference to users in the system',
  version: '1.0.0',
  config: {
    slashCommands: [], // No slash commands
    canHaveChildren: false,
    canBeChild: true
  },
  // No viewer component
  reference: {
    component: UserReference,
    priority: 1
  }
};
```

## System Integration

### Registration and Initialization

```typescript
// Core plugin registration in app initialization
// packages/desktop-app/src/lib/components/layout/app-shell.svelte

import { pluginRegistry } from '$lib/plugins/index';
import { registerCorePlugins } from '$lib/plugins/corePlugins';

// Initialize core plugins
registerCorePlugins(pluginRegistry);

// Now all plugin functionality is available through unified registry
const allCommands = pluginRegistry.getAllSlashCommands();
const textViewer = await pluginRegistry.getViewer('text');
const taskReference = pluginRegistry.getReferenceComponent('task');
```

### Component Hydration Integration

The unified registry integrates with the component hydration system:

```typescript
// Component hydration uses plugin registry for component resolution
// packages/desktop-app/src/lib/services/componentHydrationSystem.ts

class ComponentHydrationSystem {
  private resolveComponent(componentName: string, nodeType: string): ComponentClass {
    // Try unified plugin registry first
    const pluginComponent = pluginRegistry.getReferenceComponent(nodeType);
    if (pluginComponent) {
      return pluginComponent;
    }

    // Fall back to legacy system (maintains compatibility)
    return getNodeReferenceComponent(nodeType);
  }
}
```

### SlashCommandService Integration

The slash command service integrates with the unified registry while maintaining backward compatibility:

```typescript
// packages/desktop-app/src/lib/services/slashCommandService.ts

class SlashCommandService {
  getCommands(): SlashCommand[] {
    // Get hardcoded commands (for reliability)
    const hardcodedCommands = this.getHardcodedCommands();

    // Add registry commands if available
    try {
      const registryCommands = pluginRegistry.getAllSlashCommands()
        .map(cmd => this.convertToSlashCommand(cmd));

      // Merge and deduplicate
      return this.mergeCommands(hardcodedCommands, registryCommands);
    } catch (error) {
      console.warn('Registry integration failed, using hardcoded commands:', error);
      return hardcodedCommands;
    }
  }
}
```

## Performance Characteristics

### Lazy Loading

Viewer components are loaded on-demand for optimal performance:

```typescript
// Lazy loading with caching
async getViewer(nodeType: string): Promise<NodeViewerComponent | null> {
  // Check cache first
  if (this.loadedViewers.has(nodeType)) {
    return this.loadedViewers.get(nodeType)!;
  }

  const plugin = this.plugins.get(nodeType);
  if (!plugin?.viewer) return null;

  if (plugin.viewer.component) {
    // Direct component
    this.loadedViewers.set(nodeType, plugin.viewer.component);
    return plugin.viewer.component;
  } else if (plugin.viewer.lazyLoad) {
    // Lazy load with caching
    try {
      const module = await plugin.viewer.lazyLoad();
      this.loadedViewers.set(nodeType, module.default);
      return module.default;
    } catch (error) {
      console.error(`Failed to lazy load viewer for ${nodeType}:`, error);
      return null;
    }
  }

  return null;
}
```

### Performance Metrics

Current system metrics:

```typescript
// Plugin registry statistics
const stats = pluginRegistry.getStats();
// {
//   pluginsCount: 6,
//   viewersCount: 4,      // text, task, ai-chat, date
//   referencesCount: 6,   // all plugins have references
//   slashCommandsCount: 7, // total slash commands
//   plugins: ['text', 'task', 'ai-chat', 'date', 'user', 'document']
// }
```

## Testing Strategy

### Comprehensive Test Coverage

The unified system includes extensive testing:

- **33 Plugin Registry Tests**: Core functionality, lifecycle, error handling
- **25 Core Plugin Tests**: Plugin definitions, integration, component resolution
- **Integration Tests**: SlashCommandService integration, component hydration
- **Backward Compatibility Tests**: Ensuring legacy systems continue to work

```typescript
// Example test structure
describe('PluginRegistry - Unified Plugin System', () => {
  it('should register a complete plugin successfully', () => {
    registry.register(testPlugin);
    expect(registry.hasPlugin('test-plugin')).toBe(true);
    expect(registry.isEnabled('test-plugin')).toBe(true);
  });

  it('should lazy load viewer component', async () => {
    const viewer = await registry.getViewer('lazy-plugin');
    expect(mockLazyImport).toHaveBeenCalled();
    expect(viewer).toBe(MockViewerComponent);
  });

  it('should cache lazy loaded components', async () => {
    await registry.getViewer('lazy-plugin');
    await registry.getViewer('lazy-plugin');
    // Should only call lazy load once
    expect(mockLazyImport).toHaveBeenCalledTimes(1);
  });
});
```

## Error Handling

### Graceful Degradation

The system handles errors gracefully without breaking functionality:

```typescript
// Error handling in plugin operations
async getViewer(nodeType: string): Promise<NodeViewerComponent | null> {
  try {
    const viewer = await this.loadViewerWithFallback(nodeType);
    return viewer;
  } catch (error) {
    console.error(`Failed to load viewer for ${nodeType}:`, error);
    return null; // Graceful degradation - UI continues to work
  }
}

private async loadViewerWithFallback(nodeType: string): Promise<NodeViewerComponent | null> {
  const plugin = this.plugins.get(nodeType);
  if (!plugin?.viewer?.lazyLoad) return null;

  try {
    const module = await plugin.viewer.lazyLoad();
    return module.default;
  } catch (error) {
    console.warn(`Lazy loading failed for ${nodeType}, returning null`);
    return null; // Component hydration will handle fallback rendering
  }
}
```

## Backward Compatibility

### Legacy System Support

The unified registry maintains compatibility with existing systems through forwarding layers:

```typescript
// Legacy ViewerRegistry compatibility
// packages/desktop-app/src/lib/components/viewers/index.ts

class ViewerRegistry {
  async getViewerComponent(nodeType: string): Promise<ComponentConstructor | null> {
    // Forward to unified registry
    const viewer = await pluginRegistry.getViewer(nodeType);
    return viewer as ComponentConstructor | null;
  }

  hasViewer(nodeType: string): boolean {
    return pluginRegistry.hasViewer(nodeType);
  }
}

// Legacy NODE_REFERENCE_COMPONENTS compatibility
// packages/desktop-app/src/lib/components/references/index.ts

export const NODE_REFERENCE_COMPONENTS = new Proxy({}, {
  get(target, nodeType: string) {
    return pluginRegistry.getReferenceComponent(nodeType);
  },

  has(target, nodeType: string) {
    return pluginRegistry.hasReferenceComponent(nodeType);
  }
});
```

## Migration Benefits

### Consolidation Results

The migration from fragmented to unified system provided:

1. **Single Source of Truth**: One registry for all plugin functionality
2. **Consistent API**: Unified interface for plugin operations
3. **Lifecycle Management**: Enable/disable plugins at runtime
4. **Better Testing**: Comprehensive test coverage with clear boundaries
5. **Performance**: Lazy loading and caching optimizations
6. **Type Safety**: Full TypeScript type checking across plugin boundaries
7. **Maintainability**: Single codebase to maintain and enhance

### Quality Improvements

- **499 Total Tests Passing**: Comprehensive test coverage
- **Zero TypeScript Errors**: Full type safety
- **Zero Lint Warnings**: Clean, consistent codebase
- **Backward Compatibility**: No breaking changes for existing functionality

## Future Roadmap

The unified registry system serves as the foundation for planned enhancements:

1. **Plugin Manager**: Developer tools for plugin creation and validation
2. **Runtime Loading**: Support for external plugins loaded at runtime
3. **Extended Extension Points**: Backend, AI, and workflow plugin support
4. **Plugin Marketplace**: Distribution and discovery system for external plugins

The current architecture is designed to support these future enhancements without requiring major architectural changes.