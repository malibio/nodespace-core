# Delta-Aware Code Review System with GitHub Integration

## Overview

The NodeSpace code review system provides intelligent, iterative code reviews with full GitHub PR integration.

### Key Features

✅ **Delta-Aware Reviews** - Only review changes since last review
✅ **GitHub PR Integration** - Post reviews and inline comments directly to PRs
✅ **Review State Tracking** - Persistent review history across review cycles
✅ **Incremental Workflow** - Efficient reviews as code evolves

---

## Architecture

```
┌──────────────────────────────────────────────────────────────┐
│  /pragmatic-code-review                                      │
│  Slash command triggers review workflow                     │
└─────────────┬────────────────────────────────────────────────┘
              │
              ▼
    ┌─────────────────────────┐
    │  review-manager.ts      │
    │  ● Delta detection      │
    │  ● State tracking       │
    │  ● GitHub integration   │
    └─────┬──────────┬────────┘
          │          │
          ▼          ▼
   ┌──────────┐  ┌─────────────────┐
   │ .git/    │  │ GitHubClient    │
   │ .review- │  │ ● createPRReview│
   │ state    │  │ ● addPRComment  │
   │ .json    │  │ ● getPRDetails  │
   └──────────┘  └─────────────────┘
```

---

## Quick Start

### 1. First Review (Full Mode)

```bash
# Run comprehensive review
/pragmatic-code-review

# This automatically:
# - Checks review status
# - Runs full review if first time
# - Records review state
```

### 2. Subsequent Reviews (Delta Mode)

```bash
# After making changes, run delta review
/pragmatic-code-review

# This automatically:
# - Detects changes since last review
# - Only reviews new commits/files
# - Updates review state
```

### 3. Post Review to GitHub

After review completes, you'll be asked:

```
Would you like me to post this review to the GitHub PR?
```

If you approve:
- Review summary posted as PR comment
- Inline comments added to specific lines
- Review status set (APPROVE/REQUEST_CHANGES/COMMENT)

---

## Review Workflow

### Standard Workflow

```bash
# 1. Make changes and commit
git add .
git commit -m "Fix issue X"

# 2. Run review
/pragmatic-code-review

# 3. Address findings
# ... make fixes ...
git commit -m "Address review findings"

# 4. Run delta review (only reviews new fixes)
/pragmatic-code-review

# 5. When approved, merge
gh pr merge
```

### Review State Management

#### Check Review Status

```bash
bun run scripts/review-manager.ts --status
```

Output:
```
📊 Review Status:
   Branch: feature/issue-158-tests
   Has been reviewed: Yes
   Last review: 10/6/2025, 11:30:00 AM
   Last commit: c730099
   Mode: full
   Pending commits: 2
   Pending files: 3

   Changed files since last review:
     - src/tests/integration/node-creation-events.test.ts
     - scripts/review-manager.ts
     - scripts/github-client.ts
```

#### Reset Review State

```bash
# Start fresh review cycle for current branch
bun run scripts/review-manager.ts --reset
```

---

## How Delta-Aware Reviews Work

### First Review (Full Mode)

```bash
# Compares: origin/main...HEAD (all PR changes)
git diff origin/main...HEAD

# Records state:
{
  "lastReviewedCommit": "abc123",
  "reviews": [{
    "commit": "abc123",
    "mode": "full",
    "filesReviewed": ["file1.ts", "file2.ts"]
  }]
}
```

### Second Review (Delta Mode)

```bash
# Only compares: abc123..HEAD (changes since last review)
git diff abc123..HEAD

# Updates state:
{
  "lastReviewedCommit": "def456",
  "reviews": [
    { "commit": "abc123", "mode": "full", ... },
    { "commit": "def456", "mode": "delta", ... }
  ]
}
```

### Benefits

**Faster Reviews**
- Only review new code, not entire PR repeatedly
- Focus on incremental changes

**Track Progress**
- See what's been reviewed vs. pending
- Verify all findings addressed

**Better Collaboration**
- GitHub comments show review evolution
- Clear audit trail of changes and reviews

---

## GitHub Integration

### Review Types

#### 1. Review-Level Comment

Posted as PR-level comment with full review report:

```markdown
## 📋 Code Review Report

**Recommendation**: REQUEST CHANGES

### 🔴 Critical Issues (2)
- Test count mismatch in header
- Missing error handling

### 🟡 Important (1)
- Add test runner documentation

### Summary
...
```

#### 2. Inline Comments

Posted on specific lines when file/line info available:

```
📍 Line 14 in src/tests/integration/node-creation-events.test.ts

🔴 CRITICAL: Test count mismatch

Test count should be 24, not 21. This documentation
is misleading and should be corrected.
```

#### 3. Review Status

Sets GitHub review state:
- ✅ **APPROVE** - All checks pass
- ❌ **REQUEST_CHANGES** - Issues must be fixed
- 💬 **COMMENT** - Suggestions only

---

## Advanced Usage

### Manual Review Manager Usage

```bash
# Full review with GitHub posting
bun run scripts/review-manager.ts --mode full --post-to-github

# Delta review only
bun run scripts/review-manager.ts --mode delta

# Check what will be reviewed
bun run scripts/review-manager.ts --status

# Reset and start fresh
bun run scripts/review-manager.ts --reset

# Show help
bun run scripts/review-manager.ts --help
```

### Custom Review Workflows

#### Workflow 1: Multiple Small Reviews

```bash
# Feature development with frequent small reviews
git commit -m "Add feature part 1"
/pragmatic-code-review

git commit -m "Add feature part 2"
/pragmatic-code-review  # Delta: only part 2

git commit -m "Add feature part 3"
/pragmatic-code-review  # Delta: only part 3
```

