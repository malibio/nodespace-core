# Issue Workflow - Creation to PR Merge

Complete workflow for GitHub issue management from creation to PR merge, including templates, reference formatting, and quality gates.

## Issue Creation Guidelines

### Issue Reference Formatting

**GitHub Issue Creation Best Practices:**

GitHub provides enhanced rendering for issue references when using specific formatting:

```markdown
## Dependencies
- #15 (blocks this issue)
- #16 (must be completed first)

## Related Issues  
- #4 (parent feature/epic)
```

**Rich GitHub Rendering Triggers:**
- **Dependencies**: Shows checkboxes with issue status and titles
- **Related Issues**: Shows checkboxes with issue status and titles  
- **Issues** (generic): Shows checkboxes with issue status and titles

**Simple References:**
- `#4` in regular text → auto-expands with issue title
- `Issue #4` in regular text → renders as basic hyperlink

**✅ RECOMMENDED:** Use simple `#number` references to avoid duplication:
```markdown
## Related Issues
- #26 (parent)
- #47 (next step)
```

**❌ AVOID:** Verbose references that duplicate GitHub's auto-generated titles:
```markdown
## Related Issues  
- Issue #26 - Hybrid Markdown Rendering System (parent)
- Issue #47 - Hybrid Markdown Rendering implementation (next step)
```

### Issue Section Guidelines

**Dependencies Section Rules:**
- **ONLY GitHub issues** with `#number` references
- **NO file references** or system requirements (move to Technical Specifications)
- **Use bullet points**, not comma-separated lists

```markdown
## Dependencies
- #46 must be completed
- #47 (blocks this work)

# NOT:
- #46, #47, #48 (comma-separated)
- Access to BaseNode.svelte file (non-issue dependency)
```

**Related Issues Section Rules:**
- **ONLY GitHub issues** with `#number` references  
- **NO redundant titles** (GitHub auto-generates these)
- **Use bullet points**, not comma-separated lists

```markdown
## Related Issues
- #26 (parent issue)
- #48 (next)
- #33 (potentially removable)

# NOT:  
- #26, #48, #33 (comma-separated)
- #48 (next - BaseNode Integration Polish) (redundant title)
```

**Technical Specifications Section:**
- **File references** and system requirements go here
- **Use bullet points** for all lists
- **Clear categorization** with subsection headers

```markdown
## Technical Specifications

### **Reference Files**
- **BaseNode.svelte**: `/path/to/file` - Integration target
- **MarkdownRenderer.svelte**: `/path/to/file` - Font size reference

### **System Requirements**  
- **Package Manager**: Bun (required for installation)
- **Bundle Analysis Tools**: webpack-bundle-analyzer for size analysis
```

### Issue Naming Patterns

**Follow Natural, Descriptive Titles (Based on Project Analysis):**

```markdown
# Good Examples from NodeSpace Issues:
- "Enhanced Visual Parent/Child Relationships with Connecting Lines"
- "Epic: ContentEditable Text Editing with Smart Markdown Conversion"
- "Enhancement: Achieve Perfect Review Score (100/100) for ContentEditable System"
- "Integration Testing with Existing Keyboard Handlers"
- "Complete Issue #26: Hybrid Markdown Rendering System"

# Avoid Generic Prefixes:
❌ "[FEATURE] Advanced Node System"
❌ "[BUG] Fix Button Click"
❌ "[ENHANCEMENT] Update Component"

# Use Natural Language Instead:
✅ "Advanced Node Hierarchy Indentation System"
✅ "Fix Button Click Handler in Navigation"
✅ "Enhanced Button Component with Loading States"
```

### Feature Implementation Template

```markdown
# [Descriptive Title Without Prefix]

## Overview
Brief description of the complete feature being implemented.

## Self-Contained Scope
- Component implementation with all required functionality
- Local state management (upgradeable to shared state later)
- Mock data and services where needed for independence
- Comprehensive testing with demo scenarios
- Documentation and usage examples

## Implementation Approach
1. **Component Structure**: Follow existing patterns
2. **State Management**: Start with local state, design for easy upgrade
3. **Data Layer**: Use mocks initially, implement interface for real data
4. **Testing Strategy**: Unit tests + integration tests with mock data
5. **Demo/Documentation**: Working examples and usage guide

## Acceptance Criteria
- [ ] Component renders and functions correctly
- [ ] All user interactions work as expected
- [ ] State persists appropriately (local storage, memory, etc.)
- [ ] Comprehensive test coverage (>90%)
- [ ] Working demo available
- [ ] Documentation complete
- [ ] Code follows project standards

## Technical Implementation Notes
- Patterns to follow: [reference existing components]
- Dependencies to mock: [list external dependencies]
- Interfaces to implement: [define clean interfaces]
- Testing approach: [specific testing strategy]

## Resources & References
- Similar implementations: [file paths]
- Type definitions: [interface files]  
- Testing utilities: [test helper files]
- Design patterns: [architecture docs]

## Definition of Done
- [ ] Feature works end-to-end in isolation
- [ ] All tests pass
- [ ] Code review completed
- [ ] Documentation updated
- [ ] Demo recorded/available
- [ ] Ready for integration with other systems
```

