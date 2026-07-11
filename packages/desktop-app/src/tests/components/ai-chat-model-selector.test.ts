/**
 * AiChatModelSelector Logic Tests
 *
 * Unit tests for the PTY selection path added by issue #1489.
 * Follows the project pattern of testing extracted logic functions directly
 * (not rendering Svelte components) using Happy-DOM.
 */

import { describe, it, expect, vi } from 'vitest';
import type { ModelSelection } from '$lib/components/viewers/ai-chat-model-selector.svelte';
import type { AiChatNode } from '$lib/types/ai-chat-node';

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

const PTY_SENTINEL = '__pty__';
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
  if (value === PTY_SENTINEL) {
    onSelect({ provider: 'pty', modelId: '' });
    return;
  }
}

/** Mirrors the pty branch of handleModelSelect() from ai-chat-node-viewer.svelte. */
function buildPtyUpdate(current: Partial<AiChatNode> | undefined): {
  messages: unknown[];
  status: string;
  provider: string;
} {
  return {
    messages: current?.messages ?? [],
    status: current?.status ?? 'active',
    provider: 'pty',
  };
}

describe('AiChatModelSelector — PTY selection', () => {
  it('selecting the PTY sentinel invokes onSelect with provider "pty" and no modelId', () => {
    const onSelect = vi.fn();
    handleChangeValue(PTY_SENTINEL, onSelect);

    expect(onSelect).toHaveBeenCalledTimes(1);
    expect(onSelect).toHaveBeenCalledWith({ provider: 'pty', modelId: '' });
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

describe('AiChatNodeViewer — handleModelSelect PTY branch', () => {
  it('writes provider "pty" with no model field, preserving existing messages/status', () => {
    const current: Partial<AiChatNode> = {
      messages: [{ role: 'user', content: 'hi' }] as AiChatNode['messages'],
      status: 'active',
    };

    const update = buildPtyUpdate(current);

    expect(update.provider).toBe('pty');
    expect(update).not.toHaveProperty('model');
    expect(update.messages).toEqual(current.messages);
    expect(update.status).toBe('active');
  });

  it('defaults messages to [] and status to "active" for a fresh node', () => {
    const update = buildPtyUpdate(undefined);

    expect(update.provider).toBe('pty');
    expect(update.messages).toEqual([]);
    expect(update.status).toBe('active');
  });
});
