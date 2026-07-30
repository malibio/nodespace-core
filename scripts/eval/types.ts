/**
 * Shared types for the eval harness.
 *
 * An eval is a fixture module: a scenario list plus a scoring function. Every
 * other concern — the environment contract, the preflight gate, results
 * assembly, the summary table, baseline diffing, and exit codes — belongs to
 * scripts/eval/runner.ts and is not reimplemented per eval.
 */

/**
 * One tool call's outcome, beyond its name.
 *
 * Sourced from the executor's own report of what it did — `isError` is only
 * false once the write returned Ok, and `fieldCount` is read from the tool
 * RESULT rather than its arguments. Asserting on arguments would measure what
 * the model asked for, which is a property of the model's output and so the
 * same class of evidence as counting tool names; the result is what the system
 * actually persisted.
 */
export interface ToolCallRecord {
  name: string;
  isError: boolean;
  /**
   * Length of the result's `fields` array, for tools that return one.
   *
   * Absent when the tool does not report fields at all; `0` when a schema
   * persisted with no properties — a real failure that is indistinguishable
   * from success by tool name alone, and which `create_schema` reaches by
   * design (a call carrying neither `fields` nor `description` succeeds).
   */
  fieldCount?: number;
}

/** One turn's observable outcome, scraped from an aichat.ts run. */
export interface TurnRecord {
  toolsOffered: string;
  /**
   * Tool names in call order.
   *
   * Kept as a plain string[] rather than folded into `toolCalls` below because
   * it has a live reader: the routing fixture scores turns off these names
   * directly and never calls into the matrix fixture's assertions. This is a
   * field in current use, not a compatibility shim the greenfield rule would
   * tell us to delete.
   */
  toolsCalled: string[];
  /**
   * Per-call outcomes, parallel to `toolsCalled`.
   *
   * Optional because results files recorded before this field existed do not
   * carry it; a scoring function must treat absence as "unknown", not "failed".
   */
  toolCalls?: ToolCallRecord[];
  reply: string;
  latencyMs: number;
  /**
   * The turn never reached the model — the send itself failed.
   *
   * Distinguished from a turn that ran and called no tools, because the two are
   * indistinguishable by tool calls alone and a negative assertion ("no tools
   * called") scores a failed send as a PASS. The runner uses this to abort when
   * sends fail consecutively rather than scoring a dead environment.
   */
  sendFailed?: boolean;
  /**
   * Stage 1's routing outcome for this turn: `"query"`, `"clarify"`,
   * `"clarify_suppressed"`, `"none"`, `"unavailable"`, or `"failed"`.
   *
   * Undefined when the daemon log carried no routing line at all (a build
   * predating this field, or a log slice that was truncated). A scenario whose
   * final action still passed can have routed by falling through
   * (`"none"`/`"unavailable"`) rather than genuinely matching a skill — this
   * field is what lets that distinction survive into the results file instead
   * of collapsing into the same pass/fail as a real match.
   */
  routingDecision?: string;
  /**
   * Whether Stage 2's prompt actually carried a candidate block.
   *
   * `false` and `undefined` are NOT the same: `false` means routing ran and
   * produced no eligible candidate (an observed outcome), `undefined` means
   * the log line was not found at all (older build, truncated slice). Without
   * this, a turn that silently skipped injection is indistinguishable from one
   * that never routed — both just fall through to the same tool surface.
   */
  stage2CandidatesInjected?: boolean;
  /**
   * Raw model output for this turn's final reply-producing generation,
   * verbatim before any narration/tool-call normalization.
   *
   * Every false result on record so far was diagnosable from one raw
   * generation and invisible in any aggregate score — this is what lets a
   * results file answer "what did the model actually say" without re-running
   * the eval. Absent when the daemon log carried no raw-response span for this
   * turn (an older build, or a turn whose generation never completed).
   */
  rawOutput?: string;
  /**
   * The turn's reply was a degenerate empty generation — the documented
   * failure mode where the model opens a turn and emits neither text nor a
   * tool call (see `agent_loop.rs`'s "Agent returned empty response with no
   * tool calls"). Distinguished from `sendFailed` because the daemon's polling
   * loop reports this the same way it reports a real timeout: `status` never
   * reaches `idle` with a new assistant message.
   *
   * Scored scenarios with this set true are excluded from the pass/fail
   * denominator rather than counted as failures, per the eval's own
   * documentation of the behavior.
   */
  emptyGeneration?: boolean;
}

/** The verdict a fixture's scoring function returns for one scenario. */
export interface Verdict {
  passed: boolean;
  failure?: string;
}

/**
 * One scored unit of an eval.
 *
 * `id` is the key baseline diffing joins on, so it must be stable across runs —
 * rewording a `prompt` is safe, renaming an `id` silently reads as one scenario
 * removed and another added.
 */
