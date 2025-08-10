# NodeSpace Development Process

Guide for dependency-free development with vertical slicing and self-contained feature implementation. This process enables parallel development for all engineers.

## ⚠️ MANDATORY STARTUP SEQUENCE ⚠️

**EVERY ENGINEER MUST COMPLETE BEFORE ANY IMPLEMENTATION:**
- [ ] Create feature branch: `feature/issue-<number>-brief-description`
- [ ] Assign issue to self: `gh issue edit <number> --add-assignee "@me"`
- [ ] Update project status: Todo → In Progress (CLI or web interface)
- [ ] Read issue acceptance criteria and requirements
- [ ] Plan self-contained implementation approach

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
- `[FEATURE] Complete node hierarchy rendering with visual connecting lines`
- `[FEATURE] Complete multi-node selection system (single, range, multi-select, keyboard)`
- `[FEATURE] Complete hybrid text rendering with precise cursor positioning`
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

**CRITICAL: All engineers MUST follow these steps:**

**Status Updates (MANDATORY):**
```bash
# First, get the project item ID for your issue:
ITEM_ID=$(gh project item-list 5 --owner malibio | grep "Issue.*<issue-title>.*<number>" | awk '{print $NF}')

# When starting work on an issue:
git checkout -b feature/issue-<number>-brief-description
gh issue edit <number> --add-assignee "@me"
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4

# When blocked:
gh issue comment <number> --body "Blocked: [explanation]"
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id db18cb7f

# When creating PR:
gh pr create --title "..." --body "Closes #<number>..."
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id b13f9084

# When work is completed/merged:
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 98236657
```

**GitHub Project Status Field IDs:**
- **Project ID**: `PVT_kwHOADHu9M4A_nxN` (nodespace project)
- **Status Field ID**: `PVTSSF_lAHOADHu9M4A_nxNzgyq13o`

**Status Option IDs:**
- **Todo**: `f75ad846`
- **In Progress**: `47fc9ee4`
- **Waiting for Input**: `db18cb7f`
- **Ready for Review**: `b13f9084`
- **In Review**: `bd055968`
- **Done**: `98236657`
- **Ready to Merge**: `414430c1`

**Status Update Options:**
- **GitHub CLI**: Use `gh project` commands for programmatic updates
- **Web Interface**: Manual updates via GitHub project board
- **MCP Tools**: Use available MCP project management tools if configured
- Choose the method that works best for your environment

**Practical Example - Complete Workflow:**
```bash
# Working on Issue #25 "Add Search Functionality"
# 1. Get the project item ID
ITEM_ID=$(gh project item-list 5 --owner malibio | grep "Add Search Functionality.*25" | awk '{print $NF}')

# 2. Start work (Todo → In Progress)
git checkout -b feature/issue-25-search-functionality
gh issue edit 25 --add-assignee "@me"
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4

# 3. Create PR when ready (In Progress → Ready for Review)  
gh pr create --title "Add Search Functionality" --body "Closes #25"
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id b13f9084

# 4. After PR merge (Ready for Review → Done)
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 98236657
```

**Package Manager (MANDATORY):**
- **ALWAYS use `bun` instead of `npm`** for all frontend/JavaScript package management and scripts
- Commands: `bun install`, `bun run dev`, `bun run build`, `bun run quality:fix`, `bun run test`, etc.
- **Rationale**: Bun provides faster package installation, script execution, and better TypeScript support
- **Violation**: Using `npm` instead of `bun` is a process violation that can cause dependency and build issues

**Code Quality Gates (MANDATORY - BLOCKING):**
```bash
# CRITICAL: Before creating PR, ALL code must pass:
bun run quality:fix

# This runs:
# 1. ESLint with auto-fix
# 2. Prettier formatting  
# 3. TypeScript compilation check
# 4. Svelte component validation

# VERIFICATION REQUIRED: Must show zero errors like this:
# ✅ ESLint: 0 errors, warnings acceptable
# ✅ Prettier: All files formatted
# ✅ TypeScript: No compilation errors
# ✅ Svelte Check: No component errors
```

**🚨 BLOCKING Quality Requirements (CANNOT CREATE PR WITHOUT):**
- [ ] **ESLint**: ZERO errors (warnings acceptable in development)
- [ ] **Prettier**: Code consistently formatted
- [ ] **TypeScript**: ZERO compilation errors, strict type checking passed
- [ ] **Svelte Check**: ZERO component errors or accessibility violations

**❌ PROCESS VIOLATION CONSEQUENCES:**
- Creating PR with linting errors = immediate process violation
- Merging PR with linting errors = critical process failure
- Both engineer and reviewer are responsible for verification

