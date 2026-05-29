# AI-Chat provider picker: fix hydration bug + redesign as dropdown-driven flow

## REVISED SCOPE (supersedes details below where they conflict)

After discussion + live debugging, the agreed shape:

- **Backend fixes A + B: DONE** (children-tree returns raw properties; provider enum widened to
  `native|ollama|openai|pty`). Tests pass; daemon builds.
- **Naming (per nodespace-docs):** the page-level viewer is **`AiChatNodeViewer`** (`Ai`, not `AI`;
  matches `AiChatNodeBehavior`). Convention is strictly `*Node` / `*NodeViewer`; sub-components are
  plain-named helpers (like `ChatMessage`/`ChatInput`), **no `*View`/`*Viewer`/`*Node` suffix**.
  → Renamed `ai-chat-pty-view.svelte` → **`ai-chat-pty-session.svelte`** (done). Added
  **`ai-chat-model-picker.svelte`** (done). `AiChatNodeViewer` is the single dispatcher.
- **Provider control:** header **dropdown** (not tiles): Built-in (native) · Ollama (gray when
  `ollamaAvailable()` false) · OpenAI (disabled/grayed, no backend) · Agent/terminal (pty).
- **Per-node chat = Option 1 (no shared store).** `chatStore` is a pre-ADR-034 singleton serving only the
  ephemeral global `ChatPanel`. Every conversation is now an `ai-chat` node, so **delete `chatStore` and
  `ChatPanel` entirely** (no orphans, per CLAUDE.md). `AiChatNodeViewer` owns its own per-node
  send/stream against the daemon session API (`localAgentNewSession`/`localAgentSend`/streaming events /
  `ensureModelReady`), reading the node's `provider`/`model`. One inference engine in the daemon serves
  all nodes via independent sessions.
- **Deletions + moves:** delete `chat-store.svelte.ts`, `chat-panel.svelte`, `chat-store.test.ts`.
  Move the `DisplayMessage` type (plain UI message shape) into `types/agent-types.ts` (beside
  `ToolExecutionRecord` it references); update importers (`chat-message.svelte`, `ai-chat-node-viewer`,
  `ai-chat-viewer.test.ts`). Remove the `chat` tab: tab-type union (`navigation.ts`), `<ChatPanel>` +
  branch (`pane-content.svelte`), persistence guard (`tab-persistence-service.ts`), and repoint the
  "AI Chat" nav item (`navigation-sidebar.svelte` / `layout.ts`) to **create/open a new `ai-chat` node**
  (minimal entry point).
- **Deferred to a follow-up issue:** turn the "AI Chat" sidebar item into a **collapsible list of
  `ai-chat` nodes** (like Collections / Schema Types). This PR only repoints it to create-a-node.
- **OpenAI mode:** grayed out (no backend config exists).

## Context

PR #1220 (merged) added an `ai-chat` provider picker (a 4-tile grid). In live testing it was inert —
clicking a provider did nothing. Playwright debugging against the browser dev stack found the cause is
**not** the UI but two backend bugs, plus a desired UX redesign:

1. **Hydration drops properties (the "nothing happens" bug).** The viewer hydrates nodes from the
   `children-tree` endpoint, which returns `"properties": {}` — empty — even though the single-node
   `GET /api/nodes/<id>` returns the full properties. Root cause: `build_node_tree_recursive`
   (`packages/core/src/services/node_service.rs:6920`) serializes via `node_to_typed_value` →
   `flatten_properties_for_api` (`packages/core/src/models/mod.rs:130`), while **every other** daemon
   endpoint (`get_node`, `get_children`, `get_roots`, `query`, `search`) returns raw properties via
   `node_to_proto`. So a selected `provider` persists to the DB but the viewer re-reads `{}` and the
   picker shows forever. (Verified: backend stored `provider:"native"`; children-tree returned `{}`.)

2. **Provider enum rejects the new modes.** `AiChatNodeBehavior::validate`
   (`packages/core/src/behaviors/mod.rs:~1513`) only allows `native|anthropic|gemini|mistral`. It will
   **reject** `ollama|openai|pty` — the ADR-034 modes. So even with hydration fixed, `pty`/`ollama`/
   `openai` writes fail validation.

