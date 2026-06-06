# Issue #1329 — Gemma 4 12B vs E4B A/B trial (handoff)

This branch carries the **setup** for the #1329 model A/B trial so it can be re-run on
more capable hardware (to also compare against Gemma 4 31B). Full first-pass results
are in the [#1329 results comment](https://github.com/NodeSpaceAI/nodespace-core/issues/1329#issuecomment-4631561216).

## First run (Mac mini M2 Pro, 16 GB) — outcome: keep E4B

12B is **not viable on 16 GB**. Three independent blockers:

1. **Memory = f16 KV cache, not weights.** Q4_K_M weights are ~7 GB and load fine
   (all 49 layers offload to Metal). The blocker is the **f16 KV cache**: at the
   shipped 32K context Gemma-4-12B allocates **10.24 GB** (key/value_length 512 ×
   40 KV layers) → ~17.7 GB total → swap thrash, turns never complete in 180 s.
   E4B's KV @32K is only ~1.8 GB. KV-cache quantization is not wired in the engine.
2. **At reduced 8K context it fits (~10.2 GB) but FABRICATES.** 12B narrates
   fictional tool successes ("created invoice ID 104", "marked it paid") with
   **tool_calls=0** — its chat template (`thought` channel, `<|tool_response>` /
   `<turn|>` EOG) isn't parsed as tool calls by the E4B-tuned loop. Worse than E4B,
   which at least invokes tools.
3. **Latency:** 12B@8K median 126 s/turn (vs E4B ~34 s).

Follow-up unblockers tracked in **#1332** (anti-fabrication guard, KV-cache
quantization, per-model n_ctx, Gemma-12B tool-call parsing).

## What's on this branch

- `packages/agent/src/local_agent/model_manager.rs` — `GEMMA_4_12B` catalog entry
  (`gemma-4-12b-q4km`, exact size 7,381,382,048 B, min_memory_gb 24 so it is not
  auto-recommended on 16 GB but is explicitly loadable). 31B (`gemma-4-31b-q4km`)
  was already in the catalog.
- `packages/nlp-engine/src/chat/types.rs` — **`n_ctx: 8_192` trial probe** (was
  32_768). GLOBAL — throttles every model. **On the bigger machine, revert to
  32_768** (or do the per-model n_ctx work from #1332) before measuring 12B/31B at
  full context.
- `scripts/aichat-matrix.ts` — runs the 8-scenario #1329 matrix against whatever
  model is loaded; writes a JSON results file.
- `scripts/aichat.ts` — existing per-turn CLI harness (unchanged; on main).

## Reproduce on the new machine

```bash
# 0. revert the 8K probe if the machine has headroom (recommended for 12B/31B @32K):
#    set n_ctx back to 32_768 in packages/nlp-engine/src/chat/types.rs

cargo build --release -p nodespace-daemon -p nodespace-cli

rm -rf /tmp/nodespaced-test && mkdir -p /tmp/nodespaced-test
NODESPACED_HEADLESS=1 NODESPACED_SOCKET=/tmp/nodespaced-test/daemon.sock \
  NODESPACED_DB_PATH=/tmp/nodespaced-test/nodespace.db RUST_LOG=info \
  target/release/nodespaced > /tmp/nodespaced-test/daemon.log 2>&1 &

# load + run per model (e4b / 12b / 31b). 31B GGUF (~18.7 GB) downloads on first load.
for M in gemma-4-e4b-q4km gemma-4-12b-q4km gemma-4-31b-q4km; do
  target/release/nodespace --socket /tmp/nodespaced-test/daemon.sock model load "$M"
  NS_BIN=target/release/nodespace NODESPACED_SOCKET=/tmp/nodespaced-test/daemon.sock \
    NS_LOG=/tmp/nodespaced-test/daemon.log NS_MODEL="$M" NS_TIMEOUT_MS=240000 \
    bun run scripts/aichat-matrix.ts "${M#gemma-4-}" "/tmp/nodespaced-test/results-${M}.json"
done
```

Inspect KV-cache fit per model in the daemon log:
`grep -E "llama_kv_cache:.*size =|offloaded.*layers|using device MTL" /tmp/nodespaced-test/daemon.log`

## NOT in this branch / do not assume

- No production default-model change (gated on the recommendation).
- The 12B GGUF is kept locally on the M2 Pro machine but is not in the repo.
- Unrelated in-progress `ChannelSplitter` reasoning-routing edits seen in the
  original primary checkout were deliberately left out — that's #1330 work, not this.
