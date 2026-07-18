/**
 * Remote-update policy for SharedNodeStore.
 *
 * Extracted from the skip-while-editing guard inline in `setNode` /
 * `batchSetNodes`. A daemon-broadcast event (`source.type === 'database'`)
 * arriving for a node the user is actively editing — or has unsaved local
 * changes pending — would otherwise overwrite the optimistic store with the
 * *older* server-confirmed state. The optimistic state is authoritative
 * until persistence settles.
 *
 * `decideRemoteUpdate` is a pure function: it takes the incoming node, the
 * existing local node (if any), the update source, and the caller's
 * pre-computed editing state, and returns a decision the caller applies.
 * It does not read `focusManager` or `PersistenceCoordinator` itself — the
 * caller computes `isFocused`/`hasPending` once (each is a live read that
 * can disagree between two reads within the same guard) and passes them in,
 * so this module stays reactivity-free and independently testable.
 *
 * Own-write echo classification (ADR-026 C5 extension): earlier versions of
 * this module guessed whether an incoming broadcast was this client's own
 * write looping back, by comparing its content against the last content this
 * client sent (`isPlausibleOwnEcho`). That guess was inherently racy across an
 * async network round-trip and repeatedly produced false-positive/false-
 * negative conflict toasts — the exact failure mode ADR-026's C5 amendment
 * already rejected for a prior client-side heuristic. The daemon now
 * suppresses a connection's own write echoes before they ever reach
 * `WatchNodes` (`packages/daemon/src/services/node_service.rs`,
 * `x-ns-client-id`-scoped `NodeService::with_client()`), so a `database`-
 * sourced event reaching this module can no longer be this client's own
 * echo of a write made through the SAME gRPC connection. No content
 * comparison is needed or performed here anymore.
 *
 * Sync-service echoes are a separate case the daemon-side fix above does not
 * cover: `nodespace-sync` writes to the local DB in-process via
 * `NodeService::with_client("sync-service")` (ADR-027), not over the gRPC
 * connection the desktop app's `x-ns-client-id` header scopes — so a stale
 * sync-service replay (e.g. during reconnect reconciliation) can still reach
 * this module as a `database`-sourced event whose version is not ahead of
 * the local optimistic version. The `incomingIsNewer` check below guards
 * against exactly that: only a version genuinely ahead of local is treated
 * as a real conflict.
 */

import type { Node } from '$lib/types';
import type { UpdateSource } from '$lib/types/update-protocol';

export interface EditingState {
  isFocused: boolean;
  hasPending: boolean;
}

export type RemoteUpdateDecision =
  | { apply: true }
  | {
      apply: false;
      /** Raise a version-mismatch conflict notification (foreign write to an actively-edited node). */
      notifyConflict: boolean;
    };

/**
 * Core remote-update policy. Given the incoming node, the existing local
 * node (undefined if this id has never been seen locally), the update
 * source, and the caller's editing state, decide whether the caller should
 * apply the incoming node to the store.
 *
 * A `database`-sourced update to a node the user is actively editing is
 * never applied — the optimistic local content is always protected. A
 * conflict notification is raised only when the incoming version is
 * strictly newer than the local version (a genuine foreign write); a
 * same-or-older version is a stale broadcast (most commonly a sync-service
 * replay — same-connection echoes are now suppressed daemon-side, see this
 * module's doc comment) and must not raise a phantom notification.
 *
 * ai-chat nodes are exempt from the skip — see `shouldSkipStaleAiChatUpdate`
 * for that separate guard (message-count heuristic, not editing state).
 */
export function decideRemoteUpdate(
  incoming: Node,
  existingNode: Node | undefined,
  source: UpdateSource,
  editingState: EditingState
): RemoteUpdateDecision {
  const isDatabaseSource = source.type === 'database';
  const isActivelyEdited = editingState.isFocused || editingState.hasPending;

  if (!isDatabaseSource || !existingNode || !isActivelyEdited) {
    return { apply: true };
  }

  // Missing/uncomparable versions fall back to notifying (conservative).
  const incomingIsNewer =
    typeof incoming.version !== 'number' ||
    typeof existingNode.version !== 'number' ||
    incoming.version > existingNode.version;

  return { apply: false, notifyConflict: incomingIsNewer };
}

/**
 * ai-chat nodes are never "typed into" — the messages array is written
 * programmatically via updateNode, and the daemon appends assistant replies
 * autonomously. Skipping daemon broadcasts for them (via `decideRemoteUpdate`)
 * would cause version drift: the store stays at the user-send version while
 * the daemon is N+1 ahead, so the next user send hits an OCC conflict.
 * Always accept daemon updates for ai-chat EXCEPT when the incoming snapshot
 * has strictly fewer messages than what's already in the store (a stale
 * broadcast racing a newer one).
 */
export function shouldSkipStaleAiChatUpdate(
  incoming: Node,
  existingNode: Node | undefined,
  source: UpdateSource
): boolean {
  if (incoming.nodeType !== 'ai-chat' || source.type !== 'database' || !existingNode) {
    return false;
  }
  type AiChatLike = Node & { messages?: unknown[] };
  const incomingMsgs = (incoming as AiChatLike).messages;
  const existingMsgs = (existingNode as AiChatLike).messages;
  const incomingCount = Array.isArray(incomingMsgs) ? incomingMsgs.length : 0;
  const existingCount = Array.isArray(existingMsgs) ? existingMsgs.length : 0;
  return incomingCount < existingCount;
}
