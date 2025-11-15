# PR Review Guidelines

> ## 🚨 **UNIVERSAL REVIEW PROCESS**
> 
> **This applies to ALL reviewers**: AI agents, human engineers, tech leads, and architects.
> 
> **NO EXCEPTIONS**: Same review standards regardless of reviewer type.

## 🚨 MANDATORY FIRST STEP - Code Quality Verification

**BEFORE ANY OTHER REVIEW ACTIVITIES:**

### 1. Quality Gates Verification (BLOCKING)
```bash
# Reviewer MUST run this first:
bun run quality:fix

# Verify ZERO errors in output:
# ✅ ESLint: 0 errors, warnings acceptable  
# ✅ Prettier: All files formatted
# ✅ TypeScript: No compilation errors
# ✅ Svelte Check: No component errors
```

### 2. Automatic Rejection Criteria
**If ANY of these conditions exist, REJECT PR immediately:**

- [ ] **Linting errors present** - ESLint shows any errors
- [ ] **TypeScript compilation fails** - Any compilation errors
- [ ] **Svelte component errors** - Any component validation failures
- [ ] **Lint suppressions found** - Any `eslint-disable` or similar suppressions
- [ ] **Unsafe patterns present** - Use of `{@html}`, excessive `any` types

### 3. Quality Violation Response
```markdown
**AUTOMATIC REJECTION - Quality Gate Failure**

This PR has been automatically rejected due to quality gate violations:

**Linting Errors Found:**
- [List specific ESLint errors]

**TypeScript Errors:**
- [List specific compilation errors]

**Process Violation:** Creating PR with linting errors is a process violation requiring acknowledgment.

**Required Actions:**
1. Run `bun run quality:fix` locally
2. Fix all reported issues properly (no suppressions)
3. Commit fixes and re-request review
4. Acknowledge process violation in PR comments

**Note:** This applies equally to AI agents and human engineers per our universal quality standards.
```

## ✅ COMPREHENSIVE REVIEW PROCESS

**Only proceed after quality gates pass:**

### Issue Requirements Review (SECOND PRIORITY)

- [ ] **All acceptance criteria met**: Each checkbox in the original issue is verified and completed
- [ ] **Original requirements satisfied**: Implementation addresses the specific goals stated in the issue  
- [ ] **Scope alignment**: No feature creep - implementation stays within defined scope
- [ ] **User value delivered**: The implemented solution provides the intended user benefit
- [ ] **Dependencies resolved**: Any stated dependencies are properly addressed
- [ ] **Success criteria validated**: Implementation can be demonstrated to work as specified

### Architecture Review

- [ ] **Follows established patterns**: Svelte component patterns, project structure
- [ ] **Interface design**: Clean, extensible, and well-defined APIs
- [ ] **Dependencies**: Minimal, well-justified, and properly managed
- [ ] **Component sizing**: Appropriately focused and not overly complex
- [ ] **Self-contained approach**: Feature works independently with proper mocks

### Implementation Review

- [ ] **Code Quality**: All quality gates passed (verified in step 1)
- [ ] **TypeScript**: Strict type checking with no compilation errors
- [ ] **Svelte**: Reactivity used correctly, no component errors
- [ ] **Naming Conventions**: Follows [Identifier Naming Conventions](../standards/identifier-naming-conventions.md)
  - [ ] Node interfaces use `nodeType` not `type` (ESLint rule enforced)
  - [ ] Variables use camelCase (TS) or snake_case (Rust)
  - [ ] IDs suffixed with `Id` / `_id`
- [ ] **Accessibility**: WCAG compliance, proper ARIA attributes where needed
- [ ] **Error Handling**: Comprehensive error boundaries and validation
- [ ] **Performance**: No obvious performance bottlenecks or inefficiencies

### Testing Review

- [ ] **Unit Tests**: Components and functions adequately tested
- [ ] **Integration Tests**: Key workflows function with mock data
- [ ] **Test Quality**: Tests are meaningful and provide good coverage
- [ ] **Mock Implementation**: Proper use of mocks for independent development

### Documentation Review

- [ ] **Code Comments**: Complex logic appropriately documented
- [ ] **Component Documentation**: Usage examples and prop descriptions
- [ ] **Architecture Updates**: Relevant docs updated for architectural changes

## PR Review Decision Matrix

### ✅ APPROVE AND MERGE
**When ALL conditions are met:**
- Quality gates pass completely
- All acceptance criteria satisfied  
- Code follows established patterns
- Comprehensive testing present
- Documentation updated as needed

**Action:** Immediately approve and merge the PR

### 🔄 REQUEST CHANGES
**When ANY condition fails:**
- Requirements not fully met
- Architecture concerns present
- Implementation issues found
- Testing inadequate
- Documentation missing

**Action:** Request changes with specific, actionable feedback

### ❌ AUTOMATIC REJECTION
**For quality gate failures:**
- Any linting errors present
- TypeScript compilation failures
- Unsafe patterns or suppressions
- Process violations

**Action:** Reject immediately with quality violation template

## Review Templates

### Approval Template
```markdown
**✅ APPROVED - Ready to Merge**

**Quality Verification:** ✅ All quality gates passed (`bun run quality:fix` shows zero errors)

**Requirements Review:** ✅ All acceptance criteria from issue #[number] are met

**Implementation Review:** ✅ Code follows established patterns and standards

**Recommendation:** Merge immediately.

---
Reviewed by: [AI Agent/Human Name]
Review Date: [Date]
```

### Change Request Template
```markdown
**🔄 CHANGES REQUESTED**

**Quality Verification:** ✅ Quality gates passed

**Issues Found:**

**Requirements:**
- [ ] [Specific unmet acceptance criteria]

**Implementation:**
- [ ] [Specific code issues with suggestions]

**Testing:**
- [ ] [Testing gaps or improvements needed]

**Please address these items and re-request review.**

---
Reviewed by: [AI Agent/Human Name]
Review Date: [Date]
```

## Advanced Review Scenarios

### Complex Features
- Use `senior-architect-reviewer` agent for architectural guidance
- Focus on system design and integration patterns
- Verify scalability and maintainability

### AI/ML Integration
- Use `ai-ml-engineer` agent for specialized review
- Verify model integration patterns
- Check performance implications

### UI/UX Changes
- Use `ux-design-expert` agent for design review
- Verify accessibility compliance
- Check design system adherence

## Review Quality Standards

### For AI Agent Reviewers
- Must follow same quality gates as human reviewers
- Required to run `bun run quality:fix` before review
- Must provide specific, actionable feedback
- Held accountable for merged quality violations

### For Human Reviewers
- Same standards and requirements as AI agents
- No shortcuts or exceptions allowed
- Required to acknowledge review completeness
- Responsible for quality gate verification

---

**Remember**: The review process ensures quality regardless of who implements or reviews. Every PR must meet the same standards whether created by AI agents or human engineers.