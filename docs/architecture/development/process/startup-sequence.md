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

### 2. Determine Branching Strategy
- Check parent issue for specified approach (single branch vs. individual branches)
- Look for instructions in issue description or comments

### 3. Create/Switch to Branch
Based on strategy determined in step 2:

**Individual Branch Approach:**
```bash
git checkout -b feature/issue-<number>-brief-description
```

**Parent Issue Branch Approach:**
```bash
git checkout feature/issue-<parent-number>-name
```

### 4. Assign Issue
```bash
gh issue edit <number> --add-assignee "@me"
```

### 5. Update Project Status - Use CLI Commands
**Use GitHub CLI commands to update project status:**

```bash
# Get project item ID for your issue
ITEM_ID=$(gh project item-list 5 --owner malibio | grep "<issue-title>.*<number>" | awk '{print $NF}')

# Update status: Todo → In Progress
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4
```

**📖 See [Issue Workflow](issue-workflow.md) for complete CLI command reference**

### 5.1 Parent Issue Status (Single-Branch Strategy)
**If working on sub-issues with single-branch approach:**
- Update BOTH parent and sub-issue to "In Progress"  
- Assign BOTH issues to yourself
- Example: Issue #26 (parent) + Issue #46 (sub-issue)

```bash
# Update parent issue status
PARENT_ID=$(gh project item-list 5 --owner malibio | grep "<parent-issue-title>.*<parent-number>" | awk '{print $NF}')
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $PARENT_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4

# Assign parent issue
gh issue edit <parent-number> --add-assignee "@me"
```

### 6. Select Appropriate Agent/Expert - MANDATORY
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

### 7. Read Issue Requirements
- Understand ALL acceptance criteria
- Note any special requirements or constraints
- Identify dependencies or integration points

### 8. Plan Self-Contained Implementation
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
- [ ] Correct branch created/selected (individual vs parent branch strategy)
- [ ] Issue assigned using `gh issue edit <number> --add-assignee "@me"` (NOT web interface)
- [ ] Project status updated using CLI commands (NOT web interface)
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

**Quick Command Reference:**
```bash
# Essential CLI commands for startup sequence
gh issue edit <number> --add-assignee "@me"
gh project item-list 5 --owner malibio | grep "<issue-title>.*<number>"
gh project item-edit --project-id PVT_kwHOADHu9M4A_nxN --id $ITEM_ID --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4
```

## Next Steps

Once startup sequence is complete:
- **For new features**: Follow [Issue Workflow](issue-workflow.md)
- **For implementation**: Begin coding with self-contained approach
- **For questions**: Check [Troubleshooting](../guides/troubleshooting.md)

---

**Remember**: This sequence is mandatory for ALL team members and ensures consistent process adherence across AI agents and human engineers.