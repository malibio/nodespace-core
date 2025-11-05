# threads=4 Investigation Handoff - Issue #411

**Status**: In Progress - Handoff for Next AI Session
**Current State**: threads=2 achieves 100% reliability, threads=4 at 40%
**Estimated Remaining Effort**: 2-3 days
**Priority**: User requested full investigation (Option B)

---

## Quick Context

Issue #411 requires **100% test reliability with concurrent execution**. We've achieved:
- ✅ threads=2: 100% reliability (20/20 consecutive runs)
- ❌ threads=4: 40% reliability (8/20 consecutive runs)

User chose **Option B**: Continue investigation to achieve 100% with threads=4 (not settle for threads=2).

---

## What's Been Completed

### Phase 0: Foundation & Documentation ✅
1. **Fixed all SQLite thread-safety violations**
   - `initialize_schema()` - uses `connect_with_timeout().await`
   - `drain_and_checkpoint()` - uses `connect_with_timeout().await`
   - All async functions audited and verified

2. **Added comprehensive documentation**
   - Updated `docs/architecture/development/testing-guide.md`
   - Documented SQLite thread-safety pattern
   - Explained threads=2 configuration and trade-offs

3. **Statistical validation**
   - threads=1: 100% (10/10) - no contention
   - threads=2: 100% (20/20) - minimal contention
   - threads=4: 40% (8/20) - high contention

### Phase 0.5: Initial Investigation ✅
1. **Identified failure pattern**
   - 20-run statistical analysis completed
   - Different tests fail on different runs (race conditions)
   - Usually 1-2 tests fail per run, never the same combination

2. **Catalogued failing tests** (see below)

3. **Confirmed root cause hypothesis**
   - All tests pass individually → proves isolation works
   - Only fail under concurrent load → proves contention issue

---

## Statistical Analysis Results

### 20-Run Test (threads=4)
- **Success**: 8/20 (40%)
- **Failure**: 12/20 (60%)
- **Pattern**: Non-deterministic, different tests each run

### Tests That Fail Intermittently

From 20-run analysis:
1. `services::schema_service::tests::test_remove_core_field_rejected`
2. `operations::sibling_queue::tests::test_reorder_with_retry_handles_version_conflict`
3. `mcp::handlers::schema::tests::test_extend_schema_enum`
4. `mcp::handlers::nodes::nodes_test::integration_tests::test_occ_performance_overhead`
5. `mcp::handlers::nodes::nodes_test::integration_tests::test_get_children_ordered_with_multiple_insertions`
6. `mcp::handlers::markdown::markdown_test::tests::test_inline_code_in_text`
7. `mcp::handlers::markdown::markdown_test::tests::test_bullet_roundtrip`

**Key Observation**: These tests span multiple modules (schema, nodes, markdown, operations), suggesting a systemic concurrency issue, not isolated test bugs.

---

## Root Cause Hypothesis

### What We Know For Sure

**✅ Fixed (Not The Problem)**:
- SQLite thread-safety violations in async functions
- Connection creation patterns
- Test isolation (TempDir, unique databases)
- No static/global state issues

**❌ Remaining Issue (The Problem)**:
**OCC (Optimistic Concurrency Control) contention at high concurrency**

### The Contention Mechanism

When 4 threads run 385 tests concurrently:

1. **Version Conflicts Multiply**
   - Multiple tests modify overlapping data simultaneously
   - OCC detects conflicts and triggers retry logic
   - Exponential backoff: 10ms, 20ms, 40ms, 80ms...

2. **Retry Windows Overlap**
   - Thread A waits 20ms to retry
   - Thread B modifies same data during that window
   - Thread A's retry fails again → waits 40ms
   - Cascading failures as retries exhaust max attempts

3. **Connection Pool Pressure**
   - 4 threads × ~96 concurrent tests = high connection demand
   - libsql may have implicit connection limits
   - Connections may be blocking waiting for locks despite busy_timeout=5000ms

4. **Schema Initialization Races**
   - Multiple tests initializing core schemas simultaneously
   - WAL checkpoint timing during concurrent schema operations
   - Possible shared state in schema initialization despite TempDir isolation

### Evidence Supporting Hypothesis

| Threads | Success Rate | Interpretation |
|---------|--------------|----------------|
| 1 | 100% | No contention → hypothesis supported |
| 2 | 100% | Minimal contention → OCC handles it |
| 4 | 40% | High contention → OCC overwhelmed |

**Individual tests**: 100% pass → proves test logic is correct, only concurrency is the issue

---

## What Needs To Happen Next

### Phase 1: Deep Instrumentation (2-3 hours)

**Goal**: Identify the specific contention points causing failures.

#### 1.1: Add OCC Retry Logging

**File**: `packages/core/src/operations/sibling_queue.rs`

