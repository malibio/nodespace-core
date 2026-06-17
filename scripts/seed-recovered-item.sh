#!/usr/bin/env bash
# Seed one entry into the Recovered Items log so the Pro "Recovered Items" UI
# (core#1303) can be smoke-tested without staging a real two-device LWW conflict.
#
# The Pro daemon writes superseded conflict-losers to
#   ~/.nodespace/recovered-items-<user>.jsonl   (snake_case, one JSON object/line)
# and the desktop app reads that same file for the current user. This helper
# appends a synthetic entry for a REAL node id so the badge attaches on relaunch.
#
# Usage:
#   scripts/seed-recovered-item.sh [--user <u>] [--db <path>] [--node <id>] \
#                                  [--mine <text>] [--won <text>]
#
#   --user  recovery-log user (default: "default" — the bundled desktop daemon;
#           use "demo-a"/"demo-b" for the two-window demo). Picks the log file:
#           ~/.nodespace/recovered-items-<user>.jsonl
#   --db    libsql/sqlite DB to pick a real node from when --node is omitted
#           (default: ~/.nodespace/database/nodespace.db;
#            two-window demo: /tmp/ns-demo-a/db)
#   --node  node id to attach the badge to (default: auto-pick a text node)
#   --mine  superseded ("your") content   (default: "my offline edit")
#   --won   winning ("current") content   (default: "the edit that won")
#
# After seeding: ⌘Q the app and relaunch → snackbar + ⟲ badge on that node.
set -uo pipefail

USER_ID="default"
DB="$HOME/.nodespace/database/nodespace.db"
NODE=""
MINE="my offline edit"
WON="the edit that won"

while [ $# -gt 0 ]; do
  case "$1" in
    --user) USER_ID="$2"; shift 2;;
    --db)   DB="$2"; shift 2;;
    --node) NODE="$2"; shift 2;;
    --mine) MINE="$2"; shift 2;;
    --won)  WON="$2"; shift 2;;
    *) echo "unknown arg: $1"; exit 2;;
  esac
done

LOG="$HOME/.nodespace/recovered-items-${USER_ID}.jsonl"
mkdir -p "$HOME/.nodespace"

# Auto-pick a real text node if none was given, so the badge actually attaches.
if [ -z "$NODE" ]; then
  [ -e "$DB" ] || { echo "DB not found: $DB (pass --db)"; exit 1; }
  NODE=$(sqlite3 "$DB" "SELECT id FROM node WHERE node_type='text' AND content<>'' ORDER BY modified_at DESC LIMIT 1;" 2>/dev/null)
  [ -n "$NODE" ] || { echo "no text node found in $DB — pass --node <id>"; exit 1; }
  echo "auto-picked node $NODE from $DB"
fi

# Timestamps: superseded (oldest) < winning < recovered (now). RFC3339 UTC.
SUP_AT=$(date -u -v-1H +%Y-%m-%dT%H:%M:%S+00:00 2>/dev/null || date -u -d '1 hour ago' +%Y-%m-%dT%H:%M:%S+00:00)
WIN_AT=$(date -u -v-30M +%Y-%m-%dT%H:%M:%S+00:00 2>/dev/null || date -u -d '30 min ago' +%Y-%m-%dT%H:%M:%S+00:00)
REC_AT=$(date -u +%Y-%m-%dT%H:%M:%S+00:00)

# Build the JSONL line with python so content is safely escaped.
LINE=$(python3 - "$NODE" "$MINE" "$SUP_AT" "$WON" "$WIN_AT" "$REC_AT" <<'PY'
import json,sys
n,mine,sat,won,wat,rat = sys.argv[1:7]
print(json.dumps({
  "node_id": n,
  "superseded_content": mine,
  "superseded_modified_at": sat,
  "winning_content": won,
  "winning_modified_at": wat,
  "recovered_at": rat,
}))
PY
)

printf '%s\n' "$LINE" >> "$LOG"
echo "✓ seeded → $LOG"
echo "  node:       $NODE"
echo "  superseded: $MINE  ($SUP_AT)"
echo "  winning:    $WON  ($WIN_AT)"
echo
echo "Now ⌘Q the app and relaunch (user=$USER_ID). Expect: snackbar once + ⟲ badge on that node."
