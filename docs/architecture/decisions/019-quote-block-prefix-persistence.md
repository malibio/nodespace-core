# ADR-019: Quote Block Prefix Persistence Strategy

## Status
**Accepted** - October 2025

## Context

NodeSpace implements specialized node types with different content transformation strategies. When implementing QuoteBlockNode (issue #277), we needed to decide how to handle the `> ` prefix that identifies block quotes.

### Existing Patterns

We have two established patterns for specialized node types:

1. **Headers** - Strip prefix in database
   - Frontend: `# Hello world` (with prefix)
   - Database: `Hello world` (without prefix)
   - Metadata: `{ headerLevel: 1 }`
   - Rationale: Header level stored in metadata, prefix is presentation-only

2. **Code Blocks** - Store language marker separately
   - Frontend: ` ```javascript\ncode here``` `
   - Database: `code here` (without backticks)
   - Metadata: `{ language: 'javascript' }`
   - Rationale: Language is metadata, code is content

### Problem Statement

For quote blocks, we had to choose between:
- **Option A**: Strip `> ` prefix, store clean content in database
- **Option B**: Retain `> ` prefix in database content

## Decision

**Quote-block content retains `> ` prefix in database.**

Example:
- Frontend (edit mode): `> Line1\n> Line2\n> Line3`
- Database: `> Line1\n> Line2\n> Line3` (same)
- Frontend (display mode): `Line1\nLine2\nLine3` (prefix stripped for rendering)

### Rationale

#### 1. **Multiline Quote Semantics**

Unlike headers (always single-line), quote blocks support multiline content where each line needs the `> ` prefix:

```
> First line of quote
> Second line of quote
> Third line of quote
```

Storing without prefixes would lose the information about which lines belong to the quote block.

#### 2. **Backend Validation Requirements**

The Rust backend's `QuoteBlockNodeBehavior::validate()` needs to verify the content is a valid quote block:

```rust
pub fn validate(&self, node: &Node) -> Result<(), NodeValidationError> {
    // Validate that content has "> " prefix and actual content after stripping
    let content_without_prefix: String = node
        .content
        .lines()
        .map(|line| line.strip_prefix("> ").or_else(|| line.strip_prefix('>')).unwrap_or(line))
        .collect::<Vec<&str>>()
        .join("\n");

    if is_empty_or_whitespace(&content_without_prefix) {
        return Err(NodeValidationError::MissingField(
            "Quote block nodes must have content beyond the '> ' prefix".to_string(),
        ));
    }
    Ok(())
}
```

Storing without prefixes would make this validation impossible.

#### 3. **Bidirectional Conversion Preservation**

When converting between node types:
- **Quote → Text**: Remove `> ` prefix from all lines
- **Text → Quote**: Add `> ` prefix to all lines

If the database doesn't store prefixes, we lose the ability to distinguish between:
- A multiline quote block that should become multiline text
- A single-line quote that should become single-line text

#### 4. **Consistency with Markdown Standards**

Markdown itself stores quote blocks with the `> ` prefix:

```markdown
> This is a quote
> that spans multiple lines
```

Storing the same format in our database maintains parity with standard markdown files.

#### 5. **Simplified Placeholder Detection**

The placeholder detection logic can check if content is "only prefixes, no actual text":

```typescript
case 'quote-block': {
  const contentWithoutPrefix = trimmedContent
    .split('\n')
    .map((line) => line.replace(/^>\s?/, ''))
    .join('\n')
    .trim();
  return contentWithoutPrefix === ''; // Placeholder if only prefixes remain
}
```

Without prefixes in database, we couldn't distinguish between empty quote blocks and placeholders.

## Alternative Considered

### Strip Prefixes (Like Headers)

**Approach:**
- Frontend adds/removes `> ` for display
- Database stores clean content: `Line1\nLine2\nLine3`
- Metadata could track per-line quote status: `{ quotedLines: [0, 1, 2] }`

**Why Rejected:**

1. **Metadata Complexity**: Tracking which lines are quoted requires complex metadata structure
2. **Mixed Content**: User could mix quoted and non-quoted lines, requiring array indexes
3. **Implementation Burden**: Every multiline operation needs metadata updates
4. **Markdown Divergence**: Doesn't match standard markdown representation
5. **Backend Validation**: Can't validate quote format without reconstructing from metadata

## Consequences

### Positive

✅ **Simple Mental Model**: Database content matches what user types (with auto-added prefixes)
✅ **Backend Validation**: Can verify quote block format integrity
✅ **Markdown Compatibility**: Direct mapping to markdown quote syntax
✅ **Robust Conversion**: Preserves multiline structure when converting node types
✅ **Clear Placeholder Detection**: Can identify empty quotes vs. quotes with content

### Negative

⚠️ **Storage Overhead**: Stores `> ` prefix for every line (minor - 2 bytes per line)
⚠️ **Display Transformation Required**: Must strip prefixes for blur mode rendering

### Implementation Notes

1. **Auto-Prefix Logic** (`quote-block-node.svelte:handleContentChange`)
   - Users only type `> ` on first line
   - Component automatically adds `> ` to all subsequent lines
   - Ensures consistent format for database storage

2. **Display Transformation** (`quote-block-node.svelte:extractQuoteForDisplay`)
   - Strips `> ` from all lines for blur mode rendering
   - Preserves markdown formatting (bold, italic, etc.)
   - Shows clean quote text without visual clutter

3. **Backend Validation** (`behaviors/mod.rs:QuoteBlockNodeBehavior`)
   - Validates content has actual text beyond `> ` prefixes
   - Prevents empty quote blocks from persisting
   - Ensures data integrity

## Related Decisions

- **ADR-015**: Text Editor Architecture Refactor (textarea-based editing)
- **ADR-017**: Node Type Pattern Detection (auto-conversion on `> ` pattern)
- Issue #277: QuoteBlockNode Implementation
- Issue #275: HeaderNode Implementation (comparison pattern)

## References

- [Markdown Specification - Block Quotes](https://spec.commonmark.org/0.30/#block-quotes)
- `packages/desktop-app/src/lib/design/components/quote-block-node.svelte`
- `packages/core/src/behaviors/mod.rs:QuoteBlockNodeBehavior`
- `packages/desktop-app/src/lib/plugins/core-plugins.ts:quoteBlockNodePlugin`