**PR Review and Merge (MANDATORY WITH EXPLICIT VERIFICATION):**
1. **🚨 FIRST - Verify Code Quality Gates (BLOCKING)**: Run `bun run quality:fix` and confirm ZERO errors
2. **Review against original issue requirements FIRST** - verify all acceptance criteria are met
3. **Conduct comprehensive technical review** (use senior-architect-reviewer for complex changes)  
4. **If review shows ready to merge**: Immediately approve and merge the PR
5. **If review shows issues**: Request changes with specific feedback
6. **❌ AUTOMATIC REJECTION**: Any PR with linting/TypeScript errors must be rejected immediately

### Code Review Guidelines

**🚨 MANDATORY FIRST STEP - Linting Verification:**
- [ ] **Reviewer MUST run**: `bun run quality:fix` before any other review steps
- [ ] **Zero errors confirmed**: ESLint, Prettier, TypeScript, Svelte Check all pass
- [ ] **Automatic rejection**: If any linting errors found, reject PR immediately with specific error list
- [ ] **Engineer accountability**: Failed linting = process violation, require acknowledgment

**CRITICAL: Issue Requirements Review (SECOND PRIORITY):**
- [ ] **All acceptance criteria met**: Each checkbox in the original issue is verified and completed
- [ ] **Original requirements satisfied**: Implementation addresses the specific goals stated in the issue
- [ ] **Scope alignment**: No feature creep - implementation stays within defined scope
- [ ] **User value delivered**: The implemented solution provides the intended user benefit
- [ ] **Dependencies resolved**: Any stated dependencies are properly addressed
- [ ] **Success criteria validated**: Implementation can be demonstrated to work as specified

**Architecture Review:**
- [ ] Follows established Svelte patterns
- [ ] Interface design is clean and extensible
- [ ] Dependencies are minimal and well-justified
- [ ] Component is appropriately sized and focused

**Implementation Review:**
- [ ] **Code Quality**: All linting and formatting checks pass
- [ ] **TypeScript**: Strict type checking with no compilation errors
- [ ] **Svelte**: Reactivity used correctly, no component errors
- [ ] **Accessibility**: WCAG compliance, proper ARIA attributes
- [ ] **Error Handling**: Comprehensive error boundaries and validation
- [ ] **Performance**: No obvious performance bottlenecks

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

## GitHub Issue Creation Best Practices

### Enhanced Issue Template with Rich Rendering

GitHub provides enhanced rendering for issue references when using specific section headers. This creates professional dependency tracking with visual indicators:

```markdown
# Issue Title (Descriptive, Action-Oriented)

Brief description of what needs to be accomplished.

## Dependencies
- #15 (blocks this issue)
- #16 (must be completed first)

## Related Issues  
- #4 (parent feature/epic)

## Requirements
- **Specific Feature**: Detailed technical requirement
- **Implementation Detail**: Exact specifications with examples
- **File Locations**: Specific paths where code should be created
- **Technical Specs**: Precise technical details (SVG paths, CSS properties, etc.)

## Acceptance Criteria
- [ ] Specific, testable outcome
- [ ] Measurable success criteria  
- [ ] User-facing functionality verified
- [ ] Technical implementation validated
- [ ] No compilation errors or warnings
```

### Issue Reference Rendering

**Rich GitHub Rendering Triggers:**
- **Dependencies**: Shows checkboxes with issue status and titles
- **Related Issues**: Shows checkboxes with issue status and titles  
- **Issues** (generic): Shows checkboxes with issue status and titles

**Simple References:**
- `#4` in regular text → auto-expands with issue title
- `Issue #4` in regular text → renders as basic hyperlink

### Self-Contained Issue Requirements

Every issue must include sufficient detail for independent implementation:

**Technical Specifications:**
- Exact file paths and structure
- Specific code patterns to follow
- Design tokens and CSS properties to use
- Component interfaces and props
- Testing requirements and approaches

**Implementation Guidance:**
- Reference existing similar implementations
- Specify design system integration points
- Include example code snippets for complex requirements
- Define clear success criteria

**Resource References:**
- Link to relevant architecture documentation
- Reference existing components to mimic
- Include design system tokens and patterns
- Specify testing utilities and patterns

### Issue Dependency Management

**True Dependencies (use "Dependencies" section):**
- Issue cannot start without dependency completion
- Blocks implementation progress
- Creates sequential workflow

**Feature Relationships (use "Related Issues" section):**
- Part of same epic/feature
- Provides context but doesn't block
- Enables parallel development

### Quality Gates for Issues

Before creating an issue, verify:
- [ ] **Self-contained**: Can be implemented independently
- [ ] **Specific**: Clear technical requirements with examples
- [ ] **Testable**: Measurable acceptance criteria
- [ ] **Valuable**: Delivers user-facing functionality
- [ ] **Scoped**: Focused on single responsibility
- [ ] **Detailed**: Includes all necessary technical specifications

This development process ensures engineers can create valuable, working features independently while maintaining architectural coherence for future integration.