# Mandatory Startup Sequence

> ## 🚨 **CRITICAL - READ FIRST**
> 
> **This applies to ALL team members**: AI agents, human engineers, and architects.
> 
> **EVERY TEAM MEMBER MUST COMPLETE THIS SEQUENCE BEFORE ANY IMPLEMENTATION WORK**

## ⚠️ MANDATORY STARTUP SEQUENCE ⚠️

**BEFORE ANY IMPLEMENTATION WORK - COMPLETE THIS EXACT SEQUENCE:**

### 1. Check Git Status
```bash
git status
```
- Commit any pending changes first
- Ensure clean working directory

### 2. Pull Latest Changes
```bash
git fetch origin && git pull origin main
```
- Ensure working from latest codebase
- Avoid working on stale code

### 3. Run Test Suite Baseline
```bash
# Run frontend and backend tests in PARALLEL for faster baseline:
bun run test & bun run rust:test & wait

# Or run sequentially if you prefer cleaner output:
bun run test           # Frontend (Vitest + Happy-DOM) ~10-20s
bun run rust:test      # Backend (Rust/Cargo tests) ~30-60s

# Record the results:
# - How many frontend tests are passing/failing?
# - How many backend tests are passing/failing?
# - Which specific tests are failing?
```

**Why this matters:**
- Establishes baseline state BEFORE your changes
- Prevents accidental regressions
- Helps identify if failures are pre-existing or introduced by your work
- Documents known issues at start of work

**Record your baseline:** Create a comment in the issue noting:
```
Starting work - baseline test status:
- Frontend: X passing, Y failing (bun run test)
- Backend: X passing, Y failing (bun run rust:test)
- Known failures: [list specific test names]
```

### 4. Determine Branching Strategy
- **REQUIRED**: Read [Epic and Sub-Issue Workflow Guide](epic-and-subissue-workflow.md) for comprehensive guidance
- Identify issue type: Epic, Sub-Issue, or Standalone
- Apply correct branching strategy based on issue type
- **For Sub-Issues**: Always use parent epic branch (never create separate branch)

### 5. Create/Switch to Branch
Based on strategy determined in step 4:

**Individual Branch Approach:**
```bash
git checkout -b feature/issue-<number>-brief-description
```

**Parent Issue Branch Approach:**
```bash
git checkout feature/issue-<parent-number>-name
```

### 6. Assign Issue
```bash
bun run gh:assign <number> "@me"
```

### 7. Update Project Status - Use Bun API Commands
**Use TypeScript GitHub API commands (no Claude Code approval prompts):**

```bash
# Update status: Todo → In Progress
bun run gh:status <number> "In Progress"
```

**Key Benefits:**
- ✅ **No approval prompts** - Pure TypeScript API calls
- ✅ **Precise and reliable** - Direct GitHub API integration
- ✅ **Simple syntax** - Easy to remember and use
- ✅ **Error handling** - Clear success/failure messages

**📖 See [Issue Workflow](issue-workflow.md) for complete CLI command reference**

### 7.1 Parent Issue Status (Single-Branch Strategy)
**If working on sub-issues with single-branch approach:**
- Update BOTH parent and sub-issue to "In Progress"  
- Assign BOTH issues to yourself
- Example: Issue #26 (parent) + Issue #46 (sub-issue)

```bash
# Update parent issue status
bun run gh:status <parent-number> "In Progress"

# Assign parent issue
bun run gh:assign <parent-number> "@me"
```

### 8. Select Appropriate Agent/Expert - MANDATORY
**🚨 YOU MUST USE SPECIALIZED AGENTS FOR IMPLEMENTATION:**

This step is NOT optional. You MUST delegate to specialized agents using the Task tool:

**Decision Tree:**
1. **Is this about visual design, UI patterns, or design systems?** → Use `ux-design-expert`
2. **Is this about frontend components, Svelte, or desktop app features?** → Use `frontend-expert`
3. **Is this about AI integration, local models, or NLP?** → Use `ai-ml-engineer`
4. **Is this about architecture review or complex system design?** → Use `senior-architect-reviewer`
5. **Is this complex research or multi-step work?** → Use `general-purpose`

**❌ COMMON MISTAKE**: Trying to implement directly instead of using Task tool with specialized agents

**✅ CORRECT APPROACH**: Use Task tool with appropriate subagent_type for all implementation work

### 9. Read Issue Requirements
- Understand ALL acceptance criteria
- Note any special requirements or constraints
- Identify dependencies or integration points

### 10. Plan Self-Contained Implementation
- Design approach that works independently
- Identify what mocks are needed for parallel development
- Plan vertical slice implementation (end-to-end functionality)

## 🔴 CRITICAL PROCESS VIOLATIONS

**If you start implementation work without completing the startup sequence:**

1. **STOP immediately**
2. Complete the startup sequence
3. Restart implementation with proper branch and issue assignment

**Common mistakes to avoid:**
- Reading files before creating feature branch
- Planning implementation before assigning issue
- Using TodoWrite without including startup sequence as first item
- Skipping git checkout step entirely

## ✅ Mandatory Verification Checklist

Before proceeding to implementation, verify ALL of the following:

- [ ] Git status is clean (no uncommitted changes)
- [ ] Latest changes pulled from main (`git pull origin main`)
- [ ] **Baseline test status recorded** (frontend and backend test counts)
- [ ] **Known failing tests documented** (specific test names noted in issue comment)
- [ ] Correct branch created/selected (individual vs parent branch strategy)
- [ ] Issue assigned using `bun run gh:assign <number> "@me"` (API command)
- [ ] Project status updated using `bun run gh:status <number> "In Progress"` (API command)
- [ ] Parent issue also updated if single-branch strategy
- [ ] Specialized agent selected (frontend-expert, ai-ml-engineer, etc.)
- [ ] Agent will be called with Task tool (NOT direct implementation)
- [ ] Issue requirements fully understood
- [ ] Self-contained implementation approach planned

## 📚 Related Documentation

**Essential References:**
- **CLI Commands**: See [Issue Workflow](issue-workflow.md) for complete GitHub CLI command reference
- **Process Overview**: See [Development Overview](../overview.md) for complete development process
- **Agent Guidelines**: See [CLAUDE.md](../../../CLAUDE.md) for detailed agent selection and usage
- **Code Review Process**: See [PR Review](pr-review.md) for review requirements
- **Quality Standards**: See [Code Quality](../standards/code-quality.md) for linting and formatting
- **File Naming**: See [File Naming Conventions](../standards/file-naming-conventions.md) for consistent naming standards

**Quick Command Reference (BUN API COMMANDS):**
```bash
# Essential bun commands for startup sequence (no approval prompts)
bun run gh:assign <number> "@me"
bun run gh:status <number> "In Progress"

# Additional useful commands
bun run gh:create "Title" "Body"   # Create new issue
bun run gh:list                    # List all issues
bun run gh:view <number>          # View issue details
bun run gh:unassign <number> "@me" # Unassign issue
bun run gh:help                   # Show all available commands
```

## Next Steps

Once startup sequence is complete:
- **For new features**: Follow [Issue Workflow](issue-workflow.md)
- **For implementation**: Begin coding with self-contained approach
- **For questions**: Check [Troubleshooting](../guides/troubleshooting.md)

---

**Remember**: This sequence is mandatory for ALL team members and ensures consistent process adherence across AI agents and human engineers.