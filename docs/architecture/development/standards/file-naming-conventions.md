# File Naming Conventions - NodeSpace Development Standards

**Status**: ✅ ACTIVE STANDARD  
**Effective**: January 2025  
**Scope**: All new files and future refactoring

## Overview

This document establishes consistent file naming conventions for the NodeSpace project to improve maintainability, reduce case-sensitivity issues, and align with modern JavaScript/TypeScript best practices.

## Naming Conventions by File Type

### **Svelte Components**

**Standard**: `kebab-case.svelte`

```
✅ Correct:
- base-node.svelte
- text-node.svelte  
- theme-provider.svelte
- node-service-context.svelte
- autocomplete-modal.svelte

❌ Incorrect:
- BaseNode.svelte
- TextNode.svelte
- ThemeProvider.svelte
- NodeServiceContext.svelte
```

**Rationale**: 
- SvelteKit official recommendation
- Avoids case-sensitivity issues across platforms
- Consistent with modern web component standards
- Easier to distinguish from TypeScript classes

### **TypeScript Service Files**

**Standard**: `camelCase.ts`

```
✅ Correct:
- contentEditableController.ts
- cursorPositioning.ts
- nodeReferenceService.ts
- mockDatabaseService.ts
- performanceMonitor.ts

❌ Incorrect:
- ContentEditableController.ts
- CursorPositioning.ts
- NodeReferenceService.ts
```

**Rationale**:
- Consistent with variable and function naming
- Distinguishes files from class names (which remain PascalCase)
- Modern TypeScript convention
- Better IDE autocomplete and search

### **TypeScript Type/Interface Files**

**Standard**: `camelCase.ts` for mixed files, `PascalCase.ts` for type-only files

```
✅ Correct:
- types/navigation.ts (mixed exports)
- types/componentDecoration.ts (mixed exports)
- types/ApiResponse.ts (type-only file)
- interfaces/NodeData.ts (interface-only file)

❌ Incorrect:
- types/Navigation.ts (mixed exports)
- types/ComponentDecoration.ts (mixed exports)
```

### **Utility Files**

**Standard**: `camelCase.ts`

```
✅ Correct:
- utils/navigationUtils.ts
- utils/markdownUtils.ts
- helpers/domHelpers.ts

❌ Incorrect:
- utils/NavigationUtils.ts
- utils/MarkdownUtils.ts
```

### **Test Files**

**Standard**: Match the file being tested + `.test.ts`

```
✅ Correct:
- base-node.test.ts (tests base-node.svelte)
- contentEditableController.test.ts (tests contentEditableController.ts)
- nodeReferenceService.test.ts (tests nodeReferenceService.ts)

❌ Incorrect:
- BaseNode.test.ts
- ContentEditableController.test.ts
```

### **Documentation Files**

**Standard**: `kebab-case.md`

```
✅ Correct:
- file-naming-conventions.md
- component-architecture.md
- development-process.md

❌ Incorrect:
- COMPONENT_ARCHITECTURE.md
- Development_Process.md
```

## Directory Structure Standards

### **Component Organization**

```
src/lib/
├── components/           # Feature components (kebab-case)
│   ├── text-node.svelte
│   ├── node-tree.svelte
│   └── index.ts
├── design/              # Design system
│   ├── components/      # Core design components
│   │   ├── base-node.svelte
│   │   ├── theme-provider.svelte
│   │   └── index.ts
│   └── icons/           # Icon system
│       ├── icon.svelte
│       └── ui/
└── services/            # Business logic (camelCase)
    ├── nodeManager.ts
    ├── contentEditableController.ts
    └── index.ts
```

## Import/Export Standards

### **Component Imports**

```typescript
// ✅ Correct - explicit file extensions
import BaseNode from '$lib/design/components/base-node.svelte';
import TextNode from '$lib/components/text-node.svelte';

// ✅ Correct - barrel exports
import { BaseNode, ThemeProvider } from '$lib/design/components';
```

### **Service Imports**

```typescript
// ✅ Correct
import { contentEditableController } from '$lib/services/contentEditableController';
import type { NodeData } from '$lib/types/nodeData';

// ❌ Incorrect
import { ContentEditableController } from '$lib/services/ContentEditableController';
```

