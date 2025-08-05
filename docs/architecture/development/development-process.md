# NodeSpace Development Process

Guide for dependency-free development with vertical slicing and self-contained feature implementation. This process enables parallel development whether using AI agents or human developers.

## Core Principles

### 1. Dependency-Free Issue Design

**The Problem with Traditional Dependencies:**
```
❌ Blocking Chain:
[TASK] Create database schema
  ↓ (blocks)
[TASK] Build API endpoints  
  ↓ (blocks)
[TASK] Add frontend UI
  ↓ (blocks)
[TASK] Write integration tests
```

**The Solution - Vertical Slicing:**
```
✅ Parallel Development:
[FEATURE] Complete Text Node System (database + API + UI + tests)
[FEATURE] Complete Task Node System (database + API + UI + tests)
[FEATURE] Complete AI Chat System (database + API + UI + tests)
[INTEGRATION] Connect systems via shared interfaces
```

### 2. Self-Contained Feature Implementation

Each issue should be:
- **Independently implementable** - No waiting for other work
- **Demonstrable** - Shows working functionality
- **Testable** - Includes verification criteria
- **Valuable** - Delivers user-facing capability

### 3. Mock-First Development

Enable parallel work by mocking dependencies:
```typescript
// ✅ TextNode can work independently
const mockDataStore = {
  saveNode: (node) => Promise.resolve(node.id),
  loadNode: (id) => Promise.resolve(mockNodes[id]),
  searchNodes: (query) => Promise.resolve(mockSearchResults)
};

// Real implementation replaces mock later
const realDataStore = new LanceDBDataStore(config);
```

## Issue Creation Guidelines

### Feature Implementation Template

```markdown
# [FEATURE] Complete [Component] System

## Overview
Brief description of the complete feature being implemented.

## Self-Contained Scope
- Component implementation with all required functionality
- Local state management (upgradeable to shared state later)
- Mock data and services where needed for independence
- Comprehensive testing with demo scenarios
- Documentation and usage examples

## Implementation Approach
1. **Component Structure**: Follow existing patterns
2. **State Management**: Start with local state, design for easy upgrade
3. **Data Layer**: Use mocks initially, implement interface for real data
4. **Testing Strategy**: Unit tests + integration tests with mock data
5. **Demo/Documentation**: Working examples and usage guide

## Acceptance Criteria
- [ ] Component renders and functions correctly
- [ ] All user interactions work as expected
- [ ] State persists appropriately (local storage, memory, etc.)
- [ ] Comprehensive test coverage (>90%)
- [ ] Working demo available
- [ ] Documentation complete
- [ ] Code follows project standards

## Technical Implementation Notes
- Patterns to follow: [reference existing components]
- Dependencies to mock: [list external dependencies]
- Interfaces to implement: [define clean interfaces]
- Testing approach: [specific testing strategy]

## Resources & References
- Similar implementations: [file paths]
- Type definitions: [interface files]  
- Testing utilities: [test helper files]
- Design patterns: [architecture docs]

## Definition of Done
- [ ] Feature works end-to-end in isolation
- [ ] All tests pass
- [ ] Code review completed
- [ ] Documentation updated
- [ ] Demo recorded/available
- [ ] Ready for integration with other systems
```

## Development Stages

### Stage 1: Core Architecture
**Independent Work Packages:**
- `[FEATURE] Complete Svelte component store system`
- `[FEATURE] Complete node hierarchy rendering`
- `[FEATURE] Complete basic routing and navigation`
- `[FEATURE] Complete Tauri integration layer`

### Stage 2: AI Integration
**Self-Contained AI Features:**
- `[FEATURE] Complete TextNode with AI assistance`
- `[FEATURE] Complete AIChatNode with intent classification`
- `[FEATURE] Complete entity CRUD with natural language`
- `[FEATURE] Complete validation system with AI rules`

### Stage 3: Advanced Features
**Enhanced Functionality:**
- `[FEATURE] Complete QueryNode with real-time updates`
- `[FEATURE] Complete calculated fields system`
- `[FEATURE] Complete PDF plugin architecture`
- `[FEATURE] Complete image processing plugin`

### Stage 4: Production Readiness
**System Integration:**
- `[INTEGRATION] Connect all node types to unified data store`
- `[INTEGRATION] Implement cross-node search and linking`
- `[FEATURE] Complete performance monitoring`
- `[FEATURE] Complete deployment automation`

## Implementation Patterns

### Self-Contained Component Development

