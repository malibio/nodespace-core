import { createLogger } from '$lib/utils/logger';

const log = createLogger('SubtreeAccessDenied');

interface PendingRefusal {
  /** Minimum number of nodes in the aborted delete the actor cannot read. */
  inaccessibleCount: number;
}

let _pending = $state<PendingRefusal | null>(null);

/**
 * Surface a subtree-access-denied refusal to the UI.
 *
 * Called from the optimistic-delete error path when the daemon refuses a cascade
 * delete because the subtree contains nodes the actor cannot read (ADR-041). The
 * globally-mounted `SubtreeAccessDeniedModal` reads this state and renders the
 * refusal dialog. A newer refusal replaces any dialog still on screen.
 */
export function showSubtreeAccessDenied(inaccessibleCount: number): void {
  log.warn(`Delete refused: ${inaccessibleCount} inaccessible node(s) in subtree`);
  _pending = { inaccessibleCount };
}

export function getSubtreeAccessDeniedState() {
  return {
    get pending() {
      return _pending;
    },
    dismiss() {
      _pending = null;
    }
  };
}
