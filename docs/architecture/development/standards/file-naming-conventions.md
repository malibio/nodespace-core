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

**Standard**: `kebab-case.ts`

```
✅ Correct:
- content-editable-controller.ts
- cursor-positioning.ts
- node-reference-service.ts
- mock-database-service.ts
- performance-monitor.ts

❌ Incorrect:
- ContentEditableController.ts
- contentEditableController.ts
- CursorPositioning.ts
```

**Rationale** (Updated January 2025):
- **Industry alignment**: Matches 2024-2025 TypeScript best practices
- **Cross-platform safety**: Avoids case-sensitivity issues on Mac/Windows
- **Package naming compatibility**: Aligns with npm lowercase requirement
- **Framework consistency**: Matches SvelteKit and modern framework conventions
- **Ecosystem standard**: Dominant pattern in modern TypeScript projects

### **TypeScript Type/Interface Files**

**Standard**: `kebab-case.ts` (consistent with all TypeScript files)

```
✅ Correct:
- types/navigation.ts
- types/component-decoration.ts
- types/api-response.ts
- types/node-data.ts

❌ Incorrect:
- types/Navigation.ts
- types/componentDecoration.ts
- types/ApiResponse.ts (previously allowed, now deprecated)
```

### **Utility Files**

**Standard**: `kebab-case.ts`

```
✅ Correct:
- utils/navigation-utils.ts
- utils/markdown-utils.ts
- helpers/dom-helpers.ts

❌ Incorrect:
- utils/NavigationUtils.ts
- utils/navigationUtils.ts
```

### **Test Files**

**Standard**: Match the file being tested + `.test.ts` (always kebab-case)

```
✅ Correct:
- base-node.test.ts (tests base-node.svelte)
- content-editable-controller.test.ts (tests content-editable-controller.ts)
- node-reference-service.test.ts (tests node-reference-service.ts)

❌ Incorrect:
- BaseNode.test.ts
- ContentEditableController.test.ts
- contentEditableController.test.ts
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
└── services/            # Business logic (kebab-case)
    ├── node-manager.ts
    ├── content-editable-controller.ts
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
import { contentEditableController } from '$lib/services/content-editable-controller';
import type { NodeData } from '$lib/types/node-data';

// ❌ Incorrect
import { ContentEditableController } from '$lib/services/ContentEditableController';
import { contentEditableController } from '$lib/services/contentEditableController'; // Old convention
```

### **Barrel Export Pattern**

```typescript
// index.ts files should use consistent naming
export { default as BaseNode } from './base-node.svelte';
export { default as TextNode } from './text-node.svelte';
export { default as ThemeProvider } from './theme-provider.svelte';

// For services
export { nodeManager } from './node-manager';
export { contentEditableController } from './content-editable-controller';
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

### **✅ ESLint Rules (ACTIVE)**

**Implementation**: The naming conventions are now automatically enforced through ESLint using `eslint-plugin-unicorn`.

**Configuration** (`eslint.config.js`):

```javascript
// Svelte components must use kebab-case
{
  files: ['**/*.svelte'],
  plugins: { unicorn },
  rules: {
    'unicorn/filename-case': ['error', {
      cases: { kebabCase: true }
    }]
  }
}

// TypeScript service files must use kebab-case
{
  files: ['**/*.{js,ts}'],
  plugins: { unicorn },
  rules: {
    'unicorn/filename-case': ['error', {
      cases: { kebabCase: true },
      ignore: ['index\\.ts$', '\\.d\\.ts$', '\\.test\\.ts$', '\\.spec\\.ts$']
    }]
  }
}

// Test files must use kebab-case
{
  files: ['src/tests/**/*.{js,ts}'],
  plugins: { unicorn },
  rules: {
    'unicorn/filename-case': ['error', {
      cases: { kebabCase: true }
    }]
  }
}
```

**Real-time Feedback**:
```bash
# ❌ Linting will catch violations:
$ bun run lint:check

/src/lib/services/WrongName.ts
  1:1  error  Filename is not in kebab case. Rename it to `wrong-name.ts`  unicorn/filename-case

/src/lib/components/WrongName.svelte  
  1:1  error  Filename is not in kebab case. Rename it to `wrong-name.svelte`  unicorn/filename-case

✖ 2 problems (2 errors, 0 warnings)
```

### **Quality Scripts**

The naming rules are integrated into the standard quality check workflow:

```bash
# Check naming violations (and other linting issues)
bun run lint:check

# Auto-fix what can be fixed, check naming violations  
bun run lint

# Complete quality check (lint + format + type check)
bun run quality:fix
```

### **Development Workflow Integration**

**✅ Pre-commit Prevention**: ESLint rules block commits with naming violations:
```bash
# This workflow will fail if files don't follow conventions:
git add src/lib/services/BadName.ts
git commit -m "Add service"  
# ESLint error prevents commit
```

**✅ CI/CD Integration**: Linting runs in build pipeline:
```bash
# Production builds will fail with naming violations
bun run build  # Includes lint check
```

**✅ IDE Integration**: Real-time feedback during development:
- VS Code shows red squiggles for naming violations
- Auto-suggestions provide correct naming format
- Problems panel shows specific rename recommendations

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
node-manager.ts               # Main service
node-manager.test.ts         # Service tests
node-manager.types.ts        # Service-specific types (if large)
```

### **Feature Module Structure**

```
features/text-editing/
├── components/
│   ├── text-editor.svelte
│   ├── text-toolbar.svelte
│   └── index.ts
├── services/
│   ├── text-editing-service.ts
│   ├── keyboard-handler.ts
│   └── index.ts
└── types/
    └── text-editing.ts
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

### **Phase 2: Enforcement Implementation (✅ COMPLETED)**
- ✅ Added ESLint rules for automatic enforcement
- ✅ Integrated with development workflow (lint, build, quality scripts)
- ✅ Real-time IDE feedback and commit prevention
- ✅ Updated documentation with implementation details

### **Phase 3: TypeScript Migration to kebab-case (In Progress)**
- 🔄 Updating TypeScript naming convention from camelCase to kebab-case (Issue #181)
- 🔄 Aligning with 2024-2025 industry standards
- 🔄 Migrating ~38 TypeScript files with git history preservation
- Target: Full kebab-case compliance across all file types

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