**Example: Complete TextNode Implementation**
```typescript
// TextNode.svelte - Complete self-contained implementation
<script lang="ts">
  import { writable } from 'svelte/store';
  import { createEventDispatcher } from 'svelte';
  
  // Props with sensible defaults
  export let initialContent = '';
  export let readOnly = false;
  export let autoSave = true;
  
  // Local state management
  const content = writable(initialContent);
  const isEditing = writable(false);
  const saveStatus = writable<'saved' | 'saving' | 'unsaved'>('saved');
  
  const dispatch = createEventDispatcher();
  
  // Mock data store for independent development
  const mockStore = {
    async save(nodeContent: string): Promise<void> {
      $saveStatus = 'saving';
      // Simulate network delay
      await new Promise(resolve => setTimeout(resolve, 500));
      $saveStatus = 'saved';
      dispatch('saved', nodeContent);
    }
  };
  
  // Auto-save functionality
  let saveTimeout: NodeJS.Timeout;
  $: if (autoSave && $content !== initialContent) {
    clearTimeout(saveTimeout);
    $saveStatus = 'unsaved';
    saveTimeout = setTimeout(() => {
      mockStore.save($content);
    }, 1000);
  }
  
  // Keyboard shortcuts
  function handleKeydown(event: KeyboardEvent) {
    if (event.ctrlKey && event.key === 's') {
      event.preventDefault();
      mockStore.save($content);
    }
    if (event.key === 'Escape') {
      $isEditing = false;
    }
  }
</script>

<div class="text-node" class:editing={$isEditing}>
  {#if $isEditing}
    <textarea
      bind:value={$content}
      on:keydown={handleKeydown}
      on:blur={() => $isEditing = false}
      placeholder="Enter your text..."
      disabled={readOnly}
    ></textarea>
  {:else}
    <div
      class="content"
      on:click={() => !readOnly && ($isEditing = true)}
      on:keydown={(e) => e.key === 'Enter' && !readOnly && ($isEditing = true)}
      tabindex="0"
      role="button"
    >
      {$content || 'Click to edit...'}
    </div>
  {/if}
  
  <div class="status">
    {#if $saveStatus === 'saving'}
      <span class="saving">Saving...</span>
    {:else if $saveStatus === 'unsaved'}
      <span class="unsaved">Unsaved changes</span>
    {:else}
      <span class="saved">Saved</span>
    {/if}
  </div>
</div>

<style>
  .text-node {
    border: 1px solid #ddd;
    border-radius: 4px;
    padding: 12px;
    min-height: 100px;
    position: relative;
  }
  
  .text-node.editing {
    border-color: #007acc;
  }
  
  textarea {
    width: 100%;
    height: 100px;
    border: none;
    outline: none;
    resize: vertical;
    font-family: inherit;
  }
  
  .content {
    min-height: 100px;
    cursor: pointer;
    color: #333;
  }
  
  .content:hover {
    background-color: #f9f9f9;
  }
  
  .status {
    position: absolute;
    top: 4px;
    right: 8px;
    font-size: 12px;
  }
  
  .saving { color: #007acc; }
  .unsaved { color: #ff9500; }
  .saved { color: #28a745; }
</style>
```

### Mock Data Layer Pattern

```typescript
// mockDataStore.ts - Enable independent development
export interface NodeData {
  id: string;
  type: 'text' | 'task' | 'ai-chat';
  content: string;
  metadata: Record<string, any>;
  createdAt: Date;
  updatedAt: Date;
}

export class MockDataStore {
  private nodes = new Map<string, NodeData>();
  private static instance: MockDataStore;
  
  static getInstance(): MockDataStore {
    if (!MockDataStore.instance) {
      MockDataStore.instance = new MockDataStore();
    }
    return MockDataStore.instance;
  }
  
  async saveNode(node: Partial<NodeData>): Promise<string> {
    const id = node.id || this.generateId();
    const nodeData: NodeData = {
      ...node,
      id,
      createdAt: node.createdAt || new Date(),
      updatedAt: new Date()
    } as NodeData;
    
    this.nodes.set(id, nodeData);
    
    // Simulate network delay
    await new Promise(resolve => setTimeout(resolve, 100));
    return id;
  }
  
  async loadNode(id: string): Promise<NodeData | null> {
    await new Promise(resolve => setTimeout(resolve, 50));
    return this.nodes.get(id) || null;
  }
  
  async searchNodes(query: string): Promise<NodeData[]> {
    await new Promise(resolve => setTimeout(resolve, 200));
    return Array.from(this.nodes.values())
      .filter(node => 
        node.content.toLowerCase().includes(query.toLowerCase()) ||
        JSON.stringify(node.metadata).toLowerCase().includes(query.toLowerCase())
      );
  }
  
  async deleteNode(id: string): Promise<boolean> {
    await new Promise(resolve => setTimeout(resolve, 100));
    return this.nodes.delete(id);
  }
  
  private generateId(): string {
    return `node-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}

// Easy to replace with real implementation later
export const dataStore = MockDataStore.getInstance();
```

### Integration-Ready Architecture

```typescript
// nodeStore.ts - Interface-based design for easy integration
export interface DataStore {
  saveNode(node: Partial<NodeData>): Promise<string>;
  loadNode(id: string): Promise<NodeData | null>;
  searchNodes(query: string): Promise<NodeData[]>;
  deleteNode(id: string): Promise<boolean>;
}

