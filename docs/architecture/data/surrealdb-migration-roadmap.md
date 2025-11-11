# SurrealDB Migration Roadmap - Epic #461

> **🚧 Current Migration Phase**: **Phase 0→1 Transition**
>
> NodeSpace is migrating from Turso (Phase 0 baseline) to a hybrid architecture (SurrealDB for desktop, Turso for mobile). We are currently between phases:
> - ✅ **Phase 0**: Turso baseline established - **COMPLETE**
> - 🚧 **Phase 1**: NodeStore abstraction - **IN PROGRESS** (Issues #462-464)
> - 📋 **Phase 2-4**: SurrealDB implementation, testing, deployment - **PLANNED**
>
> **Quick Links**:
> - [Phase 1 Details](#phase-1-abstraction-layer-current---epic-461)
> - [Phase 1 Architecture](node-store-abstraction.md)
> - [Migration Guide](surrealdb-migration-guide.md)
> - [GitHub Epic #461](https://github.com/malibio/nodespace-core/issues/461)

## Overview

This document provides the complete roadmap for migrating NodeSpace from Turso to a hybrid database architecture (SurrealDB for desktop, Turso for mobile).

**Legacy Status Note**: Phase 1 Planning (Awaiting Approval) - Updated status above reflects current progress

## Migration Phases

### Phase 1: Abstraction Layer (Current - Epic #461)

**Timeline**: 2 weeks (14 days)

**Scope**:
- Issue #462: Create NodeStore Trait Abstraction Layer
- Issue #463: Implement TursoStore Wrapper

**Deliverables**:
- ✅ NodeStore trait definition (23 methods)
- ✅ TursoStore wrapper (delegates to DatabaseService)
- ✅ Refactored NodeService (uses trait abstraction)
- ✅ Zero test regressions (100% pass rate)
- ✅ Performance overhead <5%
- ✅ Comprehensive documentation

**Branch**: `feature/issue-461-surrealdb-hybrid-migration`

**Success Criteria**:
- All existing tests pass
- Performance benchmarks within limits
- Architecture approved by senior stakeholders

**Prerequisites for Phase 2**:
- Phase 1 merged to main
- Stable in production for 1 month
- Zero critical issues reported

---

### Phase 2: SurrealDB Implementation (Future - Epic #467)

**Timeline**: 2 weeks (14 days)

**Scope**:
- Issue #464: Implement NodeStore Trait for SurrealDB Backend
- SurrealStore implementation using PoC code (Epic #460)
- Direct replacement of TursoStore with SurrealStore (no feature flags)

**Deliverables**:
- ✅ SurrealStore implementation (all 22 trait methods)
- ✅ Test suite passes with SurrealStore backend
- ✅ Performance benchmarks (Turso baseline vs SurrealDB)
- ✅ Migration tool (optional - if needed for data migration)

**Branch**: `feature/epic-467-surrealdb-implementation`

**Success Criteria**:
- All tests pass with SurrealStore backend
- SurrealDB performance meets/exceeds PoC benchmarks
- Zero data loss during backend switch
- Clean cutover from Turso to SurrealDB (git revert as escape hatch)

**Prerequisites for Phase 3**:
- Phase 2 merged to main
- SurrealDB backend stable in production
- Performance validated equivalent to PoC benchmarks

---

### Phase 3: A/B Testing Framework (Future - Epic #468)

**Timeline**: 1 week (7 days)

**Scope**:
- Issue #465: Implement A/B Testing Framework for Backend Comparison
- Runtime backend switching
- Performance monitoring dashboard

**Deliverables**:
- ✅ A/B testing infrastructure
- ✅ Performance comparison metrics
- ✅ Automated regression testing
- ✅ Monitoring dashboard (optional)

**Branch**: `feature/epic-468-ab-testing`

**Success Criteria**:
- Can switch backends at runtime
- Performance metrics collected automatically
- Clear comparison data (latency, throughput, errors)
- Automated alerts for performance degradation

**Prerequisites for Phase 4**:
- Phase 3 merged to main
- A/B testing validates performance equivalence
- No critical issues discovered

---

### Phase 4: Gradual Rollout (Future - Epic #469)

**Timeline**: 1 week (7 days)

**Scope**:
- Issue #466: Implement Gradual Rollout System for SurrealDB Backend
- Percentage-based rollout (10% → 25% → 50% → 100%)
- Rollback mechanism

**Deliverables**:
- ✅ Rollout configuration system
- ✅ User cohort assignment (by percentage)
- ✅ Rollback mechanism (instant revert)
- ✅ User feedback collection
- ✅ Migration complete (100% SurrealDB desktop)

**Branch**: `feature/epic-469-gradual-rollout`

**Success Criteria**:
- Smooth rollout without incidents
- User feedback positive (no critical issues)
- Performance stable across all cohorts
- Zero data loss or corruption
- Turso deprecated for desktop (mobile still uses Turso)

**Rollout Schedule**:
- Week 1: 10% rollout (early adopters)
- Week 2: 25% rollout (if no issues)
- Week 3: 50% rollout (if stable)
- Week 4: 100% rollout (full migration)

---

## Timeline Summary

| Phase | Epic | Duration | Prerequisites | Status |
|-------|------|----------|---------------|--------|
| **Phase 1** | #461 | 2 weeks | PoC complete (#460) | **IN PROGRESS** |
| **Phase 2** | #467 | 2 weeks | Phase 1 stable (1 month) | Not Started |
| **Phase 3** | #468 | 1 week | Phase 2 complete | Not Started |
| **Phase 4** | #469 | 1 week | Phase 3 validates equivalence | Not Started |

**Total Duration**: 6 weeks (from Phase 1 start to 100% rollout)

**Critical Path**: Phase 1 → 1 month stability → Phase 2 → Phase 3 → Phase 4

---

## Risk Assessment by Phase

### Phase 1 Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Test regressions | HIGH | Run tests after each method extraction |
| Performance degradation | MEDIUM | Benchmark before/after, <5% overhead target |
| SQL extraction errors | MEDIUM | Extract one method at a time, test individually |

**Overall Risk**: **LOW** - Proven refactoring pattern

### Phase 2 Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| SurrealDB API instability | MEDIUM | Pin to specific version, monitor releases |
| Performance worse than PoC | HIGH | Validate benchmarks before merging |
| Data migration issues | LOW | No migration needed (early dev phase) |
| Binary size increase | LOW | PoC validated 10MB (within limits) |

**Overall Risk**: **MEDIUM** - New backend introduces unknowns

### Phase 3 Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| A/B testing overhead | LOW | Minimal code, mostly monitoring |
| False positives in comparison | MEDIUM | Statistical validation, multiple runs |

**Overall Risk**: **LOW** - Validation layer, low complexity

### Phase 4 Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Rollout causes user issues | HIGH | Gradual rollout with instant rollback |
| Performance degradation at scale | MEDIUM | Monitor metrics, halt rollout if issues |
| Data corruption during switch | CRITICAL | Extensive testing in Phase 2/3 |

**Overall Risk**: **MEDIUM** - User-facing changes, but validated in earlier phases

---

## Decision Points

### Decision Point 1: After Phase 1 (2 weeks)

**Question**: Proceed with SurrealDB implementation?

**Evaluate**:
- ✅ Abstraction layer stable? (1 month in production)
- ✅ Zero critical issues?
- ✅ Performance acceptable? (<5% overhead)

**Outcomes**:
- **YES**: Proceed to Phase 2 (Epic #467)
- **NO**: Refine abstraction layer, defer SurrealDB

---

### Decision Point 2: After Phase 2 (4 weeks total)

**Question**: Proceed with A/B testing?

**Evaluate**:
- ✅ SurrealDB implementation complete?
- ✅ Both backends working?
- ✅ Performance benchmarks validate PoC results?

**Outcomes**:
- **YES**: Proceed to Phase 3 (Epic #468)
- **NO**: Fix performance issues, iterate on SurrealDB implementation

---

### Decision Point 3: After Phase 3 (5 weeks total)

**Question**: Proceed with gradual rollout?

**Evaluate**:
- ✅ A/B testing shows equivalence? (<2% performance delta)
- ✅ No critical issues discovered?
- ✅ Automated monitoring in place?

**Outcomes**:
- **YES**: Proceed to Phase 4 (Epic #469)
- **NO**: Address issues, extend A/B testing period

---

### Decision Point 4: During Phase 4 Rollout

**Question at each milestone**: Continue rollout or rollback?

**10% Rollout** (Week 1):
- ✅ No crashes or data loss?
- ✅ Performance stable?
- ✅ User feedback positive?

**25% Rollout** (Week 2):
- ✅ Scale issues detected?
- ✅ Error rates normal?

**50% Rollout** (Week 3):
- ✅ Performance at scale?
- ✅ No unexpected issues?

**100% Rollout** (Week 4):
- ✅ Full migration successful?
- ✅ Turso deprecated for desktop?

**Outcomes at each milestone**:
- **CONTINUE**: Proceed to next percentage
- **PAUSE**: Hold at current percentage, investigate
- **ROLLBACK**: Instant revert to Turso, analyze issues

---

## Rollback Plan

### Phase 1 Rollback (Abstraction Layer)

**Trigger**: Critical performance regression or test failures

**Process**:
1. Use `git revert` to undo abstraction layer commits
2. Return to direct DatabaseService usage (no trait abstraction)
3. Hot-fix release if in production
4. Analyze root cause and refine approach

**Recovery Time**: <1 hour (git revert + rebuild)

**Why No Feature Flag**:
- Early development phase - acceptable to revert via git
- Simpler implementation (no conditional compilation)
- Clean rollback without maintaining dual code paths
- If abstraction fails, we likely need to redesign anyway

---

### Phase 2 Rollback (SurrealDB Implementation)

**Trigger**: SurrealDB fails validation or causes issues

**Process**:
1. Use `git revert` to undo SurrealStore implementation
2. Return to TursoStore implementation
3. Analyze SurrealDB issues
4. Re-evaluate SurrealDB viability

**Recovery Time**: <1 hour (git revert + rebuild)

**Why No Feature Flag**:
- Early development phase - limited production users
- Trait abstraction makes switching backend simple (change one line)
- If SurrealDB fundamentally doesn't work, we need to fix it, not toggle it
- Simpler codebase without conditional compilation

---

### Phase 3 Rollback (A/B Testing)

**Trigger**: Comparison shows unacceptable performance delta (>2%)

**Process**:
1. Halt A/B testing
2. All users default to Turso
3. Investigate SurrealDB performance issues
4. Iterate on Phase 2 improvements

**Recovery Time**: Immediate (stop A/B test, keep Turso)

---

### Phase 4 Rollback (Gradual Rollout)

**Trigger**: User issues, data loss, or critical performance problems

**Process**:
1. **Instant rollback**: Configuration change reverts all users to Turso
2. Emergency hotfix if data integrity issues
3. Post-mortem analysis
4. Fix issues before resuming rollout

**Recovery Time**: <5 minutes (configuration flag flip)

**Data Preservation**: No data loss (both backends keep full history during rollout)

---

## Success Metrics

### Phase 1 Success Metrics

- ✅ Zero new test failures
- ✅ Performance overhead <5%
- ✅ All 23 trait methods implemented
- ✅ Documentation complete

### Phase 2 Success Metrics

- ✅ SurrealDB startup: <100ms (PoC was 52ms)
- ✅ 100K query: <200ms (PoC was 104ms)
- ✅ Deep pagination: <20ms (PoC was 8.3ms)
- ✅ Binary size: <15MB (PoC was 10MB)

### Phase 3 Success Metrics

- ✅ Performance delta: <2% between backends
- ✅ A/B test coverage: 1000+ operations
- ✅ Automated monitoring: Real-time metrics
- ✅ Zero false positives in comparison

### Phase 4 Success Metrics

- ✅ 10% rollout: Zero critical issues (Week 1)
- ✅ 25% rollout: Stable performance (Week 2)
- ✅ 50% rollout: Positive user feedback (Week 3)
- ✅ 100% rollout: Full migration complete (Week 4)
- ✅ Desktop fully migrated to SurrealDB
- ✅ Turso deprecated for desktop (mobile still uses Turso)

---

## Mobile Strategy (Deferred)

**Current Decision**: Keep Turso for mobile (Phase 1-4 are desktop-only)

**Rationale**:
- SurrealKV (mobile backend) not production-ready
- Turso mobile integration stable and working
- Revisit in 6 months after desktop migration stable

**Future Evaluation** (6 months post-Phase 4):
- Assess SurrealKV production readiness
- Evaluate mobile-specific requirements
- Consider if unified backend (SurrealDB) worth complexity

**Options**:
1. **Stay with Turso mobile** - Keep hybrid architecture
2. **Migrate to SurrealDB mobile** - If SurrealKV matures
3. **Use SurrealDB cloud** - If mobile sync critical

**Decision Timeline**: Q3 2025 (after desktop migration complete)

---

## Communication Plan

### Stakeholder Updates

**Weekly Status Updates** (during active phases):
- Progress on current phase
- Blockers or risks identified
- Metrics and benchmarks
- Next week's goals

**Decision Point Communications**:
- Detailed analysis at each decision point
- Recommendation with confidence level
- Risk assessment and mitigation plan
- Stakeholder approval required before proceeding

### User Communications

**Phase 1-3**: No user communication (internal refactoring)

**Phase 4 Rollout**:
- **10% rollout**: Internal announcement (dev team, early adopters)
- **25% rollout**: Beta user notification (if applicable)
- **50% rollout**: General user notification (what's changing, benefits)
- **100% rollout**: Migration complete announcement

**Messaging Focus**:
- Performance improvements (faster queries, better scale)
- No user action required (seamless migration)
- Support channels available for issues

---

## Current Status

**Phase**: Phase 1 (Abstraction Layer)
**Epic**: #461
**Branch**: `feature/issue-461-surrealdb-hybrid-migration`
**Status**: Planning (Awaiting Approval)

**Next Steps**:
1. Architectural review (this document)
2. Stakeholder approval
3. Begin Phase 1 implementation
4. 2-week implementation cycle
5. Deploy to production
6. 1-month stability validation
7. Decision Point 1: Proceed to Phase 2?

**Estimated Completion** (if all phases proceed):
- Phase 1 complete: 2 weeks from approval
- Phase 2 start: +1 month (stability period)
- Phase 2 complete: +2 weeks
- Phase 3 complete: +1 week
- Phase 4 complete: +1 week (gradual rollout)

**Total Timeline**: ~8 weeks from approval to 100% desktop migration (excluding 1-month stability period)

---

## References

- Epic #460: SurrealDB Feasibility PoC (completed)
- Epic #461: SurrealDB Migration - Phase 1 (current)
- Epic #467: SurrealDB Implementation - Phase 2 (future)
- Epic #468: A/B Testing Framework - Phase 3 (future)
- Epic #469: Gradual Rollout - Phase 4 (future)
- `/docs/architecture/data/node-store-abstraction.md` - Architecture design
- `/docs/architecture/data/surrealdb-migration-guide.md` - Implementation guide
