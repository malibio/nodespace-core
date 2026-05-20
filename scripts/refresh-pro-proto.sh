#!/usr/bin/env bash
# Refresh the vendored Pro proto from the source-of-truth in nodespace-sync.
# Run from the repo root. Expects nodespace-sync to be a sibling directory.
set -euo pipefail
SRC="$(cd "$(dirname "$0")/.." && pwd)/../nodespace-sync/nodespaced-pro/proto/nodespace_pro.proto"
DST="$(cd "$(dirname "$0")/.." && pwd)/packages/desktop-app/src-tauri/proto/nodespace_pro.proto"
if [ ! -f "$SRC" ]; then
  echo "error: source proto not found at $SRC" >&2
  echo "       (expected ../nodespace-sync/nodespaced-pro/proto/nodespace_pro.proto relative to this repo)" >&2
  exit 1
fi
cp "$SRC" "$DST"
echo "✓ refreshed $DST from $SRC"
