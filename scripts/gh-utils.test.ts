// Regression test for nodespace-core#2288: `bun run gh:assign <n> "@me"` and
// `bun run gh:unassign <n> "@me"` used to resolve "@me" to the hardcoded
// literal string "malibio" regardless of who was actually authenticated via
// `gh`, then print a success message even though the wrong account got
// assigned. This silently mis-assigned real issues (nodespace-core#2283,
// #2280) to the wrong GitHub account.
//
// This exercises NodeSpaceGitHubManager.assignIssues/unassignIssues against a
// stubbed GitHubClient (constructor-injected) so the assertions don't depend
// on which account actually runs this suite -- the stub's resolved login is
// deliberately something no real account will ever be, so a regression back
// to a hardcoded literal fails loudly no matter who's authenticated.
import { describe, expect, mock, test } from "bun:test";
import { NodeSpaceGitHubManager } from "./gh-utils.ts";
import { GitHubClient } from "./github-client.ts";

const RESOLVED_LOGIN = "totally-unrelated-test-account";

function makeStubClient() {
  const getAuthenticatedUser = mock(async () => RESOLVED_LOGIN);
  const assignIssues = mock(async (issueNumbers: number[]) =>
    issueNumbers.map((issueNumber) => ({ issueNumber, success: true })),
  );
  const unassignIssues = mock(async (issueNumbers: number[]) =>
    issueNumbers.map((issueNumber) => ({ issueNumber, success: true })),
  );

  const client = {
    getAuthenticatedUser,
    assignIssues,
    unassignIssues,
  } as unknown as GitHubClient;

  return { client, getAuthenticatedUser, assignIssues, unassignIssues };
}

describe("NodeSpaceGitHubManager.assignIssues", () => {
  test('"@me" resolves via the authenticated-user API, not a hardcoded literal', async () => {
    const { client, getAuthenticatedUser, assignIssues } = makeStubClient();
    const manager = new NodeSpaceGitHubManager(client);

    await manager.assignIssues([2288], "@me");

    expect(getAuthenticatedUser).toHaveBeenCalledTimes(1);
    expect(assignIssues).toHaveBeenCalledWith([2288], [RESOLVED_LOGIN]);

    // The exact regression this guards: the buggy code passed ["malibio"]
    // unconditionally for "@me", no matter who was authenticated.
    const [, assignedTo] = assignIssues.mock.calls[0] as [number[], string[]];
    expect(assignedTo).not.toContain("malibio");
    expect(assignedTo).toEqual([RESOLVED_LOGIN]);
  });

  test("an explicit username bypasses @me resolution entirely", async () => {
    const { client, getAuthenticatedUser, assignIssues } = makeStubClient();
    const manager = new NodeSpaceGitHubManager(client);

    await manager.assignIssues([2288], "@someone-else");

    expect(getAuthenticatedUser).not.toHaveBeenCalled();
    expect(assignIssues).toHaveBeenCalledWith([2288], ["someone-else"]);
  });
});

describe("NodeSpaceGitHubManager.unassignIssues", () => {
  test('"@me" resolves via the authenticated-user API, not a hardcoded literal', async () => {
    const { client, getAuthenticatedUser, unassignIssues } = makeStubClient();
    const manager = new NodeSpaceGitHubManager(client);

    await manager.unassignIssues([2288], "@me");

    expect(getAuthenticatedUser).toHaveBeenCalledTimes(1);
    expect(unassignIssues).toHaveBeenCalledWith([2288], [RESOLVED_LOGIN]);

    const [, assignedFrom] = unassignIssues.mock.calls[0] as [number[], string[]];
    expect(assignedFrom).not.toContain("malibio");
    expect(assignedFrom).toEqual([RESOLVED_LOGIN]);
  });

  test("an explicit username bypasses @me resolution entirely", async () => {
    const { client, getAuthenticatedUser, unassignIssues } = makeStubClient();
    const manager = new NodeSpaceGitHubManager(client);

    await manager.unassignIssues([2288], "@someone-else");

    expect(getAuthenticatedUser).not.toHaveBeenCalled();
    expect(unassignIssues).toHaveBeenCalledWith([2288], ["someone-else"]);
  });
});

