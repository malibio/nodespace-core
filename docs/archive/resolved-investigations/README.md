# Resolved Investigations Archive

This directory contains investigation and handoff documents for issues that have been **resolved and closed**. These documents are preserved for historical reference and to document the investigation process, but the issues themselves are no longer active.

---

## Archived Investigation Documents

### 1. `TEST-STATE-ANALYSIS.md`
**Original Issue**: #409 - Achieve 100% test pass rate
**Status**: ✅ RESOLVED
**Resolution Date**: November 2025
**Resolution**: Fixed in PR #410 - "Fix 19 frontend test failures by adding database mode guards"
**Final State**:
- 19 frontend HTTP integration tests now properly skip in in-memory mode
- Tests pass 100% in appropriate test modes
- All test suites stable

**Key Learnings**:
- Integration tests requiring HTTP backend must use `skipIf(!shouldUseDatabase())` guards
- Test mode separation (in-memory vs database) critical for fast TDD workflow
- Document which tests require specific test modes

---

### 2. `threads-4-investigation-handoff.md`
**Original Issue**: #411 - Concurrent test execution reliability
**Status**: ✅ RESOLVED
**Resolution Date**: November 5, 2025
**Resolution**: Pragmatic solution - Accept threads=2 configuration (99% reliability)
**Final State**:
- threads=2: 99/100 consecutive runs passed (99% reliability)
- threads=4: 30% success rate (deemed not worth investigation cost)
- Production-ready with minimal performance trade-off

**Key Learnings**:
- Pragmatic solutions beat perfect ones (99% reliable >> uncertain 100% fix)
- Always validate empirically - don't trust claimed success rates without reproduction
- libsql concurrency limits + OCC contention = complex issue
- Document investigation paths for future reference

**Implementation**: `package.json` configured with `--test-threads=2`

---

### 3. `test-suite-concurrency-investigation.md`
**Original Issue**: #398 - SIGABRT/SIGSEGV crashes during test execution
**Status**: ✅ RESOLVED
**Resolution Date**: November 2025
**Resolution**: Fixed in PR #408 - Two bugs identified and fixed
**Bugs Fixed**:
1. **PRAGMA busy_timeout using wrong method** - Changed from `execute()` to `query()` pattern
2. **Test setup bypassing concurrency safety** - Updated to use `connect_with_timeout()`

**Key Learnings**:
- PRAGMA statements return rows and must use `query()` not `execute()`
- Test setup must follow same concurrency patterns as production code
- SQLite busy_timeout configuration critical for concurrent database access
- Individual test passing ≠ concurrent test passing (race conditions only visible under load)

---

## Why Archive These Documents?

These investigation documents served their purpose:
- ✅ Issues identified and root causes documented
- ✅ Fixes implemented and merged
- ✅ Tests now passing consistently
- ✅ Lessons learned captured

They are preserved for:
- **Historical reference** - Understanding past debugging processes
- **Learning resource** - Documented investigation techniques
- **Future troubleshooting** - Reference if similar issues occur
- **Onboarding** - Show how complex issues were approached

---

## Active Troubleshooting

For **current/active** issues, see:
- `/docs/troubleshooting/` - Active troubleshooting documents
- **Example**: `backspace-node-combination-issue.md` (still pending implementation)

---

## Related Issues (All Closed)

- #398 - SIGABRT/SIGTRAP crashes (✅ Fixed in #408)
- #409 - 100% test pass rate (✅ Fixed in #410)
- #411 - Concurrent test reliability (✅ Fixed with threads=2 config)
- #412 - Rust test race conditions (✅ Fixed in #415)
- #415 - 3 Rust test failures (✅ Fixed)

---

**Archive Date**: January 21, 2025
**Archived By**: Claude Code (Documentation Organization Task)
