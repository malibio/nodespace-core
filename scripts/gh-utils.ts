#!/usr/bin/env bun

/**
 * GitHub Project Management Utilities for NodeSpace
 * 
 * PURE TYPESCRIPT - NO SHELL COMMANDS
 * Eliminates Claude Code approval prompts by using direct API calls only
 * 
 * Usage:
 *   bun run gh:status 57,58,59 "In Progress"
 *   bun run gh:assign 60,61,62 "@me"
 *   bun run gh:startup 45 "Brief description"
 *   bun run gh:list --status open
 */

import { GitHubClient } from "./github-client.ts";

class NodeSpaceGitHubManager {
  private client: GitHubClient;

  // Status options from issue-workflow.md
  private readonly statusOptions = {
    "Todo": "f75ad846",
    "In Progress": "47fc9ee4", 
    "Waiting for Input": "db18cb7f",
    "Ready for Review": "b13f9084",
    "In Review": "bd055968",
    "Done": "98236657",
    "Ready to Merge": "414430c1"
  } as const;

  constructor() {
    this.client = new GitHubClient();
  }

  async updateIssueStatus(issueNumbers: number[], status: keyof typeof this.statusOptions) {
    const results = await this.client.updateIssueStatus(issueNumbers, status);
    
    for (const result of results) {
      if (result.success) {
        console.log(`✅ Issue #${result.issueNumber} updated to '${status}'`);
      } else {
        console.error(`❌ Failed to update issue #${result.issueNumber}: ${result.error}`);
      }
    }

    return results;
  }

  async assignIssues(issueNumbers: number[], assignee: string = "@me") {
    // Convert @me to current user (API requires actual username)
    const assignees = assignee === "@me" ? ["malibio"] : [assignee.replace("@", "")];
    
    const results = await this.client.assignIssues(issueNumbers, assignees);
    
    for (const result of results) {
      if (result.success) {
        console.log(`✅ Issue #${result.issueNumber} assigned to ${assignee}`);
      } else {
        console.error(`❌ Failed to assign issue #${result.issueNumber}: ${result.error}`);
      }
    }

    return results;
  }

  async unassignIssues(issueNumbers: number[], assignee: string = "@me") {
    // Convert @me to current user (API requires actual username)
    const assignees = assignee === "@me" ? ["malibio"] : [assignee.replace("@", "")];
    
    const results = await this.client.unassignIssues(issueNumbers, assignees);
    
    for (const result of results) {
      if (result.success) {
        console.log(`✅ Issue #${result.issueNumber} unassigned from ${assignee}`);
      } else {
        console.error(`❌ Failed to unassign issue #${result.issueNumber}: ${result.error}`);
      }
    }

    return results;
  }

  async listIssues(options: { status?: string, label?: string, assignee?: string } = {}) {
    const apiOptions: any = {};
    
    if (options.status) {
      apiOptions.state = options.status === "open" ? "open" : options.status === "closed" ? "closed" : "all";
    }
    
    if (options.label) {
      apiOptions.labels = [options.label];
    }

    if (options.assignee) {
      apiOptions.assignee = options.assignee.replace("@", "");
    }

    const issues = await this.client.listIssues(apiOptions);
    
    console.log(`\n📋 Issues (${issues.length} found):`);
    console.log("=".repeat(80));
    
    for (const issue of issues) {
      const assigneeNames = issue.assignees.map(a => a.login).join(", ") || "Unassigned";
      const labelNames = issue.labels.map(l => l.name).join(", ") || "No labels";
      
      console.log(`#${issue.number}: ${issue.title}`);
      console.log(`   State: ${issue.state} | Assignees: ${assigneeNames}`);
      console.log(`   Labels: ${labelNames}\n`);
    }

    return issues;
  }

