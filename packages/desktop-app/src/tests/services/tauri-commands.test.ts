/**
 * Tauri System Commands Service Tests
 *
 * Tests the non-node commands that remain in tauri-commands after C1a.
 * Node-CRUD functions were removed; use backend-adapter directly.
 */

import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import * as tauriCommands from '$lib/services/tauri-commands';

describe('Tauri System Commands - API Surface', () => {
  describe('Non-node command exports', () => {
    it('should export local agent functions', () => {
      expect(typeof tauriCommands.localAgentStatus).toBe('function');
      expect(typeof tauriCommands.localAgentCancelTurn).toBe('function');
      expect(typeof tauriCommands.ensureModelReady).toBe('function');
    });

    it('should export chat model functions', () => {
      expect(typeof tauriCommands.chatModelList).toBe('function');
      expect(typeof tauriCommands.chatModelRecommended).toBe('function');
      expect(typeof tauriCommands.chatModelDownload).toBe('function');
      expect(typeof tauriCommands.chatModelCancelDownload).toBe('function');
      expect(typeof tauriCommands.chatModelDelete).toBe('function');
      expect(typeof tauriCommands.chatModelLoad).toBe('function');
      expect(typeof tauriCommands.chatModelUnload).toBe('function');
      expect(typeof tauriCommands.getSystemRamGb).toBe('function');
      expect(typeof tauriCommands.ensureModelReady).toBe('function');
    });

    it('should export PTY session functions', () => {
      expect(typeof tauriCommands.ptyLaunchSession).toBe('function');
      expect(typeof tauriCommands.ptyWriteInput).toBe('function');
      expect(typeof tauriCommands.ptyResizeTerminal).toBe('function');
      expect(typeof tauriCommands.ptyTerminateSession).toBe('function');
      expect(typeof tauriCommands.ptyListSessions).toBe('function');
    });

    it('should export capture settings functions', () => {
      expect(typeof tauriCommands.getCaptureSettings).toBe('function');
      expect(typeof tauriCommands.updateCaptureSettings).toBe('function');
    });

    it('should export agent availability function', () => {
      expect(typeof tauriCommands.ptyCheckAgentAvailability).toBe('function');
    });

    it('should export ACP stub functions', () => {
      expect(typeof tauriCommands.acpListAgents).toBe('function');
      expect(typeof tauriCommands.acpStartSession).toBe('function');
      expect(typeof tauriCommands.acpSendMessage).toBe('function');
      expect(typeof tauriCommands.acpEndSession).toBe('function');
      expect(typeof tauriCommands.acpRefreshAgents).toBe('function');
    });
  });

  describe('Non-Tauri fallbacks (outside desktop)', () => {
    beforeEach(() => {
      vi.stubGlobal('fetch', vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve([]),
      }));
    });

    afterEach(() => {
      vi.unstubAllGlobals();
    });

    it('localAgentStatus returns idle outside Tauri', async () => {
      const result = await tauriCommands.localAgentStatus();
      expect(result).toEqual({ status: 'idle' });
    });

    it('chatModelList returns empty array outside Tauri', async () => {
      const result = await tauriCommands.chatModelList();
      expect(Array.isArray(result)).toBe(true);
    });

    it('getCaptureSettings returns defaults outside Tauri', async () => {
      const result = await tauriCommands.getCaptureSettings();
      expect(result).toHaveProperty('enabled');
      expect(result).toHaveProperty('sync');
      expect(result).toHaveProperty('content');
    });
  });
});
