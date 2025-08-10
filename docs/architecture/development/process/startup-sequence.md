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

### 5. Update Project Status
- **Manual Step**: Go to GitHub web interface
- Update project status: **Todo → In Progress**

### 6. Select Appropriate Agent/Expert
Use the decision tree to choose specialized expertise:

**Decision Tree:**
1. **Is this about visual design, UI patterns, or design systems?** → Use `ux-design-expert`
2. **Is this about frontend components, Svelte, or desktop app features?** → Use `frontend-expert`
3. **Is this about AI integration, local models, or NLP?** → Use `ai-ml-engineer`
4. **Is this about architecture review or complex system design?** → Use `senior-architect-reviewer`
5. **Is this complex research or multi-step work?** → Use `general-purpose`

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

## ✅ Verification Checklist

Before proceeding to implementation:

- [ ] Git status is clean
- [ ] Proper branch created/selected based on strategy
- [ ] Issue assigned to yourself
- [ ] Project status updated to "In Progress"
- [ ] Appropriate expert/agent selected
- [ ] Issue requirements fully understood
- [ ] Self-contained implementation approach planned

## Next Steps

Once startup sequence is complete:
- **For new features**: Follow [Issue Workflow](issue-workflow.md)
- **For implementation**: Begin coding with self-contained approach
- **For questions**: Check [Troubleshooting](../guides/troubleshooting.md)

---

**Remember**: This sequence is mandatory for ALL team members and ensures consistent process adherence across AI agents and human engineers.