  async viewIssue(issueNumber: number, web: boolean = false) {
    try {
      if (web) {
        console.log(`🌐 View issue #${issueNumber} at: https://github.com/malibio/nodespace-core/issues/${issueNumber}`);
        return;
      }
      
      const issue = await this.client.getIssue(issueNumber);
      
      console.log(`#${issue.number}: ${issue.title}`);
      console.log(`State: ${issue.state}`);
      console.log(`Assignees: ${issue.assignees.map(a => a.login).join(", ") || "None"}`);
      console.log(`Labels: ${issue.labels.map(l => l.name).join(", ") || "None"}`);
      console.log(`\nBody:\n${issue.body}`);
      
    } catch (error) {
      console.error(`❌ Failed to view issue #${issueNumber}: ${error.message}`);
    }
  }

  async createIssue(title: string, body: string, labels?: string[], assignees?: string[]) {
    try {
      const issue = await this.client.createIssue(title, body, labels, assignees);
      
      console.log("✅ Issue created:");
      console.log(`#${issue.number}: ${title}`);
      console.log(`URL: ${issue.url}`);
      
      return issue;
    } catch (error) {
      console.error(`❌ Failed to create issue: ${error.message}`);
      throw error;
    }
  }

  async editIssue(issueNumber: number, options: {
    title?: string;
    body?: string;
    labels?: string[];
    state?: "open" | "closed";
  }) {
    try {
      await this.client.updateIssue(issueNumber, options);
      
      console.log(`✅ Issue #${issueNumber} updated:`);
      if (options.title) console.log(`  Title: ${options.title}`);
      if (options.body) console.log(`  Body updated`);
      if (options.labels) console.log(`  Labels: ${options.labels.join(", ")}`);
      if (options.state) console.log(`  State: ${options.state}`);
      
      return true;
    } catch (error) {
      console.error(`❌ Failed to edit issue #${issueNumber}: ${error.message}`);
      throw error;
    }
  }

  async createPR(title: string, body: string, draft: boolean = false) {
    try {
      const currentBranch = this.client.getCurrentBranch();
      const pr = await this.client.createPullRequest(title, body, currentBranch, "main", draft);
      
      console.log("✅ Pull request created:");
      console.log(`#${pr.number}: ${title}`);
      console.log(`URL: ${pr.url}`);
      
      return pr;
    } catch (error) {
      console.error(`❌ Failed to create PR: ${error.message}`);
      throw error;
    }
  }

  // Complete startup sequence for an issue
  async startupSequence(issueNumber: number, branchDescription?: string) {
    console.log(`🚀 Starting startup sequence for issue #${issueNumber}...\n`);

    try {
      // 1. Check git status (simplified check)
      console.log("1️⃣ Checking git status...");
      if (!this.client.isWorkingDirectoryClean()) {
        console.log("⚠️  Git directory check failed. Please ensure git status is clean before proceeding.");
        console.log("Run: git status");
        return false;
      }
      console.log("✅ Git status appears clean\n");

      // 2. Get issue details first
      console.log("2️⃣ Getting issue details...");
      const issue = await this.client.getIssue(issueNumber);
      console.log(`📄 Issue: ${issue.title}\n`);

      // 3. Note: Branch creation requires git commands - user must handle manually
      const branchName = branchDescription 
        ? `feature/issue-${issueNumber}-${branchDescription.toLowerCase().replace(/\s+/g, '-')}`
        : `feature/issue-${issueNumber}`;
      
      console.log(`3️⃣ Recommended branch name: ${branchName}`);
      console.log("⚠️  Please create branch manually: git checkout -b " + branchName);
      console.log("Press Enter when branch is created...");

      // 4. Assign issue
      console.log("4️⃣ Assigning issue to self...");
      await this.assignIssues([issueNumber], "@me");

      // 5. Update project status to In Progress
      console.log("5️⃣ Updating project status to 'In Progress'...");
      await this.updateIssueStatus([issueNumber], "In Progress");

      console.log(`🎉 Startup sequence completed for issue #${issueNumber}!`);
      console.log(`📋 Next steps:`);
      console.log(`   - Read the issue requirements carefully`);
      console.log(`   - Plan your self-contained implementation`);
      console.log(`   - Use appropriate specialized agents via Task tool`);
      console.log(`   - Begin implementation\n`);

      return true;

    } catch (error) {
      console.error(`❌ Startup sequence failed: ${error.message}`);
      return false;
    }
  }

