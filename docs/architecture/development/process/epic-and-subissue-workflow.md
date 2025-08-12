# Epic and Sub-Issue Workflow Guide

> ## 🎯 **MANDATORY FOR ALL EPIC AND SUB-ISSUE WORK**
> 
> **This applies to ALL team members**: AI agents, human engineers, and architects working on issues that are part of larger epics or have sub-issues.

## Overview

This guide defines the standardized approach for working with Epic issues (parent issues with multiple sub-issues) and their associated sub-issues, including branching strategy, status management, and completion workflow.

## Issue Type Classification

### Epic Issues
- **Definition**: Large features broken down into multiple sub-issues
- **Characteristics**: 
  - Contains list of sub-issues (e.g., `- [ ] #55 - Sub-issue title`)
  - Labeled with `epic` or similar
  - Has implementation phases or logical groupings
- **Examples**: Issue #54 (ContentEditable Text Editing Epic)

### Sub-Issues  
- **Definition**: Individual implementable tasks within an epic
- **Characteristics**:
  - References parent epic (e.g., `**Parent Epic**: #54`)
  - Has specific, focused acceptance criteria
  - Can be completed independently within the epic context
- **Examples**: Issue #55 (ContentEditable Foundation)

### Standalone Issues
- **Definition**: Individual features not part of a larger epic
- **Characteristics**: Self-contained with no parent/child relationships
- **Branching**: Individual branch per issue

## Branching Strategy Decision Matrix

### 🔍 Step 1: Identify Issue Type

**Check for these indicators:**

```bash
# View the issue to identify type
gh issue view <issue-number>

# Look for:
# - "Parent Epic: #XX" → This is a SUB-ISSUE
# - List of "- [ ] #XX" → This is an EPIC  
# - Neither → This is STANDALONE
```

### 📋 Step 2: Apply Branching Strategy

| Issue Type | Branching Approach | Branch Naming | Example |
|------------|-------------------|---------------|---------|
| **Epic** | Single branch for entire epic | `feature/issue-<epic-number>-<epic-name>` | `feature/issue-54-contenteditable-epic` |
| **Sub-Issue** | Use parent epic branch | Same as parent epic | Work on `feature/issue-54-contenteditable-epic` |
| **Standalone** | Individual branch | `feature/issue-<number>-<brief-description>` | `feature/issue-123-new-feature` |

### 🚨 Critical Rules

1. **NEVER create separate branches for sub-issues** - Always use the parent epic branch
2. **Epic branch is created when starting the first sub-issue** - Not when viewing the epic
3. **All sub-issues work on the same epic branch** - Enables incremental development
4. **Epic branch contains all sub-issue implementations** - Single PR for entire epic

## Implementation Workflow

### For Epic Issues (Parent Issues)

#### When Starting an Epic:
1. **Don't implement the epic directly** - Start with first sub-issue
2. **Create epic branch when starting first sub-issue**:
   ```bash
   git checkout -b feature/issue-<epic-number>-<epic-name>
   ```
3. **Assign and update epic status**:
   ```bash
   gh issue edit <epic-number> --add-assignee "@me"
   # Update project status to "In Progress"
   ```

#### Epic Branch Management:
- Epic branch lives throughout all sub-issue development
- Single PR created when entire epic is complete
- All sub-issues are implemented incrementally on this branch

### For Sub-Issues

