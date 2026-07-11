/**
 * Unit tests for AgentStore - agent selection, refresh, availability
 */

import { describe, it, expect, beforeEach, vi, afterEach } from 'vitest';
import { agentStore, availabilityToAgent } from '$lib/stores/agent-store.svelte';
import type { AgentAvailabilityInfo } from '$lib/services/tauri-commands';

// Mock the logger
vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

describe('AgentStore', () => {
  beforeEach(() => {
    agentStore.reset();
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  describe('Initial State', () => {
    it('starts with empty agents', () => {
      expect(agentStore.agents).toEqual([]);
    });

    it('starts with no selected agent', () => {
      expect(agentStore.selectedAgentId).toBeNull();
    });

    it('starts not loading', () => {
      expect(agentStore.isLoading).toBe(false);
    });

    it('reports no agents available', () => {
      expect(agentStore.hasAgents).toBe(false);
    });
  });

  describe('refreshAgents', () => {
    it('loads mock agents', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      expect(agentStore.agents.length).toBeGreaterThan(0);
      expect(agentStore.hasAgents).toBe(true);
    });

    it('sets loading state during refresh', async () => {
      const refreshPromise = agentStore.refreshAgents();
      expect(agentStore.isLoading).toBe(true);

      await vi.runAllTimersAsync();
      await refreshPromise;

      expect(agentStore.isLoading).toBe(false);
    });

    it('auto-selects first available agent', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      expect(agentStore.selectedAgentId).toBeTruthy();

      // The selected agent should be available
      const selected = agentStore.selectedAgent;
      expect(selected).toBeDefined();
      expect(selected!.available).toBe(true);
    });

    it('preserves existing selection on refresh', async () => {
      // First refresh to populate
      let promise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await promise;

      // Manually select a specific agent
      const agentId = agentStore.agents[0].id;
      agentStore.selectAgent(agentId);

      // Refresh again
      promise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await promise;

      // Selection should be preserved
      expect(agentStore.selectedAgentId).toBe(agentId);
    });
  });

  describe('selectAgent', () => {
    it('selects a valid agent', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      const agent = agentStore.agents[0];
      agentStore.selectAgent(agent.id);

      expect(agentStore.selectedAgentId).toBe(agent.id);
      expect(agentStore.selectedAgent).toEqual(agent);
    });

    it('ignores selection of unknown agent', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      const originalId = agentStore.selectedAgentId;
      agentStore.selectAgent('nonexistent-agent');

      expect(agentStore.selectedAgentId).toBe(originalId);
    });
  });

  describe('availableAgents', () => {
    it('filters to only available agents', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      const available = agentStore.availableAgents;
      expect(available.length).toBeGreaterThan(0);
      expect(available.length).toBeLessThanOrEqual(agentStore.agents.length);

      for (const agent of available) {
        expect(agent.available).toBe(true);
      }
    });
  });

  describe('selectedAgent', () => {
    it('returns undefined when nothing selected', () => {
      expect(agentStore.selectedAgent).toBeUndefined();
    });

    it('returns the selected agent object', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      const agent = agentStore.agents[0];
      agentStore.selectAgent(agent.id);

      const selected = agentStore.selectedAgent;
      expect(selected).toBeDefined();
      expect(selected!.id).toBe(agent.id);
      expect(selected!.name).toBe(agent.name);
    });
  });

  describe('availabilityToAgent', () => {
    it('maps a known agent type to its display label', () => {
      const info: AgentAvailabilityInfo = {
        agentType: 'claude-code',
        binary: 'claude',
        binaryFound: true,
        authFound: true,
        binaryPath: '/usr/local/bin/claude',
        installHint: null,
      };

      const agent = availabilityToAgent(info);

      expect(agent).toEqual({
        id: 'claude-code',
        name: 'Claude Code',
        binary: 'claude',
        args: [],
        auth_method: { method: 'agent_managed' },
        available: true,
      });
    });

    it('falls back to the raw agentType when no label is known', () => {
      const info: AgentAvailabilityInfo = {
        agentType: 'some-future-agent',
        binary: 'future-agent',
        binaryFound: true,
        authFound: true,
        binaryPath: '/usr/local/bin/future-agent',
        installHint: null,
      };

      expect(availabilityToAgent(info).name).toBe('some-future-agent');
    });

    it('is unavailable when the binary is missing', () => {
      const info: AgentAvailabilityInfo = {
        agentType: 'gemini-cli',
        binary: 'gemini',
        binaryFound: false,
        authFound: true,
        binaryPath: null,
        installHint: 'npm install -g @google/gemini-cli',
      };

      expect(availabilityToAgent(info).available).toBe(false);
    });

    it('is unavailable when auth is missing even if the binary is found', () => {
      const info: AgentAvailabilityInfo = {
        agentType: 'gemini-cli',
        binary: 'gemini',
        binaryFound: true,
        authFound: false,
        binaryPath: '/usr/local/bin/gemini',
        installHint: null,
      };

      expect(availabilityToAgent(info).available).toBe(false);
    });
  });

  describe('reset', () => {
    it('clears all state', async () => {
      const refreshPromise = agentStore.refreshAgents();
      await vi.runAllTimersAsync();
      await refreshPromise;

      expect(agentStore.agents.length).toBeGreaterThan(0);

      agentStore.reset();

      expect(agentStore.agents).toEqual([]);
      expect(agentStore.selectedAgentId).toBeNull();
      expect(agentStore.isLoading).toBe(false);
    });
  });
});
