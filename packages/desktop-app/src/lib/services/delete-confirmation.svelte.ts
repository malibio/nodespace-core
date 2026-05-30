import { createLogger } from '$lib/utils/logger';

const log = createLogger('DeleteConfirmation');

interface PendingConfirmation {
  descendantCount: number;
  resolve: (confirmed: boolean) => void;
}

let _pending = $state<PendingConfirmation | null>(null);

/**
 * Show a confirmation dialog before deleting a node with descendants.
 * Returns true if the user confirms, false if they cancel.
 * Nodes with no descendants (descendantCount === 0) skip the dialog and return true immediately.
 */
export async function confirmNodeDeletion(descendantCount: number): Promise<boolean> {
  if (descendantCount === 0) {
    return true;
  }

  log.debug(`Showing delete confirmation for ${descendantCount} descendants`);

  return new Promise((resolve) => {
    _pending = { descendantCount, resolve };
  });
}

export function getDeleteConfirmationState() {
  return {
    get pending() {
      return _pending;
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
    }
  };
}
