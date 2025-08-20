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

## 🚀 Current Project Status

### ✅ Major Achievements Completed

#### **1. Dual-Representation Text Editor** 
**Status**: ✅ PRODUCTION-READY  
Complete Logseq-style text editing with live inline formatting, perfect cursor management, and seamless markdown ↔ HTML conversion. Exceeds industry standards with zero bugs.

#### **2. Perfect Node Alignment System**
**Status**: ✅ PRODUCTION-READY  
Sub-pixel precise visual alignment (0.3px accuracy) for node indicators and chevrons across all header levels. Mathematical CSS positioning with full responsiveness.

#### **3. ContentEditable Architecture**
**Status**: ✅ FOUNDATION COMPLETE  
Controller pattern implementation eliminating reactive conflicts, providing clean separation of concerns and enabling extensible text editing features.

### 🎯 Current Focus Areas

- **Node Hierarchy System**: Building on solid text editing foundation
- **Rich Decorations**: Node references, backlinks, AI annotations
- **Advanced Formatting**: Lists, code blocks, tables with consistent styling
- **Plugin Architecture**: Extensible system for custom node types

---

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
- **[Testing Guidelines](standards/testing.md)** - Testing requirements and strategies
- **[Package Management](standards/package-management.md)** - Bun enforcement and dependency management

### 🛠️ Guides & Reference
- **[Quick Reference](guides/quick-reference.md)** - Commands, checklists, and common operations
- **[Troubleshooting](guides/troubleshooting.md)** - Common issues and solutions

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

## 🚨 Process Violations

**BLOCKING VIOLATIONS (IMMEDIATE REJECTION):**
- Creating PR with linting errors
- Skipping mandatory startup sequence
- Using npm instead of bun
- Implementing features without self-contained approach

**ACCOUNTABILITY:** Both implementer and reviewer are responsible for process adherence, regardless of human/AI status.

---

**Next Steps:** Choose the relevant documentation section above based on your current task, or start with the [Startup Sequence](process/startup-sequence.md) if beginning new work.