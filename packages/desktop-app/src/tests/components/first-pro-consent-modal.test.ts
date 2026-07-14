/**
 * first-pro-consent-modal component (presentational).
 *
 * The gate that keeps local data from reaching the public workspace without an
 * explicit, informed, irreversible choice. Merge is disabled until the user
 * acknowledges permanence; declining shares nothing.
 */
import { describe, it, expect, afterEach, vi } from 'vitest';
import { render, fireEvent, cleanup } from '@testing-library/svelte';

import FirstProConsentModal from '$lib/components/first-pro-consent-modal.svelte';

describe('FirstProConsentModal', () => {
  afterEach(() => cleanup());

  it('renders nothing when closed', () => {
    const { queryByRole } = render(FirstProConsentModal, {
      props: { open: false, onMerge: vi.fn(), onKeepLocal: vi.fn() }
    });
    expect(queryByRole('dialog')).toBeNull();
  });

  it('shows the irreversible warning when open', () => {
    const { getByText } = render(FirstProConsentModal, {
      props: { open: true, onMerge: vi.fn(), onKeepLocal: vi.fn() }
    });
    expect(getByText(/once your notes are in the public workspace/i)).toBeTruthy();
  });

  it('gates Merge behind the acknowledgement checkbox', async () => {
    const onMerge = vi.fn();
    const { getByRole } = render(FirstProConsentModal, {
      props: { open: true, onMerge, onKeepLocal: vi.fn() }
    });
    const mergeBtn = getByRole('button', { name: /merge into public workspace/i });

    // Disabled until acknowledged; clicking does nothing.
    expect(mergeBtn.hasAttribute('disabled')).toBe(true);
    await fireEvent.click(mergeBtn);
    expect(onMerge).not.toHaveBeenCalled();

    // Acknowledge → enabled → merges.
    await fireEvent.click(getByRole('checkbox'));
    expect(mergeBtn.hasAttribute('disabled')).toBe(false);
    await fireEvent.click(mergeBtn);
    expect(onMerge).toHaveBeenCalledTimes(1);
  });

  it('keeps the database local-only without acknowledgement', async () => {
    const onKeepLocal = vi.fn();
    const { getByRole } = render(FirstProConsentModal, {
      props: { open: true, onMerge: vi.fn(), onKeepLocal }
    });
    await fireEvent.click(getByRole('button', { name: /keep this database local-only/i }));
    expect(onKeepLocal).toHaveBeenCalledTimes(1);
  });
});
