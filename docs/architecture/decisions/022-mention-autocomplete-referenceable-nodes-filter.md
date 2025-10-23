# ADR-022: Mention Autocomplete Referenceable Nodes Filter

**Status:** Accepted
**Date:** 2025-10-23
**Deciders:** Development Team + Pragmatic Code Review

## Context

NodeSpace's @mention autocomplete feature needed to restrict results to only nodes that make sense to reference (containers and tasks), excluding child text nodes and other content fragments that shouldn't be directly mentioned.

The key question was how to implement filtering:

1. **Boolean flag approach** - Add `include_containers_and_tasks` parameter to NodeQuery
2. **Flexible filter builder** - Implement a composable filter system for future extensibility
3. **Frontend post-processing** - Filter results in the frontend after database query
4. **Separate query method** - Create dedicated `query_referenceable_nodes()` method

## Decision

We will implement filtering as a **boolean flag (`include_containers_and_tasks`) in the NodeQuery struct**, applied at the SQL level via a helper function that generates filter clauses.

## Rationale

### Primary Use Case Alignment

The filter's primary use case is:
> "@mention autocomplete should only show task nodes and container/root nodes, not individual text paragraphs or other child content"

This requirement is specific and well-defined - there's no current need for arbitrary filter combinations or complex filtering logic.

### Technical Analysis

**Advantages of Boolean Flag Approach:**

1. **Simplicity**: Single optional boolean parameter, minimal API surface
2. **SQL-Level Efficiency**: Filter applied at database level (not post-processing in frontend)
3. **Consistent Application**: Automatically works across all query paths (mentioned_by, content_contains, node_type)
4. **Backward Compatible**: Optional parameter defaults to `false` (no breaking changes)
5. **Clear Intent**: Name `include_containers_and_tasks` explicitly states what's included
6. **Maintainable**: Centralized filter logic in `build_container_task_filter()` helper
7. **YAGNI Principle**: Doesn't over-engineer for hypothetical future requirements

**Disadvantages of Alternative Approaches:**

1. **Filter Builder Pattern**: Would add ~200 lines of code for a single use case (over-engineering)
2. **Frontend Post-Processing**: Inefficient - fetches unwanted nodes then discards them
3. **Separate Query Method**: API proliferation - every filter variant would need its own method

### Implementation Details

**SQL Filter Generation:**

```rust
fn build_container_task_filter(enabled: bool, table_alias: Option<&str>) -> String {
    if !enabled {
        return String::new();
    }
    let prefix = table_alias.map(|a| format!("{}.", a)).unwrap_or_default();
    format!(
        " AND ({}node_type = 'task' OR {}container_node_id IS NULL)",
        prefix, prefix
    )
}
```

**Safety:**
- Only generates hardcoded SQL fragments (no user input)
- `table_alias` only called with compile-time constants (`Some("n")` or `None`)
- No SQL injection risk

**Filter Logic:**
- **Includes**: Task nodes (`node_type = 'task'`) OR container nodes (`container_node_id IS NULL`)
- **Excludes**: Text children and other non-referenceable content fragments

### Query Priority Order

The filter integrates seamlessly into the existing query priority system:

1. `id` - Direct node lookup
2. `mentioned_by` - Nodes referencing target (filter applied)
3. `content_contains` + optional `node_type` - Full-text search (filter applied)
4. `node_type` - Type-based query (filter applied)
5. **`include_containers_and_tasks`** - Filter-only query (new capability)
6. Empty query - Returns empty vec

### Code Review Validation

The pragmatic-code-reviewer agent validated this approach:
- ✅ No security concerns (SQL injection risk assessed and cleared)
- ✅ Good separation of concerns (helper function)
- ✅ Comprehensive test coverage (6 Rust tests, 8 frontend tests)
- ✅ Clear documentation with inline SQL examples
- ✅ Performance acceptable for expected scale (<10k nodes)

## Consequences

**Positive:**

- Simple, maintainable implementation
- Efficient SQL-level filtering
- Works across all existing query types without duplication
- Clear API with explicit intent
- Comprehensive test coverage ensures correctness
- No breaking changes to existing code

**Negative:**

- Hardcoded filter logic limits flexibility
- Future filtering needs (if any) might require refactoring
- Filter cannot be combined with arbitrary conditions (e.g., "containers created after date X")

**Mitigation:**

- Current requirement is specific and unlikely to change
- If future needs arise, can refactor to filter builder pattern
- YAGNI principle: Don't add complexity for hypothetical requirements

## Test Coverage

**Rust Unit Tests (6 tests):**
- `basic_filter()` - Verifies containers and tasks included, text children excluded
- `content_contains_with_filter()` - Tests filter with full-text search
- `mentioned_by_with_filter()` - Tests filter with mention queries
- `node_type_with_filter()` - Tests filter with type queries (tasks always included)
- `default_behavior()` - Verifies filter defaults to `false` (no filtering)
- Filter-only query support (filter as standalone parameter)

**Frontend Unit Tests (8 tests):**
- Filter application in `searchNodes()`
- Caching behavior with filter
- Error handling
- Query parameter combinations
- minQueryLength validation

## Future Considerations

If filtering requirements expand beyond the current use case, consider:

1. **Filter Builder Pattern**: Composable filter system for complex conditions
2. **Predicate Functions**: User-provided filter callbacks
3. **SQL WHERE Clause Builder**: Parameterized SQL generation library

However, implement only when actual requirements emerge (avoid speculative complexity).

## References

- PR: Mention autocomplete filter implementation
- Code Review: `pragmatic-code-reviewer` comprehensive analysis
- Implementation: `packages/core/src/services/node_service.rs:1104-1114`
- Tests: `packages/core/src/services/node_service.rs:2828-3104`
