# Ablation arm generators

Throwaway scripts that produced the arms in the parent directory. Committed as
provenance, not as tooling: each one records exactly how its arms were derived,
so a result can be re-derived rather than taken on trust.

**The arms are the artifact. These are how the arms were built.** If you need to
build a new arm, read the one closest to what you want and copy it — do not try
to generalise these into a framework.

Run from the repo root, in the order below (later ones read earlier ones' output):

| script | produces |
|---|---|
| `build_arm.py` | `full-tool-surface` — real 9-tool surface, one candidate |
| `build_arm2.py` | `production-baseline` — 9 tools + the real 3-candidate block |
| `build_arms3.py` | `fidelity-no-subtrees`, `fidelity-rank1-only`, `fidelity-rank1-fetch` |
| `build_arms4.py` | `fidelity-rank1-header-only`, `-tool-only`, `-wrong-fetch`, `fidelity-k1` |
| `build_arm_a_sweep.py` | `*-no-subtrees` across the cases with inexpressible rules |
| `build_isolate.py` | `schema-creation-only-*` — one rule kept per arm |
| `build_priming.py` | `schema-creation-{irrelevant-long,fields-expanded,one-type-terse}` |
| `build_minimal.py` | `schema-creation-minimal`, `minimal-at-fidelity` |
| `build_sweep_minimal.py` | `*-minimal` across all 8 corpus cases |
| `build_filter_rule.py` | `*-filter-rule`, `*-query-empty-rule` |
| `build_prod_trim.py` | `prod-guidance-*` — production's own text, trimmed |
| `build_adr_probe.py` | `prod-guidance-adr-example-*` |

## Inputs they depend on

Several read files produced by capturing a live turn, which are **not**
committed (they are machine- and run-specific):

- `/tmp/tool_defs.json` — from `cargo run --release -p nodespace-agent --bin dump_tool_defs`
- `/tmp/real_candidates.txt`, `/tmp/real_decls.txt`, `/tmp/real_prose.txt` — split
  out of a `NODESPACE_PROMPT_DUMP` capture of a live daemon Stage-2 turn

To regenerate the capture: run a daemon with `NODESPACE_PROMPT_DUMP=<path>`, drive
one actionable turn through it, then split the prompt at `<|tool>declaration:`.

`build_prod_trim.py` and `build_adr_probe.py` need no capture — they read rule
text directly out of `packages/agent/src/skill_rules.rs` at generation time, so
they cannot drift from source.

## One trap worth knowing

`parameters_schema` in a case file is a `toml::Value`, **not** a string. Emitting
it as a multi-line string parses cleanly, runs cleanly, and silently renders as
`parameters:{}` at the template boundary — every tool then reaches the model at
roughly half its production size. `build_arm.py`'s `toml_inline()` is the correct
rendering; the bug was caught only by diffing per-tool rendered sizes against a
live capture, not by any test.
