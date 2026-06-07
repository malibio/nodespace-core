#!/usr/bin/env bash
# Run matrix for Ministral 8B and Mistral NeMo 12B back-to-back.
# Usage: bash scripts/run-nemo-vs-8b-matrix.sh
set -euo pipefail

REPO=/Users/malibio/nodespace/nodespace-core
NS_BIN=$REPO/target/release/nodespace
NODESPACED=$REPO/target/release/nodespaced
SOCK=/tmp/nodespaced-test/daemon.sock
DB=/tmp/nodespaced-test/nodespace.db
DUMP_DIR=/tmp/nodespaced-test
NS_TIMEOUT_MS=240000

run_matrix() {
  local model_id="$1"
  local label="$2"
  local out_json="$DUMP_DIR/matrix-${label}.json"
  local log="$DUMP_DIR/daemon-${label}.log"
  local dump="$DUMP_DIR/prompt-dump-${label}.jsonl"

  echo "=== Running matrix for $label ($model_id) ==="

  # Kill any existing daemon and wait for socket to be released
  pkill -9 -f "nodespaced" 2>/dev/null || true
  sleep 2
  rm -f "$SOCK"

  # Clean DB
  rm -f "$DB" "${DB}-wal" "${DB}-shm"

  # Start daemon
  NODESPACE_PROMPT_DUMP="$dump" \
  NODESPACED_SOCKET="$SOCK" \
  NODESPACED_DB_PATH="$DB" \
    "$NODESPACED" \
    > "$log" 2>&1 &
  local daemon_pid=$!
  echo "Daemon PID=$daemon_pid, log=$log"

  # Wait for daemon to be ready (socket appears + seed complete)
  local tries=0
  while [ $tries -lt 30 ]; do
    if [ -S "$SOCK" ]; then
      sleep 1  # extra second for seed nodes
      break
    fi
    sleep 1
    tries=$((tries+1))
  done
  echo "Daemon ready."

  # Load model (triggers download+load into chat engine)
  echo "Loading $model_id..."
  "$NS_BIN" --socket "$SOCK" model load "$model_id"
  echo "Model loaded."

  # Run matrix
  NS_BIN="$NS_BIN" \
  NODESPACED_SOCKET="$SOCK" \
  NS_LOG="$log" \
  NS_MODEL="$model_id" \
  NS_TIMEOUT_MS="$NS_TIMEOUT_MS" \
    bun run "$REPO/scripts/aichat-matrix.ts" "$label" "$out_json"

  echo "=== $label done → $out_json ==="

  # Stop daemon
  kill "$daemon_pid" 2>/dev/null || true
  sleep 2
}

run_matrix "ministral-8b-q4km" "ministral8b-rerun"
run_matrix "mistral-nemo-12b-q4km" "mistral-nemo-12b"

echo ""
echo "=== RESULTS SUMMARY ==="
for f in "$DUMP_DIR/matrix-ministral8b-rerun.json" "$DUMP_DIR/matrix-mistral-nemo-12b.json"; do
  echo ""
  echo "--- $f ---"
  python3 -c "
import json, sys
data = json.load(open('$f'))
results = data.get('results', [])
passed = sum(1 for r in results if r.get('toolsCalled'))
print(f'Model: {data.get(\"model\",\"?\")}  Scenarios with tool calls: {passed}/{len(results)}')
for r in results:
    tc = r.get('toolsCalled', [])
    ok = '✅' if tc or 'Greeting' in r[\"scenario\"] or 'Capability' in r[\"scenario\"] else '❌'
    print(f'  {ok} {r[\"scenario\"]}: {tc}')
" 2>/dev/null || cat "$f" | head -20
done
