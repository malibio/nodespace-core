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
 * Own-write echo classification (ADR-026's C5 extension): earlier versions of
 * this module guessed whether an incoming broadcast was this client's own
 * write looping back, by comparing its content against the last content this
 * client sent (`isPlausibleOwnEcho`). That guess was inherently racy across an
 * async network round-trip and repeatedly produced false-positive/false-
 * negative conflict toasts — the exact failure mode ADR-026's C5 amendment
 * already rejected for a prior client-side heuristic. The daemon now
 * suppresses a connection's own write echoes before they ever reach
 * `WatchNodes` (`packages/daemon/src/services/node_service.rs`,
 * `x-ns-client-id`-scoped `NodeService::with_client()`), so a `database`-
 * sourced event reaching this module is now guaranteed to be a genuine
 * foreign write. No content comparison is needed or performed here anymore.
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
 * never applied — the daemon's echo suppression (ADR-026's C5 extension) guarantees
 * any such event is a genuine foreign write, so the optimistic local content
 * is protected and a conflict notification is always raised.
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

  return { apply: false, notifyConflict: true };
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
