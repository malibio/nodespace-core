/**
 * Per-model RAM gating tests.
 *
 * Both model-manager.svelte and ai-chat-model-selector.svelte dim/disable a
 * model card or <option> when the system doesn't meet THAT model's own
 * min_memory_gb/minMemoryGb — not a single flat constant. Before issue #1956,
 * both components used one hardcoded MIN_RAM_GB = 16, which meant a machine
 * clearing E4B's 16GB floor would render every exposed model as enabled even
 * if a larger tier (e.g. Gemma 4 26B-A4B's 32GB floor) needed far more.
 *
 * Follows the project pattern of testing extracted logic directly (not
 * rendering Svelte components) using Happy-DOM.
 */

import { describe, it, expect } from 'vitest';

/** Mirrors the per-card `modelRamTooLow` check in model-manager.svelte. */
function modelRamTooLow(systemRamGb: number, minMemoryGb: number): boolean {
  return systemRamGb > 0 && systemRamGb < minMemoryGb;
}

/** Mirrors the `minRequiredGb` derivation in model-manager.svelte. */
function minRequiredGb(models: { min_memory_gb: number }[]): number {
  return models.length > 0 ? Math.min(...models.map((m) => m.min_memory_gb)) : 0;
}

describe('Per-model RAM gating', () => {
  const E4B = { id: 'gemma-4-e4b-q4km', min_memory_gb: 16 };
  const GEMMA_26B_A4B = { id: 'gemma-4-26b-a4b-q8', min_memory_gb: 32 };

  it('a 20GB machine clears E4B but not 26B-A4B', () => {
    const systemRamGb = 20;
    expect(modelRamTooLow(systemRamGb, E4B.min_memory_gb)).toBe(false);
    expect(modelRamTooLow(systemRamGb, GEMMA_26B_A4B.min_memory_gb)).toBe(true);
  });

  it('a 48GB machine clears both tiers', () => {
    const systemRamGb = 48;
    expect(modelRamTooLow(systemRamGb, E4B.min_memory_gb)).toBe(false);
    expect(modelRamTooLow(systemRamGb, GEMMA_26B_A4B.min_memory_gb)).toBe(false);
  });

  it('a 12GB machine clears neither tier', () => {
    const systemRamGb = 12;
    expect(modelRamTooLow(systemRamGb, E4B.min_memory_gb)).toBe(true);
    expect(modelRamTooLow(systemRamGb, GEMMA_26B_A4B.min_memory_gb)).toBe(true);
  });

  it('RAM detection failure (0 GB) never dims any card', () => {
    // systemRamGb === 0 means the daemon RPC hasn't resolved yet or failed;
    // treating that as "too low" would dim every card on every cold load.
    const systemRamGb = 0;
    expect(modelRamTooLow(systemRamGb, E4B.min_memory_gb)).toBe(false);
    expect(modelRamTooLow(systemRamGb, GEMMA_26B_A4B.min_memory_gb)).toBe(false);
  });

  it('the general notice threshold is the minimum across exposed models, not a flat constant', () => {
    // A 20GB machine should NOT see "your machine doesn't meet requirements"
    // when E4B (the lower tier) still fits.
    const localModels = [E4B, GEMMA_26B_A4B];
    expect(minRequiredGb(localModels)).toBe(16);
  });

  it('the general notice only fires when nothing at all fits', () => {
    const localModels = [E4B, GEMMA_26B_A4B];
    const required = minRequiredGb(localModels);

    expect(modelRamTooLow(20, required)).toBe(false); // E4B fits, no notice
    expect(modelRamTooLow(8, required)).toBe(true); // nothing fits, notice shows
  });
});
