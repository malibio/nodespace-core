#!/usr/bin/env bun
/**
 * `eval:agent` entry point — see ./fixtures/agent-matrix.ts for the scenarios
 * and scoring, and ./runner.ts for everything else.
 */

import fixture from "./fixtures/agent-matrix.ts";
import { runEval } from "./runner.ts";

await runEval(fixture);