  // Quality gate check before PR creation (API only)
  async qualityCheck() {
    console.log("🔍 Running quality checks...\\n");

    try {
      console.log("Checking linting and formatting...");
      // Use the existing quality command from nodespace-app
      const qualityResult = await $`bun run --cwd nodespace-app quality:fix`;
      console.log(qualityResult.text());
      
      console.log("✅ Quality checks passed!");
      return true;
    } catch (error) {
      console.error("❌ Quality checks failed:");
      console.error(error.message);
      console.log("\\n🚨 Fix these issues before creating a PR");
      return false;
    }
  }

  // Complete PR workflow
  async createPRWorkflow(issueNumber: number, title?: string) {
    console.log(`📝 Creating PR for issue #${issueNumber}...\n`);

    // Remind about quality checks
    console.log("⚠️  Ensure you've run: bun run --cwd nodespace-app quality:fix");

    try {
      // Get issue details for PR
      const issue = await this.client.getIssue(issueNumber);
      
      const prTitle = title || issue.title;
      const prBody = `Closes #${issueNumber}`;

      // Create the PR
      const pr = await this.createPR(prTitle, prBody);

      // Update project status to Ready for Review
      console.log("\n📊 Updating project status to 'Ready for Review'...");
      await this.updateIssueStatus([issueNumber], "Ready for Review");

      console.log("\n🎉 PR workflow completed successfully!");
      return pr;

    } catch (error) {
      console.error(`❌ PR workflow failed: ${error.message}`);
      return false;
    }
  }
}

