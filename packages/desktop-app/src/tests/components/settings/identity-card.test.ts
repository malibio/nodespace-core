/**
 * IdentityCard (ADR-037) — the Settings → Database edit path for
 * the seeded local-user PersonNode, and the "editable afterwards" surface
 * the onboarding wizard's identity step points to.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { tick } from 'svelte';
import { render, fireEvent } from '@testing-library/svelte';

const mockInvoke = vi.fn();
import { mockTauriCore } from '../../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);

import IdentityCard from '$lib/components/settings/sections/identity-card.svelte';

const BLANK = { nodeId: 'person-1', name: '', email: '', isBlank: true };
const FILLED = { nodeId: 'person-1', name: 'Alice Example', email: 'alice@example.com', isBlank: false };

describe('IdentityCard', () => {
  beforeEach(() => {
    mockInvoke.mockReset();
  });

  it('shows "Not set" and empty fields when the seeded person is blank', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_local_identity') return Promise.resolve(BLANK);
      return Promise.resolve();
    });

    const { container } = render(IdentityCard);
    await tick();
    await tick();

    expect(container.textContent).toContain('Not set');
    expect(container.querySelector<HTMLInputElement>('#identity-card-name')?.value).toBe('');
  });

  it('loads and displays the current name/email when already set', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_local_identity') return Promise.resolve(FILLED);
      return Promise.resolve();
    });

    const { container } = render(IdentityCard);
    await tick();
    await tick();

    expect(container.textContent).toContain('Set');
    expect(container.textContent).not.toContain('Not set');
    expect(container.querySelector<HTMLInputElement>('#identity-card-name')?.value).toBe(
      'Alice Example'
    );
    expect(container.querySelector<HTMLInputElement>('#identity-card-email')?.value).toBe(
      'alice@example.com'
    );
  });

  it('saves an edit and shows the resulting confirmation', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_local_identity') return Promise.resolve(BLANK);
      if (cmd === 'set_local_identity') return Promise.resolve(FILLED);
      return Promise.resolve();
    });

    const { container } = render(IdentityCard);
    await tick();
    await tick();

    const nameInput = container.querySelector<HTMLInputElement>('#identity-card-name')!;
    const emailInput = container.querySelector<HTMLInputElement>('#identity-card-email')!;
    await fireEvent.input(nameInput, { target: { value: 'Alice Example' } });
    await fireEvent.input(emailInput, { target: { value: 'alice@example.com' } });

    const saveButton = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Save'
    )!;
    await fireEvent.click(saveButton);
    await tick();
    await tick();

    expect(mockInvoke).toHaveBeenCalledWith('set_local_identity', {
      name: 'Alice Example',
      email: 'alice@example.com'
    });
    expect(container.textContent).toContain('Saved.');
    expect(container.textContent).toContain('Set');
  });

  it('surfaces a save failure without silently discarding it', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'get_local_identity') return Promise.resolve(BLANK);
      if (cmd === 'set_local_identity') return Promise.reject(new Error('daemon unreachable'));
      return Promise.resolve();
    });

    const { container } = render(IdentityCard);
    await tick();
    await tick();

    const saveButton = Array.from(container.querySelectorAll('button')).find(
      (b) => b.textContent?.trim() === 'Save'
    )!;
    await fireEvent.click(saveButton);
    await tick();
    await tick();

    expect(container.textContent).toContain('daemon unreachable');
  });
});
