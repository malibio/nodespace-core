/**
 * Remote-update policy for SharedNodeStore.
 *
 * Extracted from the skip-while-editing guard inline in `setNode` /
 * `batchSetNodes`. A daemon-broadcast
 * event (`source.type === 'database'`) arriving for a node the user is
 * actively editing — or has unsaved local changes pending — would otherwise
 * overwrite the optimistic store with the *older* server-confirmed state.
 * The optimistic state is authoritative until persistence settles.
 *
 * `decideRemoteUpdate` is a pure function: it takes the incoming node, the
 * existing local node (if any), the update source, and the caller's
 * pre-computed editing state, and returns a decision the caller applies.
 * It does not read `focusManager` or `PersistenceCoordinator` itself — the
 * caller computes `isFocused`/`hasPending` once (each is a live read that
 * can disagree between two reads within the same guard) and passes them in,
 * so this module stays reactivity-free and independently testable.
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
      /** Stash `incoming.version` as the server-confirmed version for the next OCC write. */
      stashVersion: boolean;
      /** Raise a version-mismatch conflict notification (foreign write to an actively-edited node). */
      notifyConflict: boolean;
    };

/**
 * Decide whether an incoming database broadcast is plausibly an echo of
 * *this client's own* most-recent write. Used to decide whether to stash
 * the broadcast's `node.version` for the next OCC write.
 *
 * Returns `true` only when `incoming.content` matches the content this
 * client last sent to the backend for this node. That covers the canonical
 * case the guard exists for: our own write looping back through the daemon
 * broadcast.
 *
 * Returns `false` for everything else, including any incoming content we
 * have no last-sent record for. This is deliberately conservative — false
 * negatives just defer to OCC (the next UpdateNode RPC carries the local
 * `node.version` and the backend's OCC surfaces any conflict), while false
 * positives would silently overwrite a foreign writer's change. An earlier
 * `local.startsWith(incoming)` heuristic had exactly that false-positive
 * shape: alice with optimistic `"hello world"` + bob writes `"hello"` →
 * bob's broadcast falsely classified as own-echo → bob's version stashed →
 * alice's next RPC overwrites bob.
 */
export function isPlausibleOwnEcho(incoming: Node, lastSentContent: string | undefined): boolean {
  if (lastSentContent === undefined) return false;
  return lastSentContent === (incoming.content ?? '');
}

/**
 * Core remote-update policy. Given the incoming node, the existing local
 * node (undefined if this id has never been seen locally), the update
 * source, the caller's editing state, and the content this client last
 * persisted for the node (for the own-echo check), decide whether the
 * caller should apply the incoming node to the store.
 *
 * ai-chat nodes are exempt from the skip — see `shouldSkipStaleAiChatUpdate`
 * for that separate guard (message-count heuristic, not editing state).
 */
export function decideRemoteUpdate(
  incoming: Node,
  existingNode: Node | undefined,
  source: UpdateSource,
  editingState: EditingState,
  lastSentContent: string | undefined
): RemoteUpdateDecision {
  const isDatabaseSource = source.type === 'database';
  const isActivelyEdited = editingState.isFocused || editingState.hasPending;

  if (!isDatabaseSource || !existingNode || !isActivelyEdited) {
    return { apply: true };
  }

  const stashVersion =
    typeof incoming.version === 'number' && isPlausibleOwnEcho(incoming, lastSentContent);

  return {
    apply: false,
    stashVersion,
    // A foreign write (not our own echo) to an actively-edited node is
    // skipped to protect the optimistic text, but must not be silent.
    notifyConflict: !stashVersion
  };
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
