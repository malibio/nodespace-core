/**
 * Session Capture Settings Tests (Issue #1125)
 *
 * Tests the getCaptureSettings and updateCaptureSettings tauri-commands API surface.
 * In non-Tauri environment these return mock defaults.
 */

import { describe, it, expect } from 'vitest';
import { getCaptureSettings, updateCaptureSettings } from '$lib/services/tauri-commands';

describe('Capture Settings Commands', () => {
  describe('getCaptureSettings', () => {
    it('returns default settings outside Tauri', async () => {
      const settings = await getCaptureSettings();

      expect(settings).toBeDefined();
      expect(settings.enabled).toBe(false);
      expect(settings.sync).toBe(false);
      expect(settings.content).toBe('metadata_only');
    });

    it('returns object with correct shape', async () => {
      const settings = await getCaptureSettings();

      expect(typeof settings.enabled).toBe('boolean');
      expect(typeof settings.sync).toBe('boolean');
      expect(['metadata_only', 'summary', 'full']).toContain(settings.content);
    });
  });

  describe('updateCaptureSettings', () => {
    it('accepts partial update and returns settings outside Tauri', async () => {
      const result = await updateCaptureSettings({ enabled: true });

      expect(result).toBeDefined();
      expect(result.enabled).toBe(true);
      expect(typeof result.sync).toBe('boolean');
      expect(['metadata_only', 'summary', 'full']).toContain(result.content);
    });

    it('accepts full update outside Tauri', async () => {
      const result = await updateCaptureSettings({
        enabled: true,
        sync: false,
        content: 'full'
      });

      expect(result.enabled).toBe(true);
      expect(result.sync).toBe(false);
      expect(result.content).toBe('full');
    });

    it('accepts empty update outside Tauri', async () => {
      const result = await updateCaptureSettings({});

      expect(result).toBeDefined();
      expect(['metadata_only', 'summary', 'full']).toContain(result.content);
    });
  });
});