#### Workflow 2: Comprehensive Re-Review

```bash
# Force full re-review after major refactor
bun run scripts/review-manager.ts --reset
/pragmatic-code-review  # Full review from scratch
```

---

## Review State Storage

### Location

```
.git/.review-state.json
```

### Structure

```json
{
  "reviews": [
    {
      "timestamp": "2025-10-06T10:44:32.000Z",
      "commit": "c730099abc...",
      "branch": "feature/issue-158-tests",
      "mode": "full",
      "filesReviewed": [
        "src/tests/integration/node-creation-events.test.ts"
      ],
      "prNumber": 42,
      "reviewUrl": "https://github.com/.../pull/42#pullrequestreview-123"
    }
  ],
  "currentBranch": "feature/issue-158-tests",
  "lastReviewedCommit": "c730099abc..."
}
```

### Benefits

✅ **Git-Native** - Stored in `.git/` directory (not committed)
✅ **Branch-Specific** - Each branch has independent review state
✅ **Persistent** - Survives across review sessions
✅ **Lightweight** - Simple JSON format

---

## Comparison with Traditional Reviews

| Feature | Traditional | Delta-Aware |
|---------|------------|-------------|
| **First review** | Full PR | Full PR ✅ |
| **After fixes** | Re-review entire PR ❌ | Only review fixes ✅ |
| **Speed** | Same time every review ❌ | Faster incremental ✅ |
| **GitHub integration** | Manual gh commands ❌ | Automatic posting ✅ |
| **Progress tracking** | None ❌ | Review history ✅ |
| **Finding resolution** | Manual tracking ❌ | Threaded comments ✅ |

---

## Troubleshooting

### "No open PR found for current branch"

**Problem**: Trying to post review but no PR exists

**Solution**:
```bash
# Create PR first
bun run gh:pr <issue-number>

# Then run review
/pragmatic-code-review
```

### "Review state out of sync"

**Problem**: Review state doesn't match current code

**Solution**:
```bash
# Reset and start fresh
bun run scripts/review-manager.ts --reset
/pragmatic-code-review
```

### "Too many pending changes for delta review"

**Problem**: Many commits since last review, delta review too large

**Solution**:
```bash
# Force full review
bun run scripts/review-manager.ts --reset
/pragmatic-code-review
```

---

## Best Practices

### ✅ DO

- Run review after each logical code change
- Address all 🔴 Critical findings before merging
- Use delta reviews for iterative development
- Post reviews to GitHub for team visibility
- Reset review state when starting major refactor

### ❌ DON'T

- Skip reviews to save time
- Ignore review findings without discussion
- Post reviews to GitHub without reading them first
- Reset review state unnecessarily
- Run delta review without committing changes first

---

## Integration with Development Process

This review system integrates with the [NodeSpace Development Process](./process/overview.md):

1. **Startup Sequence** → Begin work
2. **Implementation** → Write code
3. **First Review** → `/pragmatic-code-review` (full)
4. **Address Findings** → Fix issues
5. **Delta Review** → `/pragmatic-code-review` (delta)
6. **Iterate** → Repeat steps 4-5 until approved
7. **PR Creation** → Post final review to GitHub
8. **Merge** → Complete workflow

---

## Future Enhancements

### Planned Features

- [ ] **AI-suggested fixes** - Automatically generate fix commits
- [ ] **Review templates** - Customizable review criteria by file type
- [ ] **Review metrics** - Track review coverage and quality over time
- [ ] **Team reviews** - Multi-reviewer workflows
- [ ] **Automated merging** - Auto-merge when all reviews approve

---

## API Reference

### ReviewManager Class

```typescript
class ReviewManager {
  // Get files changed since last review
  async getChangedFilesSinceLastReview(): Promise<string[]>

  // Get commits since last review
  async getCommitsSinceLastReview(): Promise<string[]>

  // Get diff for review (full or delta)
  async getDiffForReview(mode: "full" | "delta"): Promise<string>

  // Parse review markdown and extract findings
  parseReviewReport(markdown: string): ReviewReport

  // Post review to GitHub PR
  async postReviewToGitHub(
    prNumber: number,
    report: ReviewReport,
    commitSha: string
  ): Promise<{ id: number; url: string }>

  // Record completed review
  async recordReview(
    mode: "full" | "delta",
    filesReviewed: string[],
    prNumber?: number,
    reviewUrl?: string
  ): Promise<void>

  // Get review history for current branch
  getReviewHistory(): ReviewRecord[]

  // Reset review state
  resetReviewState(): void

  // Get review status summary
  async getReviewStatus(): Promise<ReviewStatus>
}
```

### GitHubClient PR Review Methods

```typescript
class GitHubClient {
  // Get PR number for current branch
  async getPRForBranch(branch?: string): Promise<number | null>

  // Create PR review with inline comments
  async createPRReview(
    prNumber: number,
    body: string,
    event: "APPROVE" | "REQUEST_CHANGES" | "COMMENT",
    comments?: Array<{ path: string; line: number; body: string }>
  ): Promise<{ id: number; url: string }>

  // Add single comment to PR
  async addPRComment(
    prNumber: number,
    body: string,
    commitId?: string,
    path?: string,
    line?: number
  ): Promise<{ id: number; url: string }>

  // Get existing reviews
  async getPRReviews(prNumber: number): Promise<ReviewInfo[]>

  // Get PR details
  async getPRDetails(prNumber: number): Promise<PRDetails>
}
```

---

**Last Updated**: October 6, 2025
**Related**: [Development Process](./process/overview.md) | [PR Review Guidelines](./process/pr-review.md)