// Configuration-driven store selection
export function createDataStore(config: AppConfig): DataStore {
  switch (config.dataStore.type) {
    case 'mock':
      return MockDataStore.getInstance();
    case 'lancedb':
      return new LanceDBDataStore(config.dataStore.lancedb);
    // PostgreSQL support removed - using LanceDB only
    default:
      throw new Error(`Unknown data store type: ${config.dataStore.type}`);
  }
}

// Components use interface, not implementation
import { getContext } from 'svelte';
export const dataStore = getContext<DataStore>('dataStore');
```

## Quality Assurance

### Definition of Done Checklist

```markdown
## Feature Completion Criteria

### Functionality ✅
- [ ] All acceptance criteria met
- [ ] Component works in isolation 
- [ ] Handles edge cases appropriately
- [ ] Error states implemented
- [ ] Loading states implemented
- [ ] Keyboard navigation works
- [ ] Accessibility requirements met

### Code Quality ✅
- [ ] Follows Svelte/TypeScript best practices
- [ ] Types are complete and accurate
- [ ] No console errors or warnings
- [ ] Performance is acceptable (< 100ms interactions)
- [ ] Memory leaks prevented
- [ ] Security considerations addressed

### Testing ✅
- [ ] Unit tests >90% coverage
- [ ] Integration tests cover main workflows
- [ ] Edge cases tested
- [ ] Error conditions tested
- [ ] All tests pass consistently
- [ ] Test data cleanup implemented

### Documentation ✅
- [ ] Component API documented
- [ ] Usage examples provided
- [ ] Demo application works
- [ ] README updated if needed
- [ ] Code comments for complex logic

### Integration Readiness ✅
- [ ] Clean interfaces defined
- [ ] No hard dependencies on unfinished work
- [ ] Mock implementations clearly marked
- [ ] Configuration options available
- [ ] Ready for real data integration
```

### Mandatory Process Steps

**CRITICAL: All developers (human and AI) MUST follow these steps:**

**Status Updates (MANDATORY):**
```bash
# When starting work on an issue:
gh issue edit <number> --add-assignee "@me"
# Then manually update project status: Todo → In Progress (via GitHub web interface)

# When blocked:
# Manual update: In Progress → Waiting for Input (add comment explaining block)

# When creating PR:
gh pr create --title "..." --body "Closes #<number>..."
# Then manually update: In Progress → Ready for Review (via GitHub web interface)
```

**PR Review and Merge (MANDATORY):**
1. **Conduct comprehensive review** (use senior-architect-reviewer for complex changes)
2. **If review shows ready to merge**: Immediately approve and merge the PR
3. **If review shows issues**: Request changes with specific feedback
4. **No additional approval required** unless explicitly stated in issue

**Status Update Requirements:**
- GitHub project status cannot be updated programmatically via CLI
- Manual updates via GitHub web interface are REQUIRED at each transition
- Status automation only works for GitHub PR review state changes
- **Failure to update status blocks the development process**

### Code Review Guidelines

**Architecture Review:**
- [ ] Follows established Svelte patterns
- [ ] Interface design is clean and extensible
- [ ] Dependencies are minimal and well-justified
- [ ] Component is appropriately sized and focused

**Implementation Review:**
- [ ] Svelte reactivity used correctly
- [ ] TypeScript types are comprehensive
- [ ] Error handling is comprehensive
- [ ] Performance considerations addressed

**Testing Review:**
- [ ] Tests use @testing-library/svelte appropriately
- [ ] Test coverage is comprehensive
- [ ] Integration scenarios tested
- [ ] Mocks are appropriate and realistic

## Integration Patterns

### Progressive Enhancement Strategy

```typescript
// Phase 1: Self-contained with mocks
const textNodeConfig: TextNodeConfig = {
  dataStore: mockDataStore,
  features: {
    autoSave: true,
    aiAssistance: false,
    collaboration: false
  }
};

// Phase 2: Real backend integration
const integratedConfig: TextNodeConfig = {
  dataStore: lanceDBStore,
  features: {
    autoSave: true,
    aiAssistance: false,
    collaboration: false
  }
};

// Phase 3: Advanced features enabled
const advancedConfig: TextNodeConfig = {
  dataStore: lanceDBStore,
  features: {
    autoSave: true,
    aiAssistance: true,
    collaboration: true
  }
};
```

### Interface Contracts

```typescript
// Contracts that enable independent development
export interface NodeComponent {
  // Required methods all node components must implement
  save(): Promise<void>;
  load(id: string): Promise<void>;
  validate(): ValidationResult;
  export(): NodeData;
  
  // Optional methods for enhanced features
  enableAI?(): void;
  enableCollaboration?(): void;
  setTheme?(theme: Theme): void;
}

// Each implementation can work independently
export class TextNode implements NodeComponent {
  async save(): Promise<void> { /* implementation */ }
  async load(id: string): Promise<void> { /* implementation */ }
  validate(): ValidationResult { /* implementation */ }
  export(): NodeData { /* implementation */ }
}
```

This development process ensures AI agents can create valuable, working features independently while maintaining architectural coherence for future integration.