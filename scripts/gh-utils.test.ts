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
import type { GitHubClient } from "./github-client.ts";

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
