#!/usr/bin/env bun
/**
 * `eval:routing` entry point — see ./fixtures/routing.ts for the scenarios and
 * scoring, and ./runner.ts for everything else.
 */

import fixture from "./fixtures/routing.ts";
import { runEval } from "./runner.ts";

await runEval(fixture);
