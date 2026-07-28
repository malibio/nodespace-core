/**
 * Preflight gate — refuse to score scenarios when the environment cannot
 * produce a valid result.
 *
 * An eval that scores a broken environment is worse than no eval, because its
 * output gets cited. The motivating case: a matrix run reported its two
 * "no tools called" scenarios as PASSING while every turn was dying on
 * `Context window exceeded` before inference ran. A turn that times out calls
 * no tools, so a negative assertion scores it green. The run produced a
 * plausible partial result from an environment where nothing worked, and it
 * took reading daemon logs to notice.
 *
 * Each check below answers a question whose wrong answer silently corrupts
 * scores rather than failing loudly:
 *   1. daemon reachable       — otherwise every turn errors and every negative
 *                               assertion passes
 *   2. requested model loaded — otherwise the score belongs to a different
 *                               model than the one it will be filed under
 *   3. granted window > prompt— the case above; catches in ~2s what otherwise
 *                               takes ~8min of inference to half-discover
 *
 * Failures exit with EXIT_ENVIRONMENT, distinct from a scenario failure, and
 * no results file is written.
 */

import type { EvalEnv } from "./env.ts";

/** Scenario assertions failed. */
export const EXIT_FAILED = 1;
/**
 * The environment could not produce a valid result. Distinct from EXIT_FAILED
 * so a caller can tell "the agent regressed" from "this machine cannot run the
 * eval" without parsing output.
 */
export const EXIT_ENVIRONMENT = 2;

export class EnvironmentError extends Error {
  constructor(
    message: string,
    /** What the operator should do about it. */
    readonly remedy: string,
  ) {
    super(message);
    this.name = "EnvironmentError";
  }
}

/** What the daemon reports about the model it has loaded. */
export interface DaemonStatus {
  loaded: boolean;
  modelId: string;
  grantedNCtx: number;
  hostRamBytes: number;
  /** On-disk path of the database the daemon is actually serving. */
  databasePath: string;
}

/**
 * Size of the agent's tool-registered system prompt, in tokens.
 *
 * Kept in step with `N_CTX_MINIMUM` in packages/nlp-engine/src/chat/mod.rs,
 * whose doc comment records the same ~6,600-token measurement and refuses to
 * load a model into a window below 16,384 for this reason. The exact figure
 * varies with the registered tool set and seeded skills, so this is a floor
 * used to catch an unusable window, not an accounting of a specific run.
 */
export const AGENT_SYSTEM_PROMPT_TOKENS = 6_600;

/**
 * Headroom the eval requires beyond the system prompt itself.
 *
 * The prompt is only the floor: every turn also carries the conversation so
 * far, the injected entity types, and tool results — and multi-turn scenario
 * groups grow all three. A window that merely clears the prompt would pass
 * preflight and then fail mid-group, which is the failure this gate exists to
 * prevent rather than relocate.
 */
const CONTEXT_HEADROOM_TOKENS = 4096;

/** Ask the daemon what it has loaded. Throws EnvironmentError if unreachable. */
export function readDaemonStatus(env: EvalEnv): DaemonStatus {
  // A missing binary throws rather than returning a non-zero exit, and an
  // unhandled throw here would surface as a stack trace with the generic
  // failure exit code — indistinguishable from a scenario failure, which is
  // precisely the confusion this gate exists to remove.
  const spawn = () =>
    Bun.spawnSync(
      [env.nsBin, "--socket", env.socket, "--json", "model", "status"],
      {
        stdout: "pipe",
        stderr: "pipe",
      },
    );
  let r: ReturnType<typeof spawn>;
  try {
    r = spawn();
  } catch (e) {
    throw new EnvironmentError(
      `Could not run the nodespace CLI at ${env.nsBin}: ${e}`,
      `Build it (cargo build --release -p nodespace-cli) or set NS_BIN to its path.`,
    );
  }

  if (r.exitCode !== 0) {
    const stderr = r.stderr.toString().trim();
    // Distinguish "this CLI predates `model status`" from "the daemon is down".
    // Both exit non-zero, and reporting the former as the latter sends the
    // operator to restart a daemon that was never the problem.
    if (/unrecognized subcommand|unexpected argument/i.test(stderr)) {
      throw new EnvironmentError(
        `The CLI at ${env.nsBin} has no \`model status\` subcommand, so the ` +
          `granted context window cannot be checked.`,
        `Rebuild it: cargo build --release -p nodespace-cli`,
      );
    }
    throw new EnvironmentError(
      `Daemon is not reachable on ${env.socket} (\`model status\` exited ${r.exitCode}).\n` +
        (stderr ? `  ${stderr}\n` : ""),
      `Start the test daemon and confirm it is listening on ${env.socket}, or point\n` +
        `  NODESPACED_SOCKET at the socket it is actually using. See\n` +
        `  nodespace-docs/development/agent-eval.md for the full setup.`,
    );
  }

  let parsed: {
    loaded?: boolean;
    model_id?: string;
    granted_n_ctx?: number;
    host_ram_bytes?: number;
  };
  try {
    parsed = JSON.parse(r.stdout.toString());
  } catch (e) {
    throw new EnvironmentError(
      `Could not parse \`model status --json\` output: ${e}`,
      `This usually means NS_BIN (${env.nsBin}) is an older build without\n` +
        `  \`model status\`. Rebuild the CLI: cargo build --release -p nodespace-cli`,
    );
  }

  return {
    loaded: parsed.loaded ?? false,
    modelId: parsed.model_id ?? "",
    grantedNCtx: parsed.granted_n_ctx ?? 0,
    hostRamBytes: parsed.host_ram_bytes ?? 0,
    databasePath: readServedDatabasePath(env),
  };
}