3. **UX redesign (requested).** Replace the 4-tile grid with a **dropdown at the top**. Selecting a
   mode drives a per-mode step: `pty` → harness picker (installed agents only); `built-in` → model list
   (downloaded + downloadable, with download); `ollama` → discover-or-gray, then its model list;
   `openai` → **grayed out / non-selectable** (no backend exists yet). For built-in/ollama, once a model
   is selected and ready, show the existing message UI.

Good news: the backends for the redesign mostly already exist (Issues #1008/#1058/#1194):
`chatModelList/Download/CancelDownload/Delete/Load/Unload` + `getSystemRamGb` (built-in **and** ollama
models merged, `backend: "gguf"|"ollama"`), `ptyCheckAgentAvailability`, and a gRPC `OllamaAvailable`
(needs a thin TS wrapper). OpenAI has no config backend → gray out.

## Approach

### A. Backend fix 1 — children-tree must return raw properties
File: `packages/core/src/services/node_service.rs` (`build_node_tree_recursive`, ~6913-6921).
- Stop flattening in the tree path so it matches every sibling endpoint. Replace the
  `node_to_typed_value(node.clone())` call with raw node serialization (`serde_json::to_value(node)`)
  plus the existing `uri` injection, OR factor the raw branch out. Keep the children array logic
  unchanged. Net effect: children-tree returns the same `properties` shape as `get_node` (namespaced,
  raw), which the frontend already normalizes on its side.
- Note: `flatten_properties_for_api` is also used by the `ops` layer + MCP handlers
  (`packages/core/src/ops/node_ops.rs`, `mcp/handlers/nodes.rs`) — **leave those**; the fix is scoped to
  the tree endpoint to restore parity with the gRPC node endpoints. Add/adjust a unit test asserting the
  tree result includes a node's `properties` (e.g. an `ai-chat` node with `provider`).

### B. Backend fix 2 — widen the ai-chat provider enum
File: `packages/core/src/behaviors/mod.rs` (`AiChatNodeBehavior::validate`, ~1513; defaults ~1559).
- Change the allowed `provider` set to ADR-034's modes: `native | ollama | openai | pty`. Keep default
  `native`. Update the matching unit test(s) in that file.

### C. Frontend — provider dropdown + per-mode flow
File: `packages/desktop-app/src/lib/components/viewers/ai-chat-node-viewer.svelte` (+ a small new
`ai-chat-config.svelte` sub-view if it keeps the main viewer readable; the existing
`ai-chat-pty-view.svelte` already handles the pty terminal/launch).

Replace the 4-tile placeholder grid with a header **provider `<select>`** offering:
`Built-in model` (native) · `Ollama` · `OpenAI endpoint` (disabled/grayed) · `Agent (terminal)` (pty).
- Writes go through the existing flat-property path: `sharedNodeStore.updateNode(nodeId,
  { properties: { ...current.properties, provider } }, source, { skipConflictDetection: true })`
  (the `skipConflictDetection` already added in #1220; the storage layer auto-namespaces under `ai-chat`).
- **pty**: render the existing `AiChatPtyView` (harness picker showing installed agents via
  `ptyCheckAgentAvailability()` — gray/annotate ones with `binaryFound:false`; note `authFound` is
  currently hardcoded true, so only gate on `binaryFound`). Launch → terminal (unchanged from #1220).
- **built-in (native)**: call `chatModelList()` + `getSystemRamGb()` + `chatModelRecommended()`; show
  models filtered to `backend === 'gguf'`. Downloaded models selectable; downloadable ones show a
  Download action (`chatModelDownload()`, progress via the `model://download-progress` event,
  `chatModelCancelDownload()`). On a ready+selected model, write `provider:native, model:<id>` and render
  the message UI.
- **ollama**: add a `ollamaAvailable()` wrapper in `tauri-commands.ts` (gRPC `OllamaAvailable` +
  Tauri `ollama_available` already exist — only the TS wrapper is missing). If unavailable → the Ollama
  option is grayed in the dropdown with a hint. If available → show `chatModelList()` entries with
  `backend === 'ollama'`; selecting one writes `provider:ollama, model:<id>` → message UI.
- **openai**: present in the dropdown but `disabled` (grayed), selecting does nothing. Tooltip/hint:
  "Coming soon".
- Reuse types/wrappers from `tauri-commands.ts` (`ModelEntry`/`chatModel*`, `AgentAvailabilityInfo`,
  `getSystemRamGb`) and `agent-types.ts` (`ModelInfo`). Use `createLogger`; no `any`; no `{@html}`.
- The viewer already reads flat `node.properties.{provider,model,status,messages}` — keep that. The
  message-UI branch (modes native/ollama/openai once a model is set) stays as-is.

### D. Remove debug instrumentation
Strip the temporary `[DIAG]` `log.warn` calls added to `selectProvider` during debugging
(`ai-chat-node-viewer.svelte`).

### Out of scope
- OpenAI endpoint config (no backend) — grayed out.
- Changing `flatten_properties_for_api` behavior globally or the ops/MCP paths.
- PTY-in-browser (the dev proxy doesn't bridge AgentSessionService; pty terminal is verified under
  `tauri:dev`, message-UI/model flows under `dev:browser`).

## Critical files
- `packages/core/src/services/node_service.rs` — `build_node_tree_recursive` (fix A)
- `packages/core/src/behaviors/mod.rs` — `AiChatNodeBehavior::validate` + defaults (fix B)
- `packages/desktop-app/src/lib/components/viewers/ai-chat-node-viewer.svelte` — dropdown + per-mode flow (C, D)
- `packages/desktop-app/src/lib/components/viewers/ai-chat-pty-view.svelte` — reused for pty (no change beyond what #1220 shipped)
- `packages/desktop-app/src/lib/services/tauri-commands.ts` — add `ollamaAvailable()` wrapper; reuse `chatModel*`, `getSystemRamGb`, `ptyCheckAgentAvailability`
- Existing reuse: `agent-types.ts` (`ModelInfo`), the `model://download-progress` event, `local_agent_service` gRPC (already wired)

## Workflow / standards
- This is a follow-up to merged #1220. Start a fresh worktree: `EnterWorktree({name:
  "issue-1220-aichat-picker-redesign"})` (or continue on the current follow-up branch
  `issue-1220-followup-provider-picker-conflict` which already has the `skipConflictDetection` fix —
  fold this work in there). `bun install`; do not re-run baseline unnecessarily.
- Proto already has the model/ollama RPCs — no proto change expected. After Rust edits:
  `cargo build -p nodespace-core -p nodespace-daemon`.
- Frontend uses Bun: `bun run --cwd packages/desktop-app quality:fix`, `bun run test`. Never `bun test`.
- `quality:fix` runs `cargo fmt --all` (whole workspace) — `git status` after and revert stray
  reformats of files you didn't touch (e.g. #1222's `sqlite_store.rs`).

## Verification
1. **Backend unit:** `cargo test -p nodespace-core` — new test that children-tree returns a node's
   `properties` (non-empty for an ai-chat node with `provider`); updated behavior test accepting
   `ollama|openai|pty`. `cargo build -p nodespace-daemon`.
2. **The original bug, end-to-end (browser dev — already set up):** daemon (headless) + dev-proxy(3001) +
   Vite(5173) running; drive with Playwright MCP (WebKit). Create `/ai-chat` node, open it, pick a
   provider from the dropdown → confirm `GET /api/nodes/<id>/children-tree` now returns non-empty
   `properties.provider`, the picker advances, and it **persists across reload** (the exact failure
   reproduced during debugging).
3. **Per-mode (browser):** built-in shows gguf models + RAM-aware recommend + download progress; ollama
   grays when daemon reports unavailable, else lists ollama models; openai grayed; selecting a ready
   model → message UI.
4. **pty (Tauri only):** `bun run tauri:dev` — dropdown→Agent(terminal)→installed harness→Launch→terminal
   (proxy can't bridge AgentSessionService).
5. **Pre-PR:** `bun run test:all`, `bun run quality:fix` (+ revert stray fmt), then PR + `/pragmatic-code-review`.

## Note on dev environment (already done this session, keep for the implementer)
- Daemon sidecar isn't built by `tauri:dev`; built `nodespaced`/`nodespace` and staged them at
  `target/debug/binaries/<name>-aarch64-apple-darwin`. The app reuses a running daemon if one is healthy.
- The old `~/.nodespace/daemon-db` was a SurrealDB/RocksDB dir incompatible with libsql (#1222) →
  `SQLITE_CANTOPEN`; moved aside to `daemon-db.surreal-bak-*` so the daemon creates a fresh libsql DB.
- `@grpc/grpc-js` was missing for the dev-proxy → `bun install` pulled the dev-tools deps.