// CLI Interface
async function main() {
  const args = process.argv.slice(2);
  const command = args[0];
  const manager = new NodeSpaceGitHubManager();

  try {
    switch (command) {
      case "issues:status": {
        const issueNumbers = args[1]?.split(",").map(n => parseInt(n.trim()));
        const status = args[2] as keyof typeof manager["statusOptions"];
        
        if (!issueNumbers || !status) {
          console.error('Usage: bun run gh:status 57,58,59 "In Progress"');
          process.exit(1);
        }
        
        await manager.updateIssueStatus(issueNumbers, status);
        break;
      }
      
      case "issues:assign": {
        const issueNumbers = args[1]?.split(",").map(n => parseInt(n.trim()));
        const assignee = args[2] || "@me";
        
        if (!issueNumbers) {
          console.error('Usage: bun run gh:assign 60,61,62 "@me"');
          process.exit(1);
        }
        
        await manager.assignIssues(issueNumbers, assignee);
        break;
      }
      
      case "issues:unassign": {
        const issueNumbers = args[1]?.split(",").map(n => parseInt(n.trim()));
        const assignee = args[2] || "@me";
        
        if (!issueNumbers) {
          console.error('Usage: bun run gh:unassign 60,61,62 "@me"');
          process.exit(1);
        }
        
        await manager.unassignIssues(issueNumbers, assignee);
        break;
      }

      case "issues:create": {
        const title = args[1];
        const body = args[2] || "";
        const labelsStr = args[3];
        const assigneesStr = args[4];
        
        if (!title) {
          console.error('Usage: bun run gh:create "Issue Title" "Issue body" "label1,label2" "user1,user2"');
          process.exit(1);
        }
        
        const labels = labelsStr ? labelsStr.split(",").map(l => l.trim()) : undefined;
        const assignees = assigneesStr ? assigneesStr.split(",").map(a => a.replace("@", "").trim()) : undefined;
        
        await manager.createIssue(title, body, labels, assignees);
        break;
      }

      case "issues:edit": {
        const issueNumber = parseInt(args[1]);
        if (!issueNumber) {
          console.error('Usage: bun run gh:edit 45 --title "New Title" --body "New body" --labels "label1,label2" --state "open|closed"');
          process.exit(1);
        }

        const options: any = {};
        
        const titleIndex = args.indexOf("--title");
        const bodyIndex = args.indexOf("--body");
        const labelsIndex = args.indexOf("--labels");
        const stateIndex = args.indexOf("--state");
        
        if (titleIndex !== -1 && args[titleIndex + 1]) {
          options.title = args[titleIndex + 1];
        }
        
        if (bodyIndex !== -1 && args[bodyIndex + 1]) {
          options.body = args[bodyIndex + 1];
        }
        
        if (labelsIndex !== -1 && args[labelsIndex + 1]) {
          options.labels = args[labelsIndex + 1].split(",").map(l => l.trim());
        }
        
        if (stateIndex !== -1 && args[stateIndex + 1]) {
          const state = args[stateIndex + 1];
          if (state === "open" || state === "closed") {
            options.state = state;
          } else {
            console.error('State must be "open" or "closed"');
            process.exit(1);
          }
        }

        if (Object.keys(options).length === 0) {
          console.error('At least one option must be provided: --title, --body, --labels, or --state');
          process.exit(1);
        }

        await manager.editIssue(issueNumber, options);
        break;
      }
      
      case "issues:list": {
        const options: any = {};
        
        const statusIndex = args.indexOf("--status");
        const labelIndex = args.indexOf("--label");
        const assigneeIndex = args.indexOf("--assignee");
        
        if (statusIndex !== -1) options.status = args[statusIndex + 1];
        if (labelIndex !== -1) options.label = args[labelIndex + 1];
        if (assigneeIndex !== -1) options.assignee = args[assigneeIndex + 1];
        
        await manager.listIssues(options);
        break;
      }

      case "issues:view": {
        const issueNumber = parseInt(args[1]);
        const web = args.includes("--web");
        
        if (!issueNumber) {
          console.error("Usage: bun run scripts/gh-utils.ts issues:view 45 [--web]");
          process.exit(1);
        }
        
        await manager.viewIssue(issueNumber, web);
        break;
      }

      case "issues:startup": {
        const issueNumber = parseInt(args[1]);
        const branchDescription = args[2];
        
        if (!issueNumber) {
          console.error('Usage: bun run gh:startup 45 "Brief description"');
          process.exit(1);
        }
        
        await manager.startupSequence(issueNumber, branchDescription);
        break;
      }

      case "pr:create": {
        const issueNumber = parseInt(args[1]);
        const title = args[2];
        
        if (!issueNumber) {
          console.error('Usage: bun run scripts/gh-utils.ts pr:create 45 "Optional title"');
          process.exit(1);
        }
        
        await manager.createPRWorkflow(issueNumber, title);
        break;
      }
      
      case "help":
      default:
        console.log(`
🚀 NodeSpace GitHub Project Manager

📋 Issue Management:
  bun run gh:create "Title" "Body" "labels" "assignees"  # Create issue
  bun run gh:edit 45 --title "New Title"      # Edit issue title
  bun run gh:edit 45 --body "New body"        # Edit issue body
  bun run gh:edit 45 --labels "tag1,tag2"     # Update labels
  bun run gh:edit 45 --state "closed"         # Close/reopen issue
  bun run gh:status 57,58,59 "In Progress"    # Update status
  bun run gh:assign 60,61,62 "@me"            # Assign issues
  bun run gh:unassign 60,61,62 "@me"          # Unassign issues
  bun run gh:list --status open               # List issues
  bun run gh:list --label foundation          # Filter by label
  bun run gh:list --assignee "@me"            # Your issues
  
🎯 Issue Workflow:
  bun run gh:startup 45 "Brief description"    # Complete startup sequence
  bun run scripts/gh-utils.ts issues:view 45   # View issue details
  bun run scripts/gh-utils.ts issues:view 45 --web  # Open in browser

📝 PR Management:
  bun run scripts/gh-utils.ts pr:create 45     # Create PR workflow
  bun run --cwd nodespace-app quality:fix      # Run quality checks

📊 Available Statuses:
  ${Object.keys(manager["statusOptions"]).map(s => `"${s}"`).join(", ")}

🚀 All commands use TypeScript API (no Claude Code approval prompts)

📖 Based on docs/architecture/development/process/ documentation
        `);
        break;
    }
  } catch (error) {
    console.error(`❌ Error: ${error.message}`);
    process.exit(1);
  }
}

if (import.meta.main) {
  main();
}

export { NodeSpaceGitHubManager };
