/**
 * AiChatModelSelector Logic Tests
 *
 * Unit tests for the PTY agent selection path (issue #1450, building on the
 * generic PTY entry point added by issue #1489).
 * Follows the project pattern of testing extracted logic functions directly
 * (not rendering Svelte components) using Happy-DOM.
 */

import { describe, it, expect, vi } from 'vitest';
import type { ModelSelection } from '$lib/components/viewers/ai-chat-model-selector.svelte';
import type { AiChatNode } from '$lib/types/ai-chat-node';
import { isLocalAgent } from '$lib/stores/agent-store.svelte';
import type { AcpAgentInfo } from '$lib/types/agent-types';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

const PTY_PREFIX = 'pty:';
const SETUP_SENTINEL = '__setup__';
const HEADER_SENTINEL_PREFIX = '__header__';

/** Mirrors handleChange() from ai-chat-model-selector.svelte. */
function handleChangeValue(
  value: string,
  onSelect: (selection: ModelSelection) => void
): void {
  if (!value || value.startsWith(HEADER_SENTINEL_PREFIX)) return;
  if (value === SETUP_SENTINEL) return;

  if (value.startsWith('native:')) {
    onSelect({ provider: 'native', modelId: value.slice('native:'.length) });
    return;
  }
  if (value.startsWith('ollama:')) {
    onSelect({ provider: 'ollama', modelId: value });
    return;
  }
  if (value.startsWith('openai-compat:')) {
    onSelect({
      provider: 'openai-compat',
      modelId: value.slice('openai-compat:'.length),
      configId: value.slice('openai-compat:'.length),
    });
    return;
  }
  if (value.startsWith(PTY_PREFIX)) {
    onSelect({ provider: 'pty', modelId: value.slice(PTY_PREFIX.length) });
    return;
  }
}

/** Mirrors the pty branch of handleModelSelect() from ai-chat-node-viewer.svelte. */
function buildPtyUpdate(
  current: Partial<AiChatNode> | undefined,
  agentId: string
): {
  messages: unknown[];
  status: string;
  provider: string;
  model: string | null;
} {
  return {
    messages: current?.messages ?? [],
    status: current?.status ?? 'active',
    provider: 'pty',
    model: agentId || null,
  };
}

/** Mirrors the ptyAgents/availablePtyAgents derivations in ai-chat-model-selector.svelte. */
function ptyAgents(agents: AcpAgentInfo[]): AcpAgentInfo[] {
  return agents.filter((a) => !isLocalAgent(a.id));
}

describe('AiChatModelSelector — PTY agent selection', () => {
  it('selecting a PTY agent invokes onSelect with provider "pty" and the agent id as modelId', () => {
    const onSelect = vi.fn();
    handleChangeValue(`${PTY_PREFIX}claude-code`, onSelect);

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith({ provider: 'pty', modelId: 'claude-code' });
  });

  it('does not invoke onSelect for header or setup sentinels', () => {
    const onSelect = vi.fn();
    handleChangeValue(`${HEADER_SENTINEL_PREFIX}no-local`, onSelect);
    handleChangeValue(SETUP_SENTINEL, onSelect);

    expect(onSelect).not.toHaveBeenCalled();
  });

  it('other provider selections remain unaffected by the PTY addition', () => {
    const onSelect = vi.fn();
    handleChangeValue('native:qwen2.5-3b', onSelect);
    expect(onSelect).toHaveBeenCalledWith({ provider: 'native', modelId: 'qwen2.5-3b' });

    onSelect.mockClear();
    handleChangeValue('ollama:llama3.2:latest', onSelect);
    expect(onSelect).toHaveBeenCalledWith({
      provider: 'ollama',
      modelId: 'ollama:llama3.2:latest',
    });
  });
});

describe('AiChatModelSelector — PTY agent list derivation', () => {
  const agents: AcpAgentInfo[] = [
    {
      id: 'local:ministral-3b-q4km',
      name: 'Ministral 3B Instruct Q4_K_M',
      binary: 'local',
      args: [],
      auth_method: { method: 'agent_managed' },
      available: true,
    },
    {
      id: 'claude-code',
      name: 'Claude Code',
      binary: 'claude',
      args: [],
      auth_method: { method: 'agent_managed' },
      available: true,
    },
    {
      id: 'gemini-cli',
      name: 'Gemini CLI',
      binary: 'gemini',
      args: [],
      auth_method: { method: 'env_api_key', var_name: 'GEMINI_API_KEY' },
      available: false,
    },
  ];

  it('excludes local model agents from the PTY Agents section', () => {
    const result = ptyAgents(agents);
    expect(result.map((a) => a.id)).toEqual(['claude-code', 'gemini-cli']);
  });

  it('keeps unavailable PTY agents in the list (rendered disabled, not hidden)', () => {
    const result = ptyAgents(agents);
    const gemini = result.find((a) => a.id === 'gemini-cli');
    expect(gemini?.available).toBe(false);
  });
});

describe('AiChatNodeViewer — handleModelSelect PTY branch', () => {
  it('writes provider "pty" with the selected agent id as model, preserving existing messages/status', () => {
    const current: Partial<AiChatNode> = {
      messages: [{ role: 'user', content: 'hi' }] as AiChatNode['messages'],
      status: 'active',
    };

    const update = buildPtyUpdate(current, 'claude-code');

    expect(update.provider).toBe('pty');
    expect(update.model).toBe('claude-code');
    expect(update.messages).toEqual(current.messages);
    expect(update.status).toBe('active');
  });

  it('defaults messages to [] and status to "active" for a fresh node', () => {
    const update = buildPtyUpdate(undefined, 'gemini-cli');

    expect(update.provider).toBe('pty');
    expect(update.model).toBe('gemini-cli');
    expect(update.messages).toEqual([]);
    expect(update.status).toBe('active');
  });

  it('nulls model when no agent id is provided', () => {
    const current: Partial<AiChatNode> = {
      provider: 'native',
      model: 'qwen2.5-3b',
      messages: [],
      status: 'active',
    };

    const update = buildPtyUpdate(current, '');

    expect(update.model).toBeNull();
  });
});
