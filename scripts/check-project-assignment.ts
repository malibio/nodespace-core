#!/usr/bin/env bun

/**
 * Check which issues are not assigned to the NodeSpace project
 */

import { GitHubClient } from "./github-client.ts";

async function main() {
  const client = new GitHubClient();

  console.log("🔍 Checking project assignments...\n");

  // Get all open issues in the repository
  console.log("📋 Fetching all open issues from repository...");
  const allIssues = await client.listIssues({ state: "open" });
  console.log(`   Found ${allIssues.length} open issues\n`);

  // Get all issues in the project
  console.log("📊 Fetching issues in project...");
  const projectItems = await client.getProjectItems();
  const projectIssueNumbers = new Set(
    projectItems.map(item => item.content?.number).filter(Boolean)
  );
  console.log(`   Found ${projectIssueNumbers.size} issues in project\n`);

  // Find issues not in the project
  const unassignedIssues = allIssues.filter(
    issue => !projectIssueNumbers.has(issue.number)
  );

  if (unassignedIssues.length === 0) {
    console.log("✅ All open issues are assigned to the project!");
    return;
  }

  console.log(`⚠️  Found ${unassignedIssues.length} issues NOT in the project:\n`);
  console.log("=".repeat(80));

  for (const issue of unassignedIssues) {
    const assigneeNames = issue.assignees.map(a => a.login).join(", ") || "Unassigned";
    const labelNames = issue.labels.map(l => l.name).join(", ") || "No labels";

    console.log(`#${issue.number}: ${issue.title}`);
    console.log(`   Assignees: ${assigneeNames}`);
    console.log(`   Labels: ${labelNames}\n`);
  }

  console.log("=".repeat(80));
  console.log(`\n💡 To add these issues to the project, you'll need to:`);
  console.log(`   1. Open each issue in GitHub`);
  console.log(`   2. Manually add them to the "NodeSpace" project`);
  console.log(`   3. Or use the GitHub web interface's bulk project assignment\n`);
}

main();
