# Quick Reference Guide

**Essential commands, checklists, and shortcuts for NodeSpace development.**

## 🚀 Startup Sequence (Mandatory)

```bash
# 1. Check git status and commit any pending changes
git status

# 2. Create feature branch (or switch to parent issue branch)
git checkout -b feature/issue-<number>-brief-description

# 3. Assign issue to yourself
gh issue edit <number> --add-assignee "@me"

# 4. Manual: Update GitHub project status (Todo → In Progress)
# 5. Read issue requirements and plan implementation
```

## ⚡ Common Commands

### Quality Gates (MANDATORY before PR)
```bash
# Run all quality checks with auto-fix
bun run quality:fix

# Individual checks
bun run lint        # ESLint only
bun run format      # Prettier only
bun run type-check  # TypeScript only
bun run svelte-check # Svelte validation
```

### Package Management (Bun Only)
```bash
# Install dependencies
bun install

# Run development server  
bun run dev

# Build for production
bun run build

# Run tests
bun run test
```

### Git Workflow
```bash
# Standard feature branch workflow
git checkout -b feature/issue-123-brief-description
git add .
git commit -m "Implement feature (addresses #123)"
git push -u origin feature/issue-123-brief-description

# Create PR
gh pr create --title "Implement Issue #123" --body "Closes #123"
```

### GitHub Issue Management
```bash
# List open issues
gh issue list

# View specific issue
gh issue view <number>

# Assign issue to yourself
gh issue edit <number> --add-assignee "@me"

# Update issue status (via web interface)
# Todo → In Progress → Ready for Review → Done
```

## ✅ Pre-Implementation Checklist

**Before starting any implementation work:**

- [ ] **Git status clean** - No uncommitted changes
- [ ] **Feature branch created** - Following naming convention
- [ ] **Issue assigned** - To yourself via GitHub CLI
- [ ] **Project status updated** - Todo → In Progress (manual)
- [ ] **Requirements understood** - All acceptance criteria clear
- [ ] **Implementation approach planned** - Self-contained with mocks

## ✅ Pre-PR Checklist

**Before creating pull request:**

- [ ] **Quality gates pass** - `bun run quality:fix` shows zero errors
- [ ] **All acceptance criteria met** - Every checkbox in issue completed
- [ ] **Tests added/updated** - Adequate coverage for new functionality
- [ ] **Documentation updated** - Comments, README, or architecture docs
- [ ] **Self-contained implementation** - Feature works independently
- [ ] **Mock implementation** - Proper use of mocks for parallel development

## ✅ Review Checklist (For Reviewers)

**MANDATORY first step:**
- [ ] **Run quality gates** - `bun run quality:fix` before any review
- [ ] **Verify zero errors** - ESLint, TypeScript, Prettier, Svelte all pass

**Review process:**
- [ ] **Requirements verification** - All acceptance criteria met
- [ ] **Code quality** - Follows established patterns
- [ ] **Testing adequacy** - Proper test coverage
- [ ] **Documentation** - Updated as needed
- [ ] **Architecture compliance** - Fits project structure

## 🔧 Troubleshooting

### Quality Gate Failures
```bash
# If linting fails:
bun run lint --fix  # Auto-fix issues
bun run lint        # Check remaining issues

# If TypeScript fails:
bun run type-check  # See specific type errors

# If Svelte Check fails:
bun run svelte-check # See component issues
```

### Common Issues

**ESLint Errors:**
- Don't suppress - fix properly
- Add DOM types to eslint.config.js globals
- Use proper TypeScript types instead of `any`

**TypeScript Errors:**
- Create proper interfaces for complex types
- Use type assertions carefully: `as 'text' | 'task' | 'ai-chat'`
- Import types properly: `import type { MyInterface }`

**Svelte Issues:**
- Avoid `{@html}` - use MarkdownRenderer component
- Proper event typing: `CustomEvent<string>`
- Use `svelte:self` for recursive components

### Branch Issues
```bash
# Switch to existing parent issue branch
git branch -a                    # List all branches
git checkout feature/issue-X-name # Switch to parent branch

# If branch doesn't exist, create individual branch
git checkout -b feature/issue-123-description
```

## 🎯 Agent/Expert Selection

**Use appropriate specialized agent based on task:**

| Task Type | Agent | When to Use |
|-----------|--------|-------------|
| **UI/Design** | `ux-design-expert` | Design systems, visual patterns, user interfaces |
| **Frontend** | `frontend-expert` | Svelte components, Tauri desktop, DOM manipulation |
| **AI/ML** | `ai-ml-engineer` | Local LLM integration, NLP, AI-powered features |
| **Architecture** | `senior-architect-reviewer` | System design, code review, complex features |
| **General** | `general-purpose` | Research, file search, multi-step tasks |

## 📋 Issue Creation Quick Template

```markdown
# [FEATURE] Complete [Component] System

## Self-Contained Scope
- Component implementation with full functionality
- Mock data/services for independent development  
- Comprehensive testing with demo scenarios
- Documentation and usage examples

## Acceptance Criteria
- [ ] Component renders and functions correctly
- [ ] All user interactions work as expected
- [ ] State management implemented
- [ ] Tests provide >90% coverage
- [ ] Documentation includes usage examples
- [ ] Quality gates pass (zero linting errors)
```

## 🚨 Emergency Commands

**If PR is failing quality gates:**
```bash
# Nuclear option - fix all issues
bun run quality:fix

# Then check what still needs manual fixes
bun run lint
bun run type-check  
bun run svelte-check
```

**If stuck on complex types:**
```bash
# Check existing similar implementations
grep -r "interface.*{" src/
grep -r "type.*=" src/

# Look at test utilities for typing patterns
cat src/tests/utils/testUtils.ts
```

---

**Remember**: These commands and checklists apply equally to AI agents and human engineers. No shortcuts or exceptions!