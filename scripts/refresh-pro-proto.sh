#!/usr/bin/env bash
# Dev-only: refresh the vendored Pro proto from the source-of-truth
# in the private `nodespace-sync` repo. CI does not run this — the
# vendored `proto/nodespace_pro.proto` checked into the public repo
# is what gets compiled into the Tauri binary.
#
# Run from the repo root. Expects `nodespace-sync` to be a sibling
# directory of `nodespace-core`. Sync access is required (CI will
# never have it); refusing to run with a clear error is the right
# failure mode when the sibling tree is missing.
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