/**
 * The database file the daemon resolved to serve.
 *
 * Read from the daemon rather than inferred from the environment because the
 * database registry overrides the boot-time path (ADR-053): setting
 * `NODESPACED_DB_PATH` alone leaves the daemon serving the real user database,
 * and nothing in the eval's own configuration reveals that.
 */
function readServedDatabasePath(env: EvalEnv): string {
  const r = Bun.spawnSync(
    [env.nsBin, "--socket", env.socket, "--json", "database", "list"],
    { stdout: "pipe", stderr: "pipe" },
  );
  if (r.exitCode !== 0) return "";
  try {
    const parsed = JSON.parse(r.stdout.toString()) as {
      databases?: Array<{ path?: string; is_default?: boolean }>;
    };
    const dbs = parsed.databases ?? [];
    return (dbs.find((d) => d.is_default) ?? dbs[0])?.path ?? "";
  } catch {
    return "";
  }
}

/**
 * Assert the environment can produce a valid result.
 *
 * `systemPromptTokens` is the eval's own system-prompt size — the thing the
 * granted window must exceed. Throws EnvironmentError on the first failure.
 */
export function preflight(
  env: EvalEnv,
  status: DaemonStatus,
  systemPromptTokens: number = AGENT_SYSTEM_PROMPT_TOKENS,
): void {
  // 2. The eval must not be writing into the real user database. Scenarios
  // create schemas, chat nodes, and instances; run against `~/.nodespace` they
  // land in live data and cannot be cleanly undone. This is easy to do by
  // accident, because the registry overrides `NODESPACED_DB_PATH` and the
  // daemon starts perfectly happily either way.
  const realHome = `${process.env.HOME ?? ""}/.nodespace/`;
  if (
    status.databasePath &&
    process.env.HOME &&
    status.databasePath.startsWith(realHome)
  ) {
    throw new EnvironmentError(
      `The daemon is serving the REAL user database (${status.databasePath}).\n` +
        `  This eval creates schemas, chat nodes, and instances; running it here would ` +
        `write test data into live user data.`,
      `Isolate the daemon with NODESPACE_HOME (not NODESPACED_DB_PATH, which the\n` +
        `  database registry overrides):\n` +
        `    NODESPACE_HOME=/tmp/nodespaced-test NODESPACED_HEADLESS=1 \\\n` +
        `      NODESPACED_SOCKET=${env.socket} target/release/nodespaced\n` +
        `  Then confirm with: grep served_db_path <daemon log>`,
    );
  }

  // 3. A model must be loaded, and it must be the one being scored.
  if (!status.loaded) {
    throw new EnvironmentError(
      "The daemon is running but has no chat model loaded, so every turn would " +
        "fail before inference.",
      `Load one: ${env.nsBin} --socket ${env.socket} model load ${env.model}`,
    );
  }

  // The daemon reports the resolved path/id it loaded, which need not equal the
  // catalog id verbatim; require containment rather than equality so a path
  // like ".../gemma-4-e4b-q4km.gguf" still satisfies NS_MODEL=gemma-4-e4b-q4km.
  // Scoring the wrong model is silent otherwise: the results file would carry
  // NS_MODEL while the numbers belong to whatever was actually resident.
  if (!status.modelId.includes(env.model)) {
    throw new EnvironmentError(
      `The daemon has "${status.modelId}" loaded, but this run is labelled for ` +
        `"${env.model}". The scores would be filed against a model that did not produce them.`,
      `Load the requested model:\n` +
        `    ${env.nsBin} --socket ${env.socket} model load ${env.model}\n` +
        `  or set NS_MODEL to what is already loaded.`,
    );
  }

  // 4. The granted window must hold the system prompt with room to work.
  const required = systemPromptTokens + CONTEXT_HEADROOM_TOKENS;
  if (status.grantedNCtx < required) {
    throw new EnvironmentError(
      `The granted context window (${status.grantedNCtx} tokens) is too small for this ` +
        `eval's system prompt (~${systemPromptTokens} tokens plus ${CONTEXT_HEADROOM_TOKENS} ` +
        `tokens of headroom for history and tool results).\n` +
        `  Every turn would die on context overflow before inference runs. Scenarios that ` +
        `assert "no tools called" would then score as PASSING, so the run would report a ` +
        `plausible partial result from an environment where nothing worked.`,
      `The window is sized to host memory at load time (host RAM here: ` +
        `${(status.hostRamBytes / 1e9).toFixed(1)} GB). Free memory and reload the model, ` +
        `or use a smaller one.`,
    );
  }
}

/** Print an EnvironmentError and exit — never as scenario results. */
export function abortOnEnvironment(
  evalName: string,
  e: EnvironmentError,
): never {
  console.error(
    `\n── ${evalName}: ENVIRONMENT NOT USABLE ─────────────────────────────`,
  );
  console.error(e.message);
  console.error(`\n  How to fix:\n  ${e.remedy}`);
  console.error(
    `\n  No scenarios were run and no results file was written: a score from this\n` +
      `  environment would not mean anything.`,
  );
  console.error(
    `────────────────────────────────────────────────────────────────────\n`,
  );
  process.exit(EXIT_ENVIRONMENT);
}
