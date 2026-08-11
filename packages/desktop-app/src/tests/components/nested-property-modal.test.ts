/**
 * NestedPropertyModal — the shared dialog wrapper around NestedFieldEditor.
 *
 * The modal is deliberately persistence-free: it renders the `value` the caller
 * hands it and reports every rebuilt value back through `onPersist`. That is
 * what lets one modal serve forms that store properties flat, under
 * `properties.task`, and under `properties[nodeType]`.
 */
import { describe, it, expect, beforeEach, afterEach, vi } from 'vitest';
import { render, cleanup, screen, fireEvent } from '@testing-library/svelte';
import type { SchemaField } from '$lib/types/schema-node';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

import NestedPropertyModal from '$lib/components/schema/nested-property-modal.svelte';

function field(partial: Partial<SchemaField> & { name: string; type: string }): SchemaField {
  return { protection: 'user', indexed: false, ...partial };
}

const addressField = field({
  name: 'address',
  type: 'object',
  fields: [field({ name: 'street', type: 'string' }), field({ name: 'city', type: 'string' })]
});

// bits-ui Dialog mount effects run through requestAnimationFrame; another suite
// installs a SYNCHRONOUS rAF global and never restores it, which makes those
// effects recurse and overflow the stack. Pin a well-behaved async rAF here.
let originalRaf: typeof globalThis.requestAnimationFrame;
let originalCancelRaf: typeof globalThis.cancelAnimationFrame;

beforeEach(() => {
  originalRaf = globalThis.requestAnimationFrame;
  originalCancelRaf = globalThis.cancelAnimationFrame;
  globalThis.requestAnimationFrame = ((cb: (time: number) => void) =>
    setTimeout(() => cb(performance.now()), 0) as unknown as number) as typeof globalThis.requestAnimationFrame;
  globalThis.cancelAnimationFrame = ((id: number) =>
    clearTimeout(id as unknown as ReturnType<typeof setTimeout>)) as typeof globalThis.cancelAnimationFrame;
});

afterEach(() => {
  cleanup();
  globalThis.requestAnimationFrame = originalRaf;
  globalThis.cancelAnimationFrame = originalCancelRaf;
});

describe('NestedPropertyModal', () => {
  it('renders the caller-supplied value rather than reading a store', () => {
    render(NestedPropertyModal, {
      props: {
        open: true,
        field: addressField,
        value: { street: '1 Main', city: 'Denver' },
        onPersist: vi.fn()
      }
    });

    expect((screen.getByLabelText('Street') as HTMLInputElement).value).toBe('1 Main');
    expect((screen.getByLabelText('City') as HTMLInputElement).value).toBe('Denver');
  });

  it('hands the whole rebuilt value to onPersist on every edit', async () => {
    const onPersist = vi.fn();
    render(NestedPropertyModal, {
      props: { open: true, field: addressField, value: { city: 'Denver' }, onPersist }
    });

    await fireEvent.input(screen.getByLabelText('Street'), { target: { value: '1 Main' } });

    expect(onPersist).toHaveBeenCalledWith({ city: 'Denver', street: '1 Main' });
  });

  it('renders nothing while closed', () => {
    render(NestedPropertyModal, {
      props: { open: false, field: addressField, value: {}, onPersist: vi.fn() }
    });

    expect(screen.queryByLabelText('Street')).toBeNull();
  });
});