### Timeline and Effort Estimates

**❌ DO NOT include timeline or effort estimates in issue descriptions.**

**Why:**
- Estimates become stale as implementation details change
- Creates false expectations and pressure
- Issues are reference documentation, not project plans
- Time tracking should be done in project management tools, not issue text

**✅ EXCEPTION: Phased Implementation**

Sequential phases ARE acceptable when breaking down large features:

```markdown
## Implementation Approach: Phased Delivery

### Phase 1: Infrastructure Setup
- Set up core configuration
- Create base utilities
- Implement proof-of-concept

### Phase 2: Core Functionality
- Implement main features
- Add error handling
- Write comprehensive tests

### Phase 3: Polish and Documentation
- Performance optimization
- Complete documentation
- Add usage examples
```

**Key Difference:**
- ❌ Bad: "Phase 1: 2-3 hours"
- ✅ Good: "Phase 1: Infrastructure Setup"
- Phases describe WHAT, not HOW LONG

### Self-Contained Issue Requirements

Every issue must include sufficient detail for independent implementation:

**Technical Specifications:**
- Exact file paths and structure
- Specific code patterns to follow
- Design tokens and CSS properties to use
- Component interfaces and props
- Testing requirements and approaches

**Implementation Guidance:**
- Reference existing similar implementations
- Specify design system integration points
- Include example code snippets for complex requirements
- Define clear success criteria

**Resource References:**
- Link to relevant architecture documentation
- Reference existing components to mimic
- Include design system tokens and patterns
- Specify testing utilities and patterns

### Issue Dependency Management

**True Dependencies (use "Dependencies" section):**
- Issue cannot start without dependency completion
- Blocks implementation progress
- Creates sequential workflow

**Feature Relationships (use "Related Issues" section):**
- Part of same epic/feature
- Provides context but doesn't block
- Enables parallel development

### Quality Gates for Issues

Before creating an issue, verify:
- [ ] **Self-contained**: Can be implemented independently
- [ ] **Specific**: Clear technical requirements with examples
- [ ] **Testable**: Measurable acceptance criteria
- [ ] **Valuable**: Delivers user-facing functionality
- [ ] **Scoped**: Focused on single responsibility
- [ ] **Detailed**: Includes all necessary technical specifications

## Issue Assignment and Status Updates

### Mandatory Startup Sequence
**EVERY TEAM MEMBER (AI AGENTS & HUMANS) MUST COMPLETE BEFORE ANY IMPLEMENTATION:**

1. **Create feature branch**: `git checkout -b feature/issue-<number>-brief-description`
2. **Assign issue to self**: `bun run gh:assign <number> "@me"`
3. **Update project status**: `bun run gh:status <number> "In Progress"`
4. **Read issue acceptance criteria** and requirements
5. **Plan self-contained implementation** approach

### GitHub Project Status Management

**GitHub Project Status Field IDs:**
- **Project ID**: `PVT_kwHOADHu9M4A_nxN` (nodespace project)
- **Status Field ID**: `PVTSSF_lAHOADHu9M4A_nxNzgyq13o`

**Status Option IDs:**
- **Todo**: `f75ad846`
- **In Progress**: `47fc9ee4`
- **Waiting for Input**: `db18cb7f`
- **Ready for Review**: `b13f9084`
- **In Review**: `bd055968`
- **Done**: `98236657`
- **Ready to Merge**: `414430c1`

**Status Update Commands:**
```bash
# Start work (Todo → In Progress)
bun run gh:status <number> "In Progress"

# Create PR (In Progress → Ready for Review)  
bun run gh:status <number> "Ready for Review"

# Complete work (Ready for Review → Done)
bun run gh:status <number> "Done"
```

**Practical Example - Complete Workflow:**
```bash
# Working on Issue #25 "Add Search Functionality"
# 1. Start work (Todo → In Progress)
git checkout -b feature/issue-25-search-functionality
bun run gh:assign 25 "@me"
bun run gh:status 25 "In Progress"

# 2. Create PR when ready (In Progress → Ready for Review)  
bun run gh:pr 25

# 3. After PR merge (Ready for Review → Done)
# Status automatically updated to Done when PR is merged
```

## Implementation Process