### **Barrel Export Pattern**

```typescript
// index.ts files should use consistent naming
export { default as BaseNode } from './base-node.svelte';
export { default as TextNode } from './text-node.svelte';
export { default as ThemeProvider } from './theme-provider.svelte';

// For services
export { nodeManager } from './nodeManager';
export { contentEditableController } from './contentEditableController';
export type { NodeData, TreeNode } from './types';
```

## Migration Standards

### **Existing Files**

**Current Policy**: Accept mixed naming as technical debt until dedicated refactoring

**Future Policy**: 
- ✅ Use new conventions for **all new files**
- ✅ Rename files during **major refactoring** of that component/service
- ✅ Update related imports when renaming
- ❌ Do not rename files during minor bug fixes or feature additions

### **Git History Preservation**

When renaming files:
```bash
# ✅ Use git mv to preserve history
git mv BaseNode.svelte base-node.svelte

# ✅ Update imports in same commit as rename
# This keeps git blame and history tracking clean
```

## Linting and Enforcement

### **ESLint Rules (Future)**

```json
{
  "rules": {
    "filename-rules/match": [
      "error",
      {
        "components/**/*.svelte": "kebab-case",
        "services/**/*.ts": "camelCase",
        "utils/**/*.ts": "camelCase"
      }
    ]
  }
}
```

### **Pre-commit Hooks (Future)**

```bash
#!/bin/sh
# Check for PascalCase component files
if git diff --cached --name-only | grep -E ".*[A-Z].*\.svelte$" | grep -v "/ui/"; then
  echo "Error: New Svelte components must use kebab-case"
  echo "Use: my-component.svelte instead of MyComponent.svelte"
  exit 1
fi
```

## Examples and Patterns

### **Component File Structure**

```
base-node.svelte              # Main component
base-node.test.ts            # Component tests
base-node.stories.ts         # Storybook stories (if used)
base-node.md                 # Component documentation (if needed)
```

### **Service File Structure**

```
nodeManager.ts               # Main service
nodeManager.test.ts         # Service tests
nodeManager.types.ts        # Service-specific types (if large)
```

### **Feature Module Structure**

```
features/text-editing/
├── components/
│   ├── text-editor.svelte
│   ├── text-toolbar.svelte
│   └── index.ts
├── services/
│   ├── textEditingService.ts
│   ├── keyboardHandler.ts
│   └── index.ts
└── types/
    └── textEditing.ts
```

## Benefits of These Conventions

### **Technical Benefits**
- **Cross-platform compatibility**: No case-sensitivity issues
- **Better tooling**: Improved IDE autocomplete and search
- **Consistent imports**: Predictable import patterns
- **Git-friendly**: Cleaner diffs and history tracking

### **Developer Experience**
- **Reduced cognitive load**: Consistent patterns to remember
- **Faster file discovery**: Predictable naming for quick navigation
- **Team coordination**: Clear standards for code reviews
- **Onboarding efficiency**: New developers learn patterns quickly

## Migration Timeline

### **Phase 1: Immediate (Completed)**
- ✅ Establish this style guide
- ✅ Add to development documentation
- ✅ Communicate to team

### **Phase 2: Future Enforcement (Planned)**
- 🔄 Add ESLint rules for new files
- 🔄 Add pre-commit hooks
- 🔄 Update VS Code workspace settings

### **Phase 3: Legacy Migration (Planned)**
- 📅 Scheduled as dedicated sprint/milestone
- 📅 Multi-agent execution plan (separate document)
- 📅 Comprehensive testing and validation

## Questions and Exceptions

### **When to Deviate**

- **External library compatibility**: Match third-party naming when required
- **Build tool requirements**: Follow bundler-specific conventions when needed
- **Legacy compatibility**: Maintain existing naming during gradual migration

### **Getting Help**

- **Questions**: Ask in development channels
- **Exceptions**: Discuss with senior architect
- **Updates**: Propose changes via documentation PR

---

**Document Version**: 1.0  
**Last Updated**: January 2025  
**Next Review**: March 2025  
**Owner**: Development Team