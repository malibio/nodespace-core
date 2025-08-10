# Code Quality Standards

> ## 🚨 **UNIVERSAL POLICY**
> 
> **This applies to ALL team members**: AI agents, human engineers, architects, and reviewers.
> 
> **NO EXCEPTIONS**: Same quality requirements regardless of human/AI status.

## 🚨 LINTING POLICY - ZERO TOLERANCE

### ZERO TOLERANCE POLICY

**FUNDAMENTAL RULES:**
- [ ] **NO lint suppression allowed anywhere in codebase** - Fix issues properly, don't suppress warnings
- [ ] **NO EXCEPTIONS** - All lint warnings and errors must be fixed with proper solutions
- [ ] **Use proper TypeScript types** instead of `any` - Type your code correctly
- [ ] **Follow Svelte best practices** - Avoid unsafe patterns like `{@html}`
- [ ] **Implement safe alternatives** - Create proper components instead of bypassing safety features

### APPROVED SOLUTIONS FOR COMMON ISSUES

**Markdown Rendering:**
- ✅ **DO**: Use structured parsing components (MarkdownRenderer) instead of `{@html}`
- ❌ **DON'T**: Use `{@html}` with raw HTML strings
- **Example**: See `src/lib/components/MarkdownRenderer.svelte`

**Type Safety:**
- ✅ **DO**: Create proper interfaces instead of using `any`
- ❌ **DON'T**: Use `any` to bypass type checking
- **Example**: `createTestNode({ type: type as 'text' | 'task' | 'ai-chat' })`

**DOM APIs:**
- ✅ **DO**: Add proper type definitions to ESLint globals
- ❌ **DON'T**: Suppress DOM-related warnings
- **Example**: Add `HTMLSpanElement: 'readonly'` to eslint.config.js globals

**Mock Objects:**
- ✅ **DO**: Type mock functions properly with correct interfaces
- ❌ **DON'T**: Use `any` for mock objects in tests
- **Example**: `(globalThis as typeof globalThis & { ResizeObserver: new () => MockResizeObserver })`

## 🚨 BLOCKING Quality Requirements

**CANNOT CREATE PR WITHOUT:**

### ESLint Requirements
- [ ] **ZERO errors** (warnings acceptable in development)
- [ ] **All auto-fixable issues resolved** via `bun run quality:fix`
- [ ] **Manual issues properly addressed** (no suppressions)

### Code Formatting
- [ ] **Prettier**: All code consistently formatted
- [ ] **No formatting inconsistencies** across the codebase

### Type Safety
- [ ] **TypeScript**: ZERO compilation errors
- [ ] **Strict type checking passed** 
- [ ] **No `any` types** except in very specific, well-documented cases

### Component Validation
- [ ] **Svelte Check**: ZERO component errors
- [ ] **No accessibility violations**
- [ ] **Proper component patterns followed**

## Quality Gates Command

**MANDATORY before creating any PR:**

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

## ❌ PROCESS VIOLATION CONSEQUENCES

**APPLIES TO ALL TEAM MEMBERS:**

### For Implementers (AI Agents & Human Engineers)
- **Creating PR with linting errors** = immediate process violation
- **Submitting code with suppressions** = critical process failure
- **Using `any` without justification** = type safety violation

### For Reviewers (AI Agents & Human Reviewers)
- **Merging PR with linting errors** = critical process failure
- **Not running quality checks first** = review process violation
- **Approving suppressed warnings** = quality gate failure

### Accountability
- Both implementer and reviewer are responsible for verification
- Process violations must be acknowledged and corrected
- **NO EXCEPTIONS**: AI agents and human team members follow identical quality standards

## PR Quality Verification

### Mandatory First Step for All Reviewers

**🚨 BEFORE ANY OTHER REVIEW STEPS:**

1. **Reviewer MUST run**: `bun run quality:fix` 
2. **Verify zero errors**: ESLint, Prettier, TypeScript, Svelte Check all pass
3. **Automatic rejection**: If any linting errors found, reject PR immediately with specific error list
4. **Document violation**: Failed linting = process violation requiring acknowledgment

### Quality Checklist

Before approving any PR:

- [ ] **Linting**: Zero ESLint errors confirmed by reviewer
- [ ] **Formatting**: Prettier applied consistently
- [ ] **Types**: No TypeScript compilation errors
- [ ] **Components**: Svelte Check passes completely
- [ ] **Standards**: No lint suppressions present
- [ ] **Safety**: No unsafe patterns (like `{@html}`) without proper alternatives

## Examples of Quality Violations

### ❌ BAD - Lint Suppression
```typescript
// eslint-disable-next-line @typescript-eslint/no-explicit-any
function handleData(data: any) {
  return data;
}
```

### ✅ GOOD - Proper Typing
```typescript
interface DataType {
  id: string;
  content: string;
}

function handleData(data: DataType) {
  return data;
}
```

### ❌ BAD - Unsafe HTML
```svelte
{@html unsafeContent}
```

### ✅ GOOD - Safe Component
```svelte
<MarkdownRenderer content={safeContent} />
```

## Quality Tools Configuration

**ESLint Configuration:**
- Strict TypeScript rules enabled
- Svelte-specific rules enforced
- DOM types properly configured in globals
- Zero tolerance for suppressions

**Prettier Configuration:**
- Consistent formatting across all file types
- Automatic formatting on save recommended

**TypeScript Configuration:**
- Strict mode enabled
- No implicit any
- Proper type checking for Svelte components

---

**Remember**: Quality is everyone's responsibility. These standards ensure consistent, safe, and maintainable code regardless of who writes or reviews it.