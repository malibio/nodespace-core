import { describe, it, expect, beforeEach } from 'vitest';
import { importModalStore } from '$lib/stores/import-modal.svelte';

describe('importModalStore', () => {
  beforeEach(() => {
    importModalStore.open = false;
  });

  it('starts closed', () => {
    expect(importModalStore.open).toBe(false);
  });

  it('show() opens the modal', () => {
    importModalStore.show();
    expect(importModalStore.open).toBe(true);
  });
});
