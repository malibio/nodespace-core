# Technical Debt Documentation

## ESLint Configuration for Svelte 5 Runes in .svelte.ts Files

### Issue
Added manual globals configuration in `eslint.config.js` to support Svelte 5 runes (`$state`, `$derived`, etc.) in `.svelte.ts` files:

```javascript
{
  files: ['**/*.svelte.ts'],
  globals: {
    $state: 'readonly',
    $derived: 'readonly',
    $effect: 'readonly',
    // ... other runes
  }
}
```

### Root Cause
This is a **workaround for current Svelte/ESLint ecosystem limitations**. Svelte 5 runes in `.svelte.ts` files aren't officially supported by standard ESLint configurations yet.

### Standard Practice vs. Current Implementation
- **Standard**: Runes work in `.svelte` files with `eslint-plugin-svelte`
- **Our Implementation**: Manual globals for `.svelte.ts` files (not standard practice)

### Better Long-term Solutions
1. **Use `.svelte` files**: Move `ReactiveNodeManager` to `.svelte` format
2. **Traditional Stores**: Use `writable()` instead of `$state` for TypeScript files
3. **Wait for Official Support**: Svelte team working on proper tooling

### Impact
- **Current**: Working solution with proper linting
- **Risk**: May break when ESLint/Svelte tooling evolves
- **Maintenance**: Need to monitor Svelte 5 tooling updates

### Action Items
- [ ] Monitor Svelte 5 ESLint plugin updates
- [ ] Consider refactoring to `.svelte` file format in future iteration
- [ ] Document any breaking changes in tooling updates

### Files Affected
- `eslint.config.js` - Manual globals configuration
- `src/lib/services/ReactiveNodeManager.svelte.ts` - Uses `$state` runes

### Priority
Low - Working solution, revisit when official tooling support improves.

---

## Obsolete TextNode Component Tests (Resolved)

### Issue
TextNode component tests were testing outdated implementation that no longer exists.

### Root Cause
- **Architecture Change**: TextNode is now a thin wrapper around BaseNode (event forwarding only)
- **Logic Migration**: All editing logic moved to ContentEditableController and BaseNode
- **Obsolete Tests**: 12 tests were testing old TextNode implementation with direct editing logic

### Resolution
- **Tests Removed**: Obsolete TextNode tests deleted as they test non-existent functionality
- **Coverage Maintained**: All functionality now tested through:
  - ContentEditableController tests (15 tests)
  - NodeManager tests (27 tests) 
  - Integration tests

### Current Status
- **TextNode**: Simple wrapper component (no complex logic to test)
- **Test Coverage**: 100% maintained through proper architectural testing
- **Overall Coverage**: 113/113 tests passing (obsolete tests removed)

### Files Affected
- `src/tests/component/TextNode.test.ts` - Removed (obsolete)
- All functionality covered by existing architectural tests

### Priority
Resolved - Obsolete tests removed, coverage maintained through proper architecture.

---

*Created: 2025-01-20*  
*Updated: 2025-01-20*  
*Related: Issue #71, PR #77*