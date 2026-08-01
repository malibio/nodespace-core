import { createLogger } from '$lib/utils/logger';

const log = createLogger('DeleteConfirmation');

interface PendingConfirmation {
  descendantCount: number;
  resolve: (confirmed: boolean) => void;
}

/**
 * Terminal refusal state (ADR-041): the backend rejected the cascade delete because the
 * subtree contains nodes the actor cannot read. No node was deleted. There is no Delete
 * button here — `acknowledge()` is the only way out, matching the "count only, no identifying
 * information" disclosure rule (nothing more for the user to decide between).
 */
interface PendingRefusal {
  inaccessibleCount: number;
  resolve: () => void;
}

let _pending = $state<PendingConfirmation | null>(null);
let _pendingRefusal = $state<PendingRefusal | null>(null);

/**
 * Show a confirmation dialog before deleting a node with descendants.
 * Returns true if the user confirms, false if they cancel.
 * Nodes with no descendants (descendantCount === 0) skip the dialog and return true immediately.
 * Returns false immediately if another confirmation is already in progress.
 */
export async function confirmNodeDeletion(descendantCount: number): Promise<boolean> {
  if (descendantCount === 0) {
    return true;
  }

  if (_pending !== null || _pendingRefusal !== null) {
    log.warn('Delete confirmation already in progress — ignoring concurrent request');
    return false;
  }

  log.debug(`Showing delete confirmation for ${descendantCount} descendants`);

  return new Promise((resolve) => {
    _pending = { descendantCount, resolve };
  });
}

/**
 * Show the access-gate refusal: the delete was NOT performed because `inaccessibleCount`
 * nodes in the subtree are unreadable by the actor. Resolves once the user acknowledges —
 * there's nothing to confirm, just to inform.
 */
export async function showInaccessibleDescendantsRefusal(inaccessibleCount: number): Promise<void> {
  if (_pending !== null || _pendingRefusal !== null) {
    log.warn('Delete confirmation already in progress — ignoring concurrent refusal');
    return;
  }

  log.debug(`Showing inaccessible-descendants refusal: ${inaccessibleCount} items`);

  return new Promise((resolve) => {
    _pendingRefusal = { inaccessibleCount, resolve };
  });
}

export function getDeleteConfirmationState() {
  return {
    get pending() {
      return _pending;
    },
    get pendingRefusal() {
      return _pendingRefusal;
    },
    confirm() {
      if (_pending) {
        _pending.resolve(true);
        _pending = null;
      }
    },
    cancel() {
      if (_pending) {
        _pending.resolve(false);
        _pending = null;
      }
    },
    acknowledge() {
      if (_pendingRefusal) {
        _pendingRefusal.resolve();
        _pendingRefusal = null;
      }
    }
  };
}
