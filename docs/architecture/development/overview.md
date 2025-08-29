# NodeSpace Development Process Overview

**Universal guide for ALL development team members**: AI agents, human engineers, and architects. This process enables dependency-free development with vertical slicing and self-contained feature implementation for parallel development.

> ## 🤖👨‍💻 **UNIVERSAL PROCESS NOTICE**
> 
> **This process applies EQUALLY to:**
> - ✅ **AI Agents** (Claude, GPT, custom agents)
> - ✅ **Human Engineers** (frontend, backend, full-stack)
> - ✅ **Human Architects** (senior, principal, staff)
> - ✅ **Human Reviewers** (tech leads, peer reviewers)
> 
> **NO EXCEPTIONS**: All quality gates, requirements, and procedures are identical for AI agents and human team members. This ensures consistent quality and process adherence regardless of who (or what) implements or reviews code.


## 📚 Documentation Navigation

### 🏛️ Architecture Decisions
- **[Dual-Representation Completion](../decisions/2025-01-dual-representation-completion.md)** - Complete text editor implementation
- **[Perfect Node Alignment System](../decisions/2025-01-perfect-node-alignment-system.md)** - Mathematical positioning system
- **[ContentEditable Pivot](../decisions/2025-01-contenteditable-pivot.md)** - Strategic architecture decision
- **[Text Editor Architecture Refactor](../decisions/2025-01-text-editor-architecture-refactor.md)** - Clean architecture implementation

### 🚀 Getting Started
- **[Startup Sequence](process/startup-sequence.md)** - Mandatory steps before any implementation work
- **[Issue Workflow](process/issue-workflow.md)** - Complete workflow from issue creation to PR merge
- **[Quick Reference](guides/quick-reference.md)** - Checklists and common commands

### 🔄 Development Process
- **[Startup Sequence](process/startup-sequence.md)** - Required pre-implementation steps
- **[Epic and Sub-Issue Workflow](process/epic-and-subissue-workflow.md)** - Working with parent issues and sub-issues
- **[Issue Workflow](process/issue-workflow.md)** - Issue creation through PR merge
- **[PR Review Guidelines](process/pr-review.md)** - Code review process and standards

### 🏗️ Development Patterns
- **[Self-Contained Features](patterns/self-contained-features.md)** - Independent feature implementation
- **[Mock-First Development](patterns/mock-first-development.md)** - Parallel development with mocks
- **[Vertical Slicing](patterns/vertical-slicing.md)** - Full-stack feature development

### 📋 Standards & Quality
- **[Code Quality](standards/code-quality.md)** - Linting policy, quality gates, and enforcement
- **[File Naming Conventions](standards/file-naming-conventions.md)** - Consistent naming standards for all files and components
- **[Testing Guidelines](standards/testing.md)** - Testing requirements and strategies
- **[Package Management](standards/package-management.md)** - Bun enforcement and dependency management
- **[Technical Debt](technical-debt.md)** - Known limitations, workarounds, and future improvements

### 🛠️ Guides & Reference
- **[Quick Reference](guides/quick-reference.md)** - Commands, checklists, and common operations
- **[Troubleshooting](guides/troubleshooting.md)** - Common issues and solutions

## 🎯 Current Implementation Status

### Advanced Text Editor Architecture
- **Smart Text Splitting**: Format-preserving content division on Enter key
- **Intelligent Cursor Positioning**: Optimal placement after inherited syntax
- **Dual Representation**: Markdown source ↔ AST ↔ Display HTML

### Sophisticated Keyboard Handling
- **Hierarchical Awareness**: Different rules for parent/child node interactions
- **Collapsed State Intelligence**: Children transfer rules based on expand/collapse state
- **Format Inheritance**: Smart format precedence on backspace operations

🔗 **See**: [`lessons/sophisticated-keyboard-handling.md`](lessons/sophisticated-keyboard-handling.md) - Complete keyboard behavior documentation