#### Startup Sequence for Sub-Issues:
1. **Identify parent epic** from issue description
2. **Switch to existing epic branch** (don't create new branch):
   ```bash
   git checkout feature/issue-<epic-number>-<epic-name>
   ```
3. **Assign and update sub-issue status**:
   ```bash
   gh issue edit <sub-issue-number> --add-assignee "@me"
   # Update sub-issue project status to "In Progress"
   ```
4. **Update parent epic status** (if not already in progress):
   ```bash
   gh issue edit <epic-number> --add-assignee "@me"
   # Update epic project status to "In Progress"
   ```

#### Sub-Issue Implementation:
1. Work on the sub-issue within the epic branch
2. Commit changes with clear sub-issue references:
   ```bash
   git commit -m "Implement ContentEditable foundation (addresses #55)
   
   - Add ContentEditable support to BaseNode.svelte
   - Preserve existing textarea functionality  
   - Enhance cursor positioning with Selection API
   
   Part of Epic: #54"
   ```
3. **Mark sub-issue as completed** when acceptance criteria are met
4. **Continue to next sub-issue** on same branch (if applicable)

## Status Management

### Project Board Updates

#### For Epic Issues:
- **Todo → In Progress**: When starting first sub-issue
- **In Progress → Ready for Review**: When ALL sub-issues completed
- **Ready for Review → Done**: When epic PR merged

#### For Sub-Issues:
- **Todo → In Progress**: When starting work on specific sub-issue
- **In Progress → Done**: When sub-issue acceptance criteria met
- **Note**: Sub-issues can be marked "Done" while epic remains "In Progress"

### GitHub Issue Status

#### Epic Issues:
- Remain open until entire epic completed
- Individual sub-issue checkboxes can be checked off as completed
- Epic closed when PR merged

#### Sub-Issues:
- Can be closed when completed (even if epic not finished)
- Reference epic in closing comment
- Use closing keywords: "Closes #55" in commit or PR

## PR and Review Strategy

### Single Epic PR Approach (Recommended)
- **One PR for entire epic** when all sub-issues completed
- **Comprehensive review** of entire feature set
- **All sub-issues addressed** in single implementation

### Incremental PR Approach (Alternative)
- **Individual PRs for each sub-issue** (if epic is very large)
- **Each PR must maintain epic branch** as base
- **Final integration PR** for epic completion

## Common Patterns and Examples

### Example: ContentEditable Epic (#54)

```bash
# Starting Epic #54 by implementing Sub-Issue #55
git checkout -b feature/issue-54-contenteditable-epic

# Assign and update both issues
gh issue edit 54 --add-assignee "@me"  # Epic
gh issue edit 55 --add-assignee "@me"  # Sub-issue

# Update project statuses (both to In Progress)
ITEM_ID=$(gh project item-list 5 --owner malibio --limit 200 --format=json | jq -r '.items[] | select(.content.number == 54) | .id')
gh project item-edit --id $ITEM_ID --project-id PVT_kwHOADHu9M4A_nxN --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4

ITEM_ID=$(gh project item-list 5 --owner malibio --limit 200 --format=json | jq -r '.items[] | select(.content.number == 55) | .id')
gh project item-edit --id $ITEM_ID --project-id PVT_kwHOADHu9M4A_nxN --field-id PVTSSF_lAHOADHu9M4A_nxNzgyq13o --single-select-option-id 47fc9ee4

# Work on Issue #55 implementation...
# After completing #55, mark it done and move to #56...
# Continue until entire epic completed
```

## Troubleshooting

### ❌ Common Mistakes

1. **Creating separate branches for sub-issues**
   - **Fix**: Delete sub-issue branch, use epic branch
   
2. **Not updating parent epic status**
   - **Fix**: Always update both epic and sub-issue statuses
   
3. **Closing epic before all sub-issues completed**
   - **Fix**: Keep epic open until entire feature set done

### ✅ Verification Checklist

Before starting any epic/sub-issue work:

- [ ] I have identified the issue type (Epic/Sub-Issue/Standalone)
- [ ] I know the correct branching strategy for this issue type
- [ ] I have the correct branch name ready (epic branch for sub-issues)
- [ ] I understand which issues need status updates (both parent and child)
- [ ] I have read the complete issue descriptions (not just titles)

## Integration with Existing Process

This workflow integrates with the existing [Startup Sequence](startup-sequence.md):

- **Step 2 Enhancement**: Use this guide to determine branching strategy
- **Step 3 Enhancement**: Apply correct branch creation/switching based on issue type
- **Step 5 Enhancement**: Update status for both parent and child issues when applicable

---

**Next Steps**: After reading this guide, continue with the standard [Startup Sequence](startup-sequence.md) and [Issue Workflow](issue-workflow.md) processes.