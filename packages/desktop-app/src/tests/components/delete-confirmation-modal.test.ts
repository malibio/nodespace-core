/**
 * DeleteConfirmationModal tests (focus-trap regression).
 *
 * focusTrap lands initial focus on Cancel (the first button). The modal must NOT
 * carry a global "Enter → confirm" handler, or Enter-with-Cancel-focused would
 * delete the node while the highlighted control says Cancel — on a dialog that
 * warns the action cannot be undone. These drive real focus to lock that down.
 */

import { describe, it, expect, afterEach } from 'vitest';
import { tick } from 'svelte';
import { render, screen, fireEvent } from '@testing-library/svelte';
import DeleteConfirmationModal from '$lib/components/delete-confirmation-modal.svelte';
import {
  confirmNodeDeletion,
  showInaccessibleDescendantsRefusal,
  getDeleteConfirmationState
} from '$lib/services/delete-confirmation.svelte';

describe('DeleteConfirmationModal', () => {
  afterEach(() => {
    // Resolve any pending confirmation/refusal so module state doesn't leak between tests.
    getDeleteConfirmationState().cancel();
    getDeleteConfirmationState().acknowledge();
  });

  it('auto-focuses Cancel and does NOT delete on Enter while Cancel is focused (#1414)', async () => {
    let result: boolean | 'pending' = 'pending';
    confirmNodeDeletion(3).then((v) => (result = v));
    await tick(); // let the $state pending flag reach the rendered modal

    render(DeleteConfirmationModal);

    const cancel = screen.getByRole('button', { name: 'Cancel' });
    // focusTrap lands initial focus on the first focusable = Cancel (safe default).
    expect(document.activeElement).toBe(cancel);

    await fireEvent.keyDown(cancel, { key: 'Enter' });
    await tick();
    await Promise.resolve();

    // The dangerous global Enter→confirm is gone: Enter must never resolve `true`.
    expect(result).not.toBe(true);
  });

  it('confirms only when Delete is explicitly activated', async () => {
    let result: boolean | 'pending' = 'pending';
    confirmNodeDeletion(3).then((v) => (result = v));
    await tick();
    render(DeleteConfirmationModal);

    await fireEvent.click(screen.getByRole('button', { name: 'Delete' }));
    await Promise.resolve();
    expect(result).toBe(true);
  });

  it('cancels when Cancel is activated', async () => {
    let result: boolean | 'pending' = 'pending';
    confirmNodeDeletion(3).then((v) => (result = v));
    await tick();
    render(DeleteConfirmationModal);

    await fireEvent.click(screen.getByRole('button', { name: 'Cancel' }));
    await Promise.resolve();
    expect(result).toBe(false);
  });

  describe('inaccessible-descendants refusal (ADR-041)', () => {
    it('shows a terminal refusal with the inaccessible count and no Delete button', async () => {
      let resolved = false;
      showInaccessibleDescendantsRefusal(3).then(() => (resolved = true));
      await tick();

      render(DeleteConfirmationModal);

      expect(screen.getByText(/3 items you don't have access to/)).toBeTruthy();
      expect(screen.queryByRole('button', { name: 'Delete' })).toBeNull();
      expect(resolved).toBe(false);
    });

    it('singularizes the count for exactly 1 inaccessible item', async () => {
      showInaccessibleDescendantsRefusal(1);
      await tick();

      render(DeleteConfirmationModal);

      expect(screen.getByText(/1 item you don't have access to/)).toBeTruthy();
    });

    it('resolves when OK is activated', async () => {
      let resolved = false;
      showInaccessibleDescendantsRefusal(2).then(() => (resolved = true));
      await tick();

      render(DeleteConfirmationModal);

      await fireEvent.click(screen.getByRole('button', { name: 'OK' }));
      await Promise.resolve();
      expect(resolved).toBe(true);
    });
  });
});
