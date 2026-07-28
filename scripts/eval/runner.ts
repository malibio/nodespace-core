/**
 * Shared eval harness.
 *
 * Owns everything an eval does NOT need to decide for itself: the environment
 * contract, the preflight gate, argv, chat-node lifecycle, results assembly,
 * the summary table, baseline diffing, and exit codes. An eval supplies only
 * its scenarios and how to score a turn (see ./types.ts, ./fixtures/).
 *
 * Adding an eval is therefore a fixture module, not another copy of this
 * plumbing — which is what let the two original harnesses drift into having
 * separately-implemented, separately-broken versions of the same five concerns.
 */

import { readEnv, ENV_USAGE, REPO_ROOT, type EvalEnv } from "./env.ts";
import {
  abortOnEnvironment,
  preflight,
  readDaemonStatus,
  EnvironmentError,
  EXIT_FAILED,
  EXIT_USAGE,
} from "./preflight.ts";
import type {
  EvalFixture,
  EvalResults,
  Provenance,
  Scenario,
  ScenarioResult,
  TurnRecord,
} from "./types.ts";

// ---------------------------------------------------------------------------
// aichat.ts driver
// ---------------------------------------------------------------------------

/**
 * Create a fresh chat node; return its id.
 *
 * Failing here means the daemon went away mid-run — preflight already proved it
 * was reachable. That is an environment failure, not a scenario failure, and
 * must not be reported (or exit) as one: the scenarios scored before the daemon
 * died would otherwise look like a partial result rather than a void run.
 */
function newChat(env: EvalEnv): string {
  const r = Bun.spawnSync(["bun", "run", env.aichat, "new"], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  if (r.exitCode !== 0) {
    throw new EnvironmentError(
      `The daemon stopped responding partway through the run: could not create a ` +
        `chat node.\n  ${r.stderr.toString().trim()}`,
      `Check that the daemon on ${env.socket} is still alive (and did not run out ` +
        `of memory or get killed).\n  Partial scores from an aborted run are not ` +
        `comparable to a complete one, so none were written.`,
    );
  }
  return r.stdout.toString().trim();
}

/**
 * Run one turn and scrape its outcome.
 *
 * A failed send is recorded rather than thrown, so one flaky turn does not
 * abandon a run that costs minutes of inference. It is flagged `sendFailed` so
 * the caller can tell it apart from a turn that genuinely called no tools —
 * scoring the two alike is what lets a dead daemon pass a "no tools" assertion.
 * The run loop aborts once sends fail consecutively.
 */
function runTurn(env: EvalEnv, chatId: string, message: string): TurnRecord {
  const start = performance.now();
  const r = Bun.spawnSync(["bun", "run", env.aichat, "send", chatId, message], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env },
  });
  const latencyMs = Math.round(performance.now() - start);

  if (r.exitCode !== 0) {
    const err = r.stderr.toString().trim();
    return {
      toolsOffered: `(error: ${err})`,
      toolsCalled: [],
      reply: `(send failed: ${err})`,
      latencyMs,
      sendFailed: true,
    };
  }

  const out = r.stdout.toString();
  return {
    toolsOffered: out.match(/\[tools offered\] (.*)/)?.[1]?.trim() ?? "",
    toolsCalled: [...out.matchAll(/\[tool\] ([a-z_]+)/g)].map((m) => m[1]),
    reply:
      out.match(/assistant> ([\s\S]*)$/)?.[1]?.trim() ?? "(no reply parsed)",
    latencyMs,
  };
}

// ---------------------------------------------------------------------------
// Provenance
// ---------------------------------------------------------------------------