### Code Quality Gates (MANDATORY - BLOCKING)

**CRITICAL:** Before creating PR, ALL code must pass:
```bash
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

**🚨 LINTING POLICY - UNIVERSAL FOR ALL TEAM MEMBERS (AI AGENTS & HUMANS):**

**ZERO TOLERANCE POLICY:**
- **NO lint suppression allowed anywhere in codebase** - Fix issues properly, don't suppress warnings
- **NO EXCEPTIONS** - All lint warnings and errors must be fixed with proper solutions
- **Use proper TypeScript types** instead of `any` - Type your code correctly
- **Follow Svelte best practices** - Avoid unsafe patterns like `{@html}` 
- **Implement safe alternatives** - Create proper components instead of bypassing safety features

**APPROVED SOLUTIONS FOR COMMON ISSUES:**
- **Markdown rendering**: Use structured parsing components (MarkdownRenderer) instead of `{@html}`
- **Type safety**: Create proper interfaces instead of using `any`
- **DOM APIs**: Add proper type definitions to ESLint globals instead of suppressing warnings
- **Mock objects**: Type mock functions properly with correct interfaces

## PR Creation and Review

### PR Creation Requirements

**🚨 BLOCKING Quality Requirements (CANNOT CREATE PR WITHOUT):**
- [ ] **ESLint**: ZERO errors (warnings acceptable in development)
- [ ] **Prettier**: Code consistently formatted
- [ ] **TypeScript**: ZERO compilation errors, strict type checking passed
- [ ] **Svelte Check**: ZERO component errors or accessibility violations

### PR Review Process

**🚨 MANDATORY FIRST STEP - Linting Verification (ALL REVIEWERS: AI AGENTS & HUMANS):**
- [ ] **Reviewer MUST run**: `bun run quality:fix` before any other review steps
- [ ] **Zero errors confirmed**: ESLint, Prettier, TypeScript, Svelte Check all pass
- [ ] **Automatic rejection**: If any linting errors found, reject PR immediately with specific error list
- [ ] **Implementer accountability**: Failed linting = process violation, require acknowledgment (AI agents & humans)
- [ ] **Universal standards**: Same quality requirements for all team members regardless of human/AI status

**CRITICAL: Issue Requirements Review (SECOND PRIORITY):**
- [ ] **All acceptance criteria met**: Each checkbox in the original issue is verified and completed
- [ ] **Original requirements satisfied**: Implementation addresses the specific goals stated in the issue
- [ ] **Scope alignment**: No feature creep - implementation stays within defined scope
- [ ] **User value delivered**: The implemented solution provides the intended user benefit
- [ ] **Dependencies resolved**: Any stated dependencies are properly addressed
- [ ] **Success criteria validated**: Implementation can be demonstrated to work as specified

**PR Review and Merge (MANDATORY WITH EXPLICIT VERIFICATION):**
1. **🚨 FIRST - Verify Code Quality Gates (BLOCKING)**: Run `bun run quality:fix` and confirm ZERO errors
2. **Review against original issue requirements FIRST** - verify all acceptance criteria are met
3. **Conduct comprehensive technical review** (use senior-architect-reviewer for complex changes)  
4. **If review shows ready to merge**: Immediately approve and merge the PR
5. **If review shows issues**: Request changes with specific feedback
6. **❌ AUTOMATIC REJECTION**: Any PR with linting/TypeScript errors must be rejected immediately

## Package Management Requirements

**MANDATORY:**
- **ALWAYS use `bun` instead of `npm`** for all frontend/JavaScript package management and scripts
- Commands: `bun install`, `bun run dev`, `bun run build`, `bun run quality:fix`, `bun run test`, etc.
- **Rationale**: Bun provides faster package installation, script execution, and better TypeScript support
- **Violation**: Using `npm` instead of `bun` is a process violation that can cause dependency and build issues

## Process Violations

**🚨 BLOCKING VIOLATIONS (IMMEDIATE REJECTION):**
- Creating PR with linting errors
- Skipping mandatory startup sequence
- Using npm instead of bun
- Implementing features without self-contained approach

**❌ PROCESS VIOLATION CONSEQUENCES (APPLIES TO ALL TEAM MEMBERS):**
- Creating PR with linting errors = immediate process violation (AI agents & humans)
- Merging PR with linting errors = critical process failure (AI agents & human reviewers)
- Both implementer and reviewer are responsible for verification regardless of human/AI status
- **NO EXCEPTIONS**: AI agents and human team members follow identical quality standards

**ACCOUNTABILITY:** Both implementer and reviewer are responsible for process adherence, regardless of human/AI status.

---

This issue workflow ensures consistent, high-quality development while enabling parallel work through self-contained features and clear dependency management.