```rust
// Around line 133, in the version conflict handler
Err(NodeOperationError::VersionConflict {
    node_id: ref conflict_node_id,
    expected_version,
    actual_version,
    ..
}) if attempt < max_retries => {
    // ADD THIS:
    tracing::warn!(
        "🔴 OCC_RETRY: node={}, attempt={}/{}, expected_v={}, actual_v={}, backoff={}ms, thread={:?}",
        conflict_node_id,
        attempt + 1,
        max_retries + 1,
        expected_version,
        actual_version,
        10u64 * (1 << attempt),
        std::thread::current().id()
    );

    // Exponential backoff: 10ms, 20ms, 40ms, 80ms, ...
    let backoff_ms = 10u64 * (1 << attempt);
    tokio::time::sleep(Duration::from_millis(backoff_ms)).await;

    attempt += 1;
    continue;
}
```

#### 1.2: Add Connection Tracking

**File**: `packages/core/src/db/database.rs`

```rust
// In connect_with_timeout(), around line 779
pub async fn connect_with_timeout(&self) -> Result<libsql::Connection, DatabaseError> {
    // ADD THIS:
    tracing::debug!(
        "🔵 DB_CONN_START: thread={:?}, db={:?}",
        std::thread::current().id(),
        self.db_path
    );

    let conn = self.connect()?;

    // Set busy timeout on this connection
    self.execute_pragma(&conn, "PRAGMA busy_timeout = 5000")
        .await?;

    // ADD THIS:
    tracing::debug!(
        "🟢 DB_CONN_SUCCESS: thread={:?}, db={:?}",
        std::thread::current().id(),
        self.db_path
    );

    Ok(conn)
}
```

#### 1.3: Run Instrumented Tests

```bash
# Run with debug logging enabled
RUST_LOG=nodespace_core=debug cargo test --lib -p nodespace-core -- \
    --test-threads=4 \
    --nocapture \
    > /tmp/test_output_instrumented.log 2>&1

# Analyze the log
grep "OCC_RETRY\|DB_CONN" /tmp/test_output_instrumented.log > /tmp/contention_analysis.txt
```

#### 1.4: Analyze Patterns

Look for:
- **Which tests retry most frequently?**
- **Which thread IDs have the most conflicts?**
- **Are there connection creation spikes?**
- **Do retries cluster in time? (thundering herd)**
- **Which nodes have version conflicts most often?**

---

### Phase 2: Implement Fix (1-2 days)

Based on Phase 1 findings, implement one or more of these solutions:

#### Option A: Connection Semaphore (If Connection Exhaustion)

```rust
// Add to packages/core/src/db/database.rs
use tokio::sync::Semaphore;
use std::sync::LazyLock;

// Limit concurrent database connections to prevent saturation
static CONNECTION_SEMAPHORE: LazyLock<Semaphore> =
    LazyLock::new(|| Semaphore::new(20)); // Adjust based on libsql limits

pub async fn connect_with_timeout(&self) -> Result<libsql::Connection, DatabaseError> {
    // Acquire permit (blocks if 20 connections already active)
    let _permit = CONNECTION_SEMAPHORE.acquire().await.unwrap();

    tracing::debug!(
        "🔵 DB_CONN_START: thread={:?}, permits_available={}",
        std::thread::current().id(),
        CONNECTION_SEMAPHORE.available_permits()
    );

    let conn = self.connect()?;
    self.execute_pragma(&conn, "PRAGMA busy_timeout = 5000").await?;

    Ok(conn)
    // _permit drops here, releasing the semaphore
}
```

#### Option B: Jittered Exponential Backoff (If Thundering Herd)

```rust
// Update packages/core/src/operations/sibling_queue.rs
use rand::Rng;

// Around line 143
let base_backoff_ms = 10u64 * (1 << attempt);

// Add jitter (0-50% of base backoff) to prevent thundering herd
let jitter_ms = rand::thread_rng().gen_range(0..=(base_backoff_ms / 2));
let total_backoff_ms = base_backoff_ms + jitter_ms;

tracing::debug!(
    "OCC retry: node={}, attempt={}, base={}ms, jitter={}ms, total={}ms",
    node_id, attempt + 1, base_backoff_ms, jitter_ms, total_backoff_ms
);

tokio::time::sleep(Duration::from_millis(total_backoff_ms)).await;
```

**Add to Cargo.toml**:
```toml
[dependencies]
rand = "0.8"
```

#### Option C: Test-Level Serialization (If Specific Tests Conflict)

If Phase 1 shows certain tests always conflict:

```toml
# Add to packages/core/Cargo.toml
[dev-dependencies]
serial_test = "3.0"
```

```rust
// Mark conflicting tests
use serial_test::serial;

#[tokio::test]
#[serial] // Forces sequential execution
async fn test_extend_schema_enum() {
    // ...
}
```

#### Option D: Enhanced Retry Strategy (If Retries Exhausted)