// Regression test: an issue that was never added to the ProjectV2 board had no
// project item, so `getItemIdForIssue` returned null and every
// `bun run gh:status <n> "..."` call on it failed with "Issue not found in
// project" — including the ones CLAUDE.md's startup sequence makes mandatory.
// Nothing in the tooling added issues to the board, so this hit every newly
// filed issue (#2376, #2384, #2389, #2390, #2396 were all missing).
//
// Two halves are covered here: creation now places the issue on the board, and
// a status update self-heals a missing row instead of refusing. The second
// matters most — it back-fills issues created before the first half existed.
describe("GitHubClient project-board membership", () => {
  const ISSUE_NUMBER = 2390;
  const ISSUE_NODE_ID = "I_kwDOtestnode";
  const NEW_ITEM_ID = "PVTI_lADOnewitem";

  function makeClientWithStubbedOctokit(options: { alreadyOnBoard: boolean }) {
    const graphqlCalls: Array<{ query: string; vars: any }> = [];

    const graphql = mock(async (query: string, vars: any) => {
      graphqlCalls.push({ query, vars });
      if (query.includes("addProjectV2ItemById")) {
        return { addProjectV2ItemById: { item: { id: NEW_ITEM_ID } } };
      }
      if (query.includes("updateProjectV2ItemFieldValue")) {
        return { updateProjectV2ItemFieldValue: { projectV2Item: { id: NEW_ITEM_ID } } };
      }
      // getProjectItems() paging query.
      return {
        organization: {
          projectV2: {
            items: {
              nodes: options.alreadyOnBoard
                ? [{ id: "PVTI_existing", content: { number: ISSUE_NUMBER } }]
                : [],
              pageInfo: { hasNextPage: false, endCursor: null },
            },
          },
        },
      };
    });

    const issuesCreate = mock(async () => ({
      data: { number: ISSUE_NUMBER, html_url: "https://example.test/i", node_id: ISSUE_NODE_ID },
    }));
    const issuesGet = mock(async () => ({ data: { node_id: ISSUE_NODE_ID } }));

    const client = new GitHubClient("stub-token");
    (client as any).octokit = {
      graphql,
      rest: { issues: { create: issuesCreate, get: issuesGet } },
    };

    return { client, graphql, graphqlCalls, issuesCreate, issuesGet };
  }

  test("creating an issue adds it to the project board", async () => {
    const { client, graphqlCalls } = makeClientWithStubbedOctokit({ alreadyOnBoard: false });

    const issue = await client.createIssue("Title", "Body");

    expect(issue.number).toBe(ISSUE_NUMBER);
    expect(issue.addedToProject).toBe(true);

    const add = graphqlCalls.find((c) => c.query.includes("addProjectV2ItemById"));
    expect(add).toBeDefined();
    expect(add!.vars.contentId).toBe(ISSUE_NODE_ID);
  });

  test("a failed board add still returns the created issue", async () => {
    const { client } = makeClientWithStubbedOctokit({ alreadyOnBoard: false });
    (client as any).octokit.graphql = mock(async () => {
      throw new Error("board unreachable");
    });

    const issue = await client.createIssue("Title", "Body");

    // The issue exists on GitHub regardless — reporting failure by throwing
    // would lose the number the caller actually needs.
    expect(issue.number).toBe(ISSUE_NUMBER);
    expect(issue.addedToProject).toBe(false);
  });

  test("a status update adds a missing issue to the board instead of failing", async () => {
    const { client, graphqlCalls } = makeClientWithStubbedOctokit({ alreadyOnBoard: false });

    const results = await client.updateIssueStatus([ISSUE_NUMBER], "Done");

    expect(results).toEqual([{ issueNumber: ISSUE_NUMBER, success: true }]);

    // The exact regression: this used to short-circuit to
    // { success: false, error: "Issue not found in project" }.
    expect(results[0].error).toBeUndefined();

    const add = graphqlCalls.find((c) => c.query.includes("addProjectV2ItemById"));
    expect(add).toBeDefined();

    // The status write must target the item the add returned.
    const update = graphqlCalls.find((c) => c.query.includes("updateProjectV2ItemFieldValue"));
    expect(update!.vars.itemId).toBe(NEW_ITEM_ID);
  });

  test("an issue already on the board is not re-added", async () => {
    const { client, graphqlCalls } = makeClientWithStubbedOctokit({ alreadyOnBoard: true });

    const results = await client.updateIssueStatus([ISSUE_NUMBER], "In Progress");

    expect(results).toEqual([{ issueNumber: ISSUE_NUMBER, success: true }]);
    expect(graphqlCalls.find((c) => c.query.includes("addProjectV2ItemById"))).toBeUndefined();

    const update = graphqlCalls.find((c) => c.query.includes("updateProjectV2ItemFieldValue"));
    expect(update!.vars.itemId).toBe("PVTI_existing");
  });
});