export interface Scenario {
  id: string;
  /** Human-readable description shown in the summary table. */
  scenario: string;
  /** The message whose turn gets scored. */
  prompt: string;
  /**
   * Turns run before `prompt` to establish context. They share the scenario's
   * chat node and are NOT scored.
   */
  priorTurns?: string[];
  /**
   * Free-form per-eval fields (expectation shape, load-bearing flags, ...).
   * The runner passes these back to `score` untouched and records them in the
   * results file; it never interprets them.
   */
  [key: string]: unknown;
}

/**
 * A group of scenarios sharing one chat node, run in order so later scenarios
 * see earlier turns. A single-element group is an isolated scenario.
 */
export type ScenarioGroup = Scenario[];

/** One scenario's full record in the results file. */
export interface ScenarioResult {
  id: string;
  scenario: string;
  prompt: string;
  passed: boolean;
  failure?: string;
  /** The scored turn plus any prior-context turns, in order. */
  turns: TurnRecord[];
  /** Eval-specific fields the fixture chose to record. */
  extra?: Record<string, unknown>;
  /**
   * The scored turn hit the documented degenerate-empty-generation failure
   * mode (see `TurnRecord.emptyGeneration`) rather than a real pass/fail.
   *
   * `passed` is still populated (as `false`, since the assertion had nothing
   * to work with) so older tooling that ignores this field degrades safely,
   * but the run summary excludes scenarios with this set from the
   * total/passed/failed denominator — scoring an inference bug as a model
   * failure silently deflates every cell it appears in.
   */
  excludedAsEmptyGeneration?: boolean;
}

/**
 * An eval definition. `scripts/eval/fixtures/*.ts` default-export one of these
 * and contain nothing else of substance.
 */
export interface EvalFixture {
  /** Short slug used in the default results filename and log prefix. */
  name: string;
  /** One-line description shown in the summary header. */
  description: string;
  /**
   * Scenario groups. Each group gets a fresh chat node; scenarios within a
   * group share it.
   */
  groups: ScenarioGroup[];
  /**
   * Score one scenario from its turns. `turns` excludes prior-context turns,
   * which the runner strips before calling this.
   *
   * Must be pure and daemon-free so it is unit-testable without a model.
   */
  score(scenario: Scenario, turns: TurnRecord[]): Verdict;
  /**
   * Optional per-scenario fields to record in the results file beyond
   * pass/fail — routing signals, matched skill, and similar.
   */
  extra?(scenario: Scenario, turns: TurnRecord[]): Record<string, unknown>;
  /**
   * Optional extra summary lines (e.g. "load-bearing: 4/5"), rendered under
   * the headline pass count.
   */
  summary?(results: ScenarioResult[]): string[];
}

/**
 * Conditions under which a run was produced. Recorded in every results file so
 * a cited number carries its context and cannot outlive it.
 */
export interface Provenance {
  /** Model id the daemon actually had loaded. */
  model: string;
  /** ISO-8601 timestamp of the run. */
  recordedAt: string;
  /** Host physical RAM in GB, as reported by the daemon. */
  hostMemoryGb: number;
  /**
   * Context window the daemon granted this model. The eval is only meaningful
   * when this comfortably exceeds the agent's system prompt; see preflight.ts.
   */
  nCtx: number;
  /**
   * Set when the loaded model could only be matched to `NS_MODEL` by filename,
   * because the daemon reported a resolved path rather than a catalog id. The
   * exact build behind this number is unconfirmed.
   */
  modelMatchedByPath?: boolean;
  /** Commit of the repo the eval ran from. */
  evalCommit: string;
  /** True when the working tree had uncommitted changes at run time. */
  dirty: boolean;
  /**
   * Which seeded guidance this run actually measured.
   *
   * Seeding is content-versioned (`_seed.version`/`_seed.key` on each seeded
   * prompt/skill node) and only replaces stale content on daemon startup — a
   * long-running daemon started before a guidance edit landed keeps serving
   * the old content indefinitely, silently. This is read back from the served
   * database right before the run (see `readGuidanceProvenance` in
   * preflight.ts) so a stale-guidance run cannot be mistaken for a fresh one
   * without re-querying the database by hand.
   */
  guidance?: GuidanceProvenance;
}

/**
 * Seed keys and versions for one seeded node type, as actually served.
 *
 * A map rather than a fixed shape because the set of seeded types (currently
 * prompt, skill) is a property of the schema, not of this eval.
 */
export interface GuidanceProvenance {
  [nodeType: string]: Array<{ key: string; version: string }>;
}

/** The results file an eval run writes. */
export interface EvalResults {
  eval: string;
  label: string;
  provenance: Provenance;
  summary: {
    /** Scored scenarios only — excludes empty-generation exclusions. */
    total: number;
    passed: number;
    failed: number;
    /**
     * Scenarios excluded as degenerate empty generations, not scored either
     * way. Kept separate from `failed` so the rate is visible rather than
     * silently deflating the pass rate; present (possibly 0) whenever the
     * runner supports the exclusion, so its absence marks an older results
     * file rather than a run with none.
     */
    excludedEmptyGenerations?: number;
  };
  results: ScenarioResult[];
}