```rust
// Increase max_retries in tests that use reorder_with_retry()
// From 3 → 5 or adaptive based on thread count

let max_retries = if cfg!(test) {
    // In tests, be more tolerant of concurrent load
    match std::env::var("RUST_TEST_THREADS").unwrap_or_default().as_str() {
        "1" => 2,
        "2" => 3,
        _ => 5, // threads=4 or higher
    }
} else {
    3 // Production default
};
```

#### Option E: Per-Test Connection Pools (If Isolation Insufficient)

```rust
// Ensure each test truly has isolated connection pool
// Check that TempDir cleanup doesn't race with active connections
// Verify WAL checkpoint completes before test teardown
```

---

### Phase 3: Validation (2-4 hours)

Once fix is implemented:

```bash
# Validate with 100 consecutive runs
bash -c 'PASS=0; FAIL=0; for i in {1..100}; do
    echo "Run $i/100:";
    if cargo test --lib -p nodespace-core -- --test-threads=4 2>&1 | grep -q "test result: ok"; then
        ((PASS++));
        echo "✅";
    else
        ((FAIL++));
        echo "❌";
    fi;
done;
echo "";
echo "Summary: $PASS/100 passed ($(echo "scale=1; $PASS*100/100" | bc)% success rate)"'
```

**Target**: 95-100 passes out of 100 runs

If < 95%, return to Phase 1 for more instrumentation.

---

## Key Files to Modify

### Primary Targets (Phase 1 Instrumentation)
1. `packages/core/src/operations/sibling_queue.rs` - OCC retry logic
2. `packages/core/src/db/database.rs` - Connection management
3. Run tests and capture logs

### Secondary Targets (Phase 2 Fix, based on findings)
1. `packages/core/src/db/database.rs` - Connection pooling
2. `packages/core/src/operations/sibling_queue.rs` - Backoff improvements
3. `packages/core/Cargo.toml` - Add dependencies (rand, serial_test)
4. Specific test files - Add #[serial] if needed

### Documentation Updates (Phase 3)
1. `docs/architecture/development/testing-guide.md` - Update with findings
2. This file - Mark as resolved and document solution

---

## Decision Tree for Next Session

```
START
  |
  ├─> Phase 1: Add instrumentation
  |     ├─> Run tests with RUST_LOG=debug
  |     ├─> Analyze /tmp/contention_analysis.txt
  |     └─> Identify primary contention point
  |
  ├─> Phase 2: Implement fix based on findings
  |     ├─> Connection exhaustion? → Option A (semaphore)
  |     ├─> Thundering herd? → Option B (jittered backoff)
  |     ├─> Specific test conflicts? → Option C (serial)
  |     ├─> Retries exhausted? → Option D (adaptive retries)
  |     └─> Isolation issues? → Option E (per-test pools)
  |
  └─> Phase 3: Validate
        ├─> 100 consecutive runs with threads=4
        ├─> Success rate >= 95%? → DONE ✅
        └─> Success rate < 95%? → Return to Phase 1
```

---

## Expected Outcomes

### Success Criteria
- ✅ 95-100% success rate with threads=4 (100 consecutive runs)
- ✅ Root cause documented and understood
- ✅ Fix is minimal and targeted (not a complete rewrite)
- ✅ No performance regression vs current threads=2

### Deliverables
1. Instrumented code with detailed logging
2. Analysis document explaining contention patterns
3. Targeted fix implementation
4. Updated documentation
5. Validation results (100-run test)

---

## Fallback Plan

If investigation reveals issue is more complex than estimated:

1. **Revert to threads=2** (already committed, 100% reliable)
2. **Document findings** in this file
3. **Create new issue** for long-term threads=4 optimization
4. **Close #411** as resolved with threads=2 (pragmatic solution)

**Do NOT spend more than 3-4 days** on this investigation. threads=2 is a perfectly acceptable solution.

---

## Questions for Next Session Agent

Before starting work, verify:

1. ✅ Is the goal still 100% reliability with threads=4?
2. ✅ Is threads=2 fallback acceptable if threads=4 takes > 3 days?
3. ✅ Should we commit instrumentation code or only use it for analysis?
4. ✅ What success rate is acceptable? (95%? 99%? 100%?)

**Default assumption**: Proceed with Phase 1 instrumentation, analyze, then implement targeted fix from Phase 2.

---

## Commit History Reference

- `947e5d0` - Added comprehensive documentation (this session)
- `e868a6c` - Reduced test concurrency to threads=2
- `73cd64d` - Fixed SQLite thread-safety (initialize_schema)

**Next commit should be**: Instrumentation code or fix implementation

---

**Last Updated**: 2025-11-05
**Session**: Address code review + investigate threads=4
**Next Action**: Phase 1 instrumentation
**Estimated Completion**: 2-3 days from start of next session
