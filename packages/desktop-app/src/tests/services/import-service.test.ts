import { describe, it, expect, vi, beforeEach } from 'vitest';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn()
}));

vi.mock('@tauri-apps/plugin-dialog', () => ({
  open: vi.fn()
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn()
}));

import { invoke } from '@tauri-apps/api/core';
import { open } from '@tauri-apps/plugin-dialog';
import { listen } from '@tauri-apps/api/event';
import { importService } from '$lib/services/import-service';
import type { ImportProgressEvent } from '$lib/services/import-service';

// Narrow, test-only view of ImportService's private fields so we can reset
// singleton state between tests without reaching for `any`.
interface ImportServiceInternals {
  unlistenProgress: (() => void) | null;
  progressListeners: Set<(event: ImportProgressEvent) => void>;
}

describe('ImportService', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
    vi.mocked(open).mockReset();
    vi.mocked(listen).mockReset();
    // Reset internal listener state between tests since importService is a singleton.
    const internals = importService as unknown as ImportServiceInternals;
    internals.unlistenProgress = null;
    internals.progressListeners = new Set();
  });

  describe('selectFolder', () => {
    it('returns the selected path on success', async () => {
      vi.mocked(open).mockResolvedValue('/Users/test/Documents');

      const result = await importService.selectFolder();

      expect(result).toBe('/Users/test/Documents');
      expect(open).toHaveBeenCalledWith({
        directory: true,
        multiple: false,
        title: 'Select folder to import'
      });
    });

    it('returns null when the dialog is cancelled', async () => {
      vi.mocked(open).mockResolvedValue(null);

      const result = await importService.selectFolder();

      expect(result).toBeNull();
    });

    it('returns null and does not throw when the dialog throws', async () => {
      vi.mocked(open).mockRejectedValue(new Error('dialog failed'));

      const result = await importService.selectFolder();

      expect(result).toBeNull();
    });
  });

  describe('importDirectory', () => {
    it('sets up the progress listener and invokes with correct args', async () => {
      vi.mocked(listen).mockResolvedValue(vi.fn());
      const batchResult = {
        total_files: 3,
        successful: 3,
        failed: 0,
        results: [],
        duration_ms: 42
      };
      vi.mocked(invoke).mockResolvedValue(batchResult);

      const result = await importService.importDirectory('/some/dir', { collection: 'notes' });

      expect(listen).toHaveBeenCalledWith('import-progress', expect.any(Function));
      expect(invoke).toHaveBeenCalledWith('import_markdown_directory', {
        directoryPath: '/some/dir',
        options: { collection: 'notes' }
      });
      expect(result).toEqual(batchResult);
    });

    it('defaults options to an empty object when omitted', async () => {
      vi.mocked(listen).mockResolvedValue(vi.fn());
      vi.mocked(invoke).mockResolvedValue({
        total_files: 0,
        successful: 0,
        failed: 0,
        results: [],
        duration_ms: 0
      });

      await importService.importDirectory('/some/dir');

      expect(invoke).toHaveBeenCalledWith('import_markdown_directory', {
        directoryPath: '/some/dir',
        options: {}
      });
    });
  });

  describe('importFile', () => {
    it('invokes with correct args and logs success on success', async () => {
      const fileResult = {
        file_path: '/a/b.md',
        root_id: 'node-1',
        nodes_created: 2,
        success: true,
        error: null,
        collection: null,
        archived: false
      };
      vi.mocked(invoke).mockResolvedValue(fileResult);

      const result = await importService.importFile('/a/b.md', { use_filename_as_title: true });

      expect(invoke).toHaveBeenCalledWith('import_markdown_file', {
        filePath: '/a/b.md',
        options: { use_filename_as_title: true }
      });
      expect(result).toEqual(fileResult);
    });

    it('returns the failed result without throwing on failure', async () => {
      const fileResult = {
        file_path: '/a/bad.md',
        root_id: null,
        nodes_created: 0,
        success: false,
        error: 'parse error',
        collection: null,
        archived: false
      };
      vi.mocked(invoke).mockResolvedValue(fileResult);

      const result = await importService.importFile('/a/bad.md');

      expect(result).toEqual(fileResult);
      expect(result.success).toBe(false);
    });
  });

  describe('onProgress', () => {
    it('subscribes and receives emitted events, then unsubscribe stops delivery', async () => {
      let capturedHandler: ((event: { payload: ImportProgressEvent }) => void) | undefined;
      vi.mocked(listen).mockImplementation(async (_eventName, handler) => {
        capturedHandler = handler as (event: { payload: ImportProgressEvent }) => void;
        return vi.fn();
      });
      vi.mocked(invoke).mockResolvedValue({
        total_files: 0,
        successful: 0,
        failed: 0,
        results: [],
        duration_ms: 0
      });

      const callback = vi.fn();
      const unsubscribe = importService.onProgress(callback);

      await importService.importDirectory('/some/dir');

      expect(capturedHandler).toBeDefined();
      const progressEvent: ImportProgressEvent = {
        step: 2,
        step_name: 'reading',
        message: 'Reading: a.md',
        current: 1,
        total: 3
      };
      capturedHandler?.({ payload: progressEvent });

      expect(callback).toHaveBeenCalledWith(progressEvent);

      unsubscribe();
      callback.mockClear();
      capturedHandler?.({ payload: progressEvent });

      expect(callback).not.toHaveBeenCalled();
    });
  });

  describe('setupProgressListener guard', () => {
    it('only calls listen once across two importDirectory calls', async () => {
      vi.mocked(listen).mockResolvedValue(vi.fn());
      vi.mocked(invoke).mockResolvedValue({
        total_files: 0,
        successful: 0,
        failed: 0,
        results: [],
        duration_ms: 0
      });

      await importService.importDirectory('/dir-a');
      await importService.importDirectory('/dir-b');

      expect(listen).toHaveBeenCalledTimes(1);
    });
  });
});
