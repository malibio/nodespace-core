# Vitest Module Duplication Fix: Plugin Registry Singleton Issue

**Date:** 2025-10-12
**Issue:** Slash command integration tests failing due to plugin registry module duplication
**Status:** RESOLVED

## Table of Contents

- [Problem Statement](#problem-statement)
  - [Root Cause](#root-cause)
  - [Evidence](#evidence)
- [Why GlobalThis Singleton Pattern Failed](#why-globalthis-singleton-pattern-failed)
- [Solution: Use Setup Files Instead of Global Setup](#solution-use-setup-files-instead-of-global-setup)
  - [Implementation](#implementation)
  - [Configuration](#configuration)
- [When to Use Each Setup Type](#when-to-use-each-setup-type)
  - [Global Setup (Node Context)](#global-setup-node-context)
  - [Setup Files (Test Context)](#setup-files-test-context)
- [Results](#results)
- [Architectural Implications](#architectural-implications)
  - [Current Architecture (Acceptable)](#current-architecture-acceptable)
  - [Recommended Future Architecture](#recommended-future-architecture)
- [Testing Strategy Guidelines](#testing-strategy-guidelines)
- [Related Issues](#related-issues)
- [Key Takeaways](#key-takeaways)
- [Files Modified](#files-modified)
- [Test Results](#test-results)

## Problem Statement

Integration tests were failing with "No commands available" in the slash command dropdown, even though 6 plugins were registered during global test setup. This gave false confidence that tests were passing while production code had regression bugs.

### Root Cause

**Module duplication in Vitest between Node context and Happy-DOM browser context.**

Vitest creates separate module graphs for:
1. **Global Setup (Node context)** - Runs in Node.js environment with access to filesystem, process, etc.
2. **Test Files & Components (Happy-DOM context)** - Runs in browser-like environment with DOM APIs

Even with a `globalThis` singleton pattern, this results in **TWO instances of PluginRegistry**:
- Instance 1: Created in global setup, registered 6 plugins
- Instance 2: Created when components import the registry, has 0 plugins

### Evidence

```
[PluginRegistry constructor] Creating NEW instance: bqnz1y  // Global setup
[PluginRegistry.getAllSlashCommands] instance: bqnz1y, plugins: 6 enabled: 6

[PluginRegistry constructor] Creating NEW instance: 22ocpf  // Tests
[PluginRegistry.getAllSlashCommands] instance: 22ocpf, plugins: 0 enabled: 0
```

## Why GlobalThis Singleton Pattern Failed

The `globalThis` singleton approach worked in theory but failed in practice:

```typescript
// plugin-registry.ts (DIDN'T WORK)
const globalKey = '__nodespace_plugin_registry__';
if (!(globalThis as any)[globalKey]) {
  (globalThis as any)[globalKey] = new PluginRegistry();
}
export const pluginRegistry = (globalThis as any)[globalKey];
```

**Why it failed:**
- Vite creates separate `globalThis` scopes for Node vs Happy-DOM contexts
- Each context gets its own module graph
- The check `!(globalThis as any)[globalKey]` returns `false` in both contexts independently
- Result: Two instances created

## Solution: Use Setup Files Instead of Global Setup

**Key Insight:** Setup files run in the **same context** as test files and components.

### Implementation

**Before (BROKEN):**
```typescript
// global-setup.ts (Node context)
export default async function setup() {
  const { pluginRegistry } = await import('$lib/plugins/index');
  const { registerCorePlugins } = await import('$lib/plugins/core-plugins');

  // This creates instance #1 in Node context
  registerCorePlugins(pluginRegistry);
}
```

**After (WORKING):**
```typescript
// setup.ts (Happy-DOM context - same as components)
import { pluginRegistry } from '$lib/plugins/index';
import { registerCorePlugins } from '$lib/plugins/core-plugins';

// Register at module load time - same context as components
if (!pluginRegistry.hasPlugin('text')) {
  registerCorePlugins(pluginRegistry);
  console.log('✅ Core plugins registered in test setup (browser context)');
}
```

### Configuration

**vitest.config.ts:**
```typescript
export default defineConfig({
  test: {
    // ❌ Global setup runs in Node context (separate module graph)
    globalSetup: 'src/tests/global-setup.ts',

    // ✅ Setup files run in Happy-DOM context (same module graph as tests)
    setupFiles: ['src/tests/setup-svelte-mocks.ts', 'src/tests/setup.ts']
  }
});
```

## When to Use Each Setup Type

### Global Setup (Node Context)
**Use for:**
- Starting/stopping external services (dev servers, databases)
- Environment validation (check for correct test command)
- One-time initialization that doesn't depend on module state
- System-level configuration

**Avoid for:**
- Registering singletons that tests will import
- Setting up shared state that components access
- Initializing services used by Svelte components

### Setup Files (Test Context)
**Use for:**
- Registering singletons (plugin registries, service containers)
- Setting up shared state for components
- Mocking browser APIs (MutationObserver, IntersectionObserver)
- Test utilities and helpers
- Anything that needs to share module instances with components

## Results

### Before Fix
```
✗ 5 slash command integration tests failing
✗ Dropdown showing "No commands available"
✗ Two PluginRegistry instances (bqnz1y with 6 plugins, 22ocpf with 0 plugins)
```

### After Fix
```
✓ All 59 slash command tests passing
✓ Single PluginRegistry instance across all tests
✓ Components access same instance with 6 registered plugins
✓ Real integration testing without mocks
```

## Architectural Implications

### Current Architecture (Acceptable)
- Singleton pattern with setup file registration
- Works for current needs
- Easy to maintain

### Recommended Future Architecture

For better testability and maintainability, consider migrating to:

#### 1. Service Container Pattern
```typescript
export class ServiceContainer {
  constructor(
    public pluginRegistry: PluginRegistry,
    public slashCommands: SlashCommandService,
    public persistence: PersistenceCoordinator
  ) {}

  static initialize() {
    const registry = new PluginRegistry();
    registerCorePlugins(registry);

    return new ServiceContainer(
      registry,
      new SlashCommandService(registry),
      PersistenceCoordinator.getInstance()
    );
  }
}
```

**Benefits:**
- Single initialization point
- Clear service dependencies
- Easy to mock entire container

#### 2. Dependency Injection via Svelte Context
```typescript
// contexts/services.ts
const SERVICES_KEY = Symbol('services');

export function setServices(services: ServiceContainer) {
  setContext(SERVICES_KEY, services);
}

export function getServices(): ServiceContainer {
  return getContext(SERVICES_KEY);
}
```

```svelte
<!-- App root -->
<script>
  import { setServices } from '$lib/contexts/services';
  const services = ServiceContainer.initialize();
  setServices(services);
</script>

<!-- BaseNode component -->
<script>
  import { getServices } from '$lib/contexts/services';
  const services = getServices();
  const commands = services.slashCommands.getCommands();
</script>
```

**Benefits:**
- No global state
- Perfect for testing (inject test services)
- Component tree has access without prop drilling
- Aligns with Svelte best practices

## Testing Strategy Guidelines

### Real Integration Tests (Preferred)
1. Use setup files to register real services
2. Test actual integration points
3. Verify observable side effects (dropdown closes, content updates)
4. Catch real regression bugs

### When to Use Mocks
- External API calls (network requests)
- Time-dependent behavior (Date.now, setTimeout)
- File system operations
- Expensive computations

### When NOT to Use Mocks
- Service integration (use real services in tests)
- Component communication (test real prop/event flow)
- State management (use real stores)
- Plugin systems (register real plugins)

## Related Issues

- **Issue #187:** Slash command selection not working (caught by these integration tests)
- **Testing Philosophy:** Tests should give genuine confidence, not false positives

## Key Takeaways

1. **Vitest context matters:** Global setup (Node) vs setup files (Happy-DOM) have separate module graphs
2. **Singleton pattern limitations:** `globalThis` doesn't prevent duplication across contexts
3. **Use setup files for shared state:** When tests import modules, use setup files in same context
4. **Real integration tests:** Avoid mocking when possible to catch actual bugs
5. **Future architecture:** Consider service containers and dependency injection for better testability

## Files Modified

- `/packages/desktop-app/src/tests/setup.ts` - Added plugin registration
- `/packages/desktop-app/src/tests/global-setup.ts` - Removed plugin registration, added documentation

## Test Results

```bash
bun run test slash-command-integration.test.ts
# ✓ 6/6 tests passing

bun run test slash-command
# ✓ 59/59 tests passing
```