function gitCommit(): { commit: string; dirty: boolean } {
  const rev = Bun.spawnSync(
    ["git", "-C", REPO_ROOT, "rev-parse", "--short", "HEAD"],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  const status = Bun.spawnSync(
    ["git", "-C", REPO_ROOT, "status", "--porcelain"],
    {
      stdout: "pipe",
      stderr: "pipe",
    },
  );
  return {
    commit: rev.exitCode === 0 ? rev.stdout.toString().trim() : "(unknown)",
    dirty: status.exitCode === 0 && status.stdout.toString().trim().length > 0,
  };
}

// ---------------------------------------------------------------------------
// Baseline comparison
// ---------------------------------------------------------------------------

/**
 * Compare against a recorded baseline. Returns the regression count.
 *
 * Joins on scenario `id`, not the prompt text: prompts get reworded (the
 * decontamination pass rewrote every one of them) and joining on text would
 * report the whole suite as removed-and-added.
 */
async function compareToBaseline(
  evalName: string,
  results: ScenarioResult[],
  baselinePath: string,
): Promise<number> {
  let baseline: EvalResults;
  try {
    baseline = JSON.parse(await Bun.file(baselinePath).text());
  } catch (e) {
    // A baseline that cannot be read is an operator error worth surfacing, but
    // it must not discard a run that took minutes of inference to produce.
    console.error(
      `[${evalName}] Warning: could not read baseline at ${baselinePath}: ${e}`,
    );
    return 0;
  }

  const p = baseline.provenance;
  console.log(`── Baseline comparison (vs ${baselinePath}) ──`);
  if (p) {
    console.log(
      `   Recorded: ${p.recordedAt} · model ${p.model} · n_ctx ${p.nCtx}`,
    );
  }

  const byId = new Map((baseline.results ?? []).map((r) => [r.id, r]));
  let regressions = 0;

  for (const cur of results) {
    const base = byId.get(cur.id);
    if (!base) {
      console.log(`   NEW         ${cur.id} → ${cur.passed ? "pass" : "fail"}`);
      continue;
    }
    if (base.passed && !cur.passed) {
      console.log(
        `   REGRESSION  ${cur.id}: was passing, now failing — ${cur.failure}`,
      );
      regressions++;
    } else if (!base.passed && cur.passed) {
      console.log(`   FIXED       ${cur.id}: was failing, now passing`);
    }
  }

  for (const [id] of byId) {
    if (!results.some((r) => r.id === id)) {
      console.log(`   REMOVED     ${id}: in baseline, not in this run`);
    }
  }

  console.log(
    regressions > 0
      ? `\n[${evalName}] ✗ ${regressions} regression(s) vs baseline`
      : `\n[${evalName}] ✓ No regressions vs baseline`,
  );
  return regressions;
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

function usage(fixture: EvalFixture): string {
  return (
    `usage: bun run scripts/eval/${fixture.name}.ts <label> [out.json] [--baseline <path>]\n\n` +
    `  ${fixture.description}\n\n` +
    `  label        tag recorded in the results (e.g. 'e4b')\n` +
    `  out.json     where to write results (default: /tmp/${fixture.name}-<label>-<ts>.json)\n` +
    `  --baseline   compare against a recorded run and fail on regression\n\n` +
    ENV_USAGE +
    `\n\nExit codes: 0 all passed · 1 scenario failure/regression · 2 environment unusable · 64 usage`
  );
}

/**
 * Run an eval end to end. Call this from a fixture's CLI wrapper; it owns the
 * process lifetime and exits rather than returning.
 */
export async function runEval(fixture: EvalFixture): Promise<never> {
  const argv = process.argv.slice(2);

  const baselineFlag = argv.indexOf("--baseline");
  let baselinePath: string | undefined;
  if (baselineFlag !== -1) {
    baselinePath = argv[baselineFlag + 1];
    if (!baselinePath) {
      console.error(`--baseline needs a path\n\n${usage(fixture)}`);
      process.exit(EXIT_USAGE);
    }
    argv.splice(baselineFlag, 2);
  }

  const [label, outPathArg] = argv;
  if (!label) {
    console.error(usage(fixture));
    process.exit(EXIT_USAGE);
  }

  const env = readEnv();

  // Preflight BEFORE any scenario runs. An environment failure must never be
  // reported as scenario results, so this precedes both the run and the file.
  const status = (() => {
    try {
      const s = readDaemonStatus(env);
      preflight(env, s);
      return s;
    } catch (e) {
      if (e instanceof EnvironmentError) abortOnEnvironment(fixture.name, e);
      throw e;
    }
  })();

  const ts = new Date().toISOString().replace(/[:.]/g, "-").slice(0, 19);
  const outPath = outPathArg ?? `/tmp/${fixture.name}-${label}-${ts}.json`;
  const { commit, dirty } = gitCommit();

  const provenance: Provenance = {
    model: status.modelId,
    recordedAt: new Date().toISOString(),
    hostMemoryGb: Number((status.hostRamBytes / 1e9).toFixed(1)),
    nCtx: status.grantedNCtx,
    evalCommit: commit,
    dirty,
  };

  console.error(
    `[${fixture.name}] label=${label} model=${status.modelId} n_ctx=${status.grantedNCtx}` +
      (dirty ? " (working tree dirty)" : ""),
  );

  // -------------------------------------------------------------------------
  // Run
  // -------------------------------------------------------------------------

  const results: ScenarioResult[] = [];

  /**
   * Consecutive failed sends tolerated before the run is declared void.
   *
   * Preflight proves the environment was usable at t=0; it cannot prove it
   * stayed usable across the minutes a run takes. If the daemon dies mid-run
   * every subsequent turn calls no tools, and every negative assertion scores
   * as a PASS — the exact false result this harness exists to prevent, just
   * relocated past the gate. One failure can be a flaky turn and is a real
   * scenario failure; two in a row is the environment.
   */
  const MAX_CONSECUTIVE_SEND_FAILURES = 2;
  let consecutiveSendFailures = 0;

  try {
    for (const group of fixture.groups) {
      const chatId = newChat(env);
      console.error(
        `[${fixture.name}] chat ${chatId} for: ${group.map((s) => s.id).join(", ")}`,
      );

      for (const scenario of group) {
        console.error(`[${fixture.name}] → ${scenario.scenario}`);

        // Prior turns establish context and are never scored.
        const priorTurns: TurnRecord[] = [];
        for (const prior of scenario.priorTurns ?? []) {
          console.error(`[${fixture.name}]   [context] ${prior}`);
          priorTurns.push(runTurn(env, chatId, prior));
        }

        const scored = runTurn(env, chatId, scenario.prompt);

        if (scored.sendFailed) {
          consecutiveSendFailures++;
          if (consecutiveSendFailures >= MAX_CONSECUTIVE_SEND_FAILURES) {
            throw new EnvironmentError(
              `${consecutiveSendFailures} turns in a row failed to send — the daemon ` +
                `went away partway through the run.\n  Last error: ${scored.reply}`,
              `Check whether the daemon on ${env.socket} is still alive (it may have ` +
                `run out of memory or been killed).\n  ${results.length} scenario(s) had ` +
                `been scored; they are NOT written, because scores from a run that died ` +
                `midway are not comparable to a complete one — and turns that never ` +
                `reached the model would score "no tools called" assertions as passes.`,
            );
          }
        } else {
          consecutiveSendFailures = 0;
        }

        const verdict = fixture.score(scenario, [scored]);

        results.push({
          id: scenario.id,
          scenario: scenario.scenario,
          prompt: scenario.prompt,
          passed: verdict.passed,
          failure: verdict.failure,
          turns: [...priorTurns, scored],
          extra: fixture.extra?.(scenario, [scored]),
        });

        console.error(
          `[${fixture.name}]   ${verdict.passed ? "✓" : "✗"} ` +
            `tools=[${scored.toolsCalled.join(",")}] ${scored.latencyMs}ms`,
        );
        if (!verdict.passed)
          console.error(`[${fixture.name}]     ↳ ${verdict.failure}`);
      }
    }
  } catch (e) {
    if (e instanceof EnvironmentError) abortOnEnvironment(fixture.name, e);
    throw e;
  }

  // -------------------------------------------------------------------------
  // Report
  // -------------------------------------------------------------------------

  const total = results.length;
  const passed = results.filter((r) => r.passed).length;
  const failed = total - passed;

  const evalResults: EvalResults = {
    eval: fixture.name,
    label,
    provenance,
    summary: { total, passed, failed },
    results,
  };

  await Bun.write(outPath, JSON.stringify(evalResults, null, 2));
  console.error(`[${fixture.name}] wrote ${total} results to ${outPath}`);

  console.log(`\n── ${fixture.description} ─────────────────────────────────`);
  console.log(`   Label:    ${label}`);
  console.log(`   Model:    ${provenance.model}`);
  console.log(
    `   Context:  n_ctx ${provenance.nCtx} · host RAM ${provenance.hostMemoryGb} GB`,
  );
  console.log(
    `   Commit:   ${provenance.evalCommit}${provenance.dirty ? " (dirty)" : ""}`,
  );
  console.log(`   Passed:   ${passed}/${total}`);
  for (const line of fixture.summary?.(results) ?? []) {
    console.log(`   ${line}`);
  }
  console.log(
    `────────────────────────────────────────────────────────────────────`,
  );
  for (const r of results) {
    console.log(`  ${r.passed ? "✓" : "✗"} ${r.id}`);
    if (!r.passed) console.log(`      ↳ ${r.failure}`);
  }
  console.log(
    `────────────────────────────────────────────────────────────────────\n`,
  );

  let regressions = 0;
  if (baselinePath) {
    regressions = await compareToBaseline(fixture.name, results, baselinePath);
  }

  if (failed > 0 || regressions > 0) {
    console.error(`\n[${fixture.name}] ✗ ${failed}/${total} scenarios failed`);
    process.exit(EXIT_FAILED);
  }
  console.error(`\n[${fixture.name}] ✓ All ${total} scenarios passed`);
  process.exit(0);
}