### Reactive State Management
- **Svelte 5 Runes**: Modern reactivity with $state(), $effect(), $derived()
- **Synchronization**: Dual state tracking with consistent UI updates
- **Performance**: Optimized reactivity triggers and state synchronization

🔗 **See**: [`lessons/reactivity-state-management.md`](lessons/reactivity-state-management.md) - Reactivity patterns and lessons learned

### Component Architecture
```
BaseNodeViewer (UI Coordination)
├── TextNode (Content Management)
├── BaseNode (Core Rendering)
└── ContentEditableController (DOM Interaction)
```

### State Management
```
ReactiveNodeManager (Svelte 5 Reactivity)
├── NodeManager (Core Logic)
├── ContentProcessor (Dual Representation)
└── State Synchronization Layer
```

## 🎯 Core Development Principles

### 1. Dependency-Free Issue Design
Enable parallel development by designing issues that don't block each other:

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
Enable parallel work by mocking dependencies initially, then replacing with real implementations.

### 4. Key Architecture Patterns

1. **Separation of Concerns**: Pure logic in managers, reactive wrappers for UI
2. **Event-Driven Architecture**: Clean communication between components
3. **Performance First**: Minimal DOM manipulation, efficient state updates
4. **Type Safety**: Full TypeScript coverage with strict configuration
5. **UI-First Development**: Build interfaces before backend integration
6. **Vertical Slicing**: Complete features end-to-end rather than horizontal layers
7. **Forward-Facing Fixes**: Root cause solutions rather than temporary workarounds

## ⚠️ Critical Requirements

### Mandatory Startup Sequence
**EVERY TEAM MEMBER (AI AGENTS & HUMANS) MUST COMPLETE BEFORE ANY IMPLEMENTATION:**

1. Create feature branch: `feature/issue-<number>-brief-description`
2. Assign issue to self: `bun run gh:assign <number> "@me"`
3. Update project status: Todo → In Progress
4. Read issue acceptance criteria and requirements
5. Plan self-contained implementation approach

### Zero-Tolerance Quality Policy
- **NO lint suppression allowed** - Fix issues properly, don't suppress warnings
- **NO EXCEPTIONS** - All lint warnings and errors must be fixed with proper solutions
- **Universal standards** - Same requirements for AI agents and human engineers
- **Bun only** - Package manager enforcement with preinstall hooks
- **Comprehensive testing** - Unit tests, integration tests, manual validation

### Quality Gates
- Build success required
- Linting passes without warnings
- Type checking completes successfully
- Manual testing for complex interactions

## 🚨 Process Violations

**BLOCKING VIOLATIONS (IMMEDIATE REJECTION):**
- Creating PR with linting errors
- Skipping mandatory startup sequence
- Using npm instead of bun
- Implementing features without self-contained approach

**ACCOUNTABILITY:** Both implementer and reviewer are responsible for process adherence, regardless of human/AI status.

## 🧪 Testing Strategy

### Manual Testing Focus
- **Keyboard interactions**: Complex Enter/Backspace scenarios
- **Hierarchy operations**: Parent/child relationships and collapsed states
- **Format preservation**: Markdown syntax handling during operations
- **State synchronization**: UI consistency during complex state changes

### Automated Testing
- Unit tests for core logic (NodeManager, ContentProcessor)
- Integration tests for component interactions
- Performance benchmarks for large document operations
- Regression tests for critical keyboard behaviors

## 🔮 Future Considerations

### Performance Optimizations
- Large document handling strategies
- Memory management for complex hierarchies
- Efficient tree traversal algorithms
- Reactive state optimization

### User Experience Enhancements
- Animation support for hierarchy changes
- Accessibility improvements for complex interactions
- Customizable keyboard behavior preferences
- Advanced undo/redo for sophisticated operations

---

**Next Steps:** Choose the relevant documentation section above based on your current task, or start with the [Startup Sequence](process/startup-sequence.md) if beginning new work.

_NodeSpace prioritizes sophisticated user experience through intelligent keyboard handling, efficient state management, parallel development patterns, and comprehensive testing practices._