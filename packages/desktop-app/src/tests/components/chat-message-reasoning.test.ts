/**
 * ChatMessage Component Tests — reasoning section rendering
 *
 * Verifies that the model's captured chain-of-thought renders as a collapsible
 * section (collapsed by default) under an assistant answer, and is absent for
 * clean answers or user messages.
 */

import { describe, it, expect, vi } from 'vitest';
import { render } from '@testing-library/svelte';
import ChatMessage from '$lib/components/chat/chat-message.svelte';
import type { DisplayMessage } from '$lib/components/chat/types';

// ChatMarkdown (rendered inside the bubble) uses the logger.
vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn(),
  }),
}));

function makeMessage(overrides: Partial<DisplayMessage>): DisplayMessage {
  return {
    id: 'm1',
    role: 'assistant',
    content: 'Here is your answer.',
    toolExecutions: [],
    timestamp: 0,
    ...overrides,
  };
}

describe('ChatMessage reasoning section', () => {
  it('renders a collapsed reasoning section for an assistant message with reasoning', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({ reasoning: 'I chose create_schema because the user wants a type.' }),
    });

    const details = container.querySelector('details.reasoning-block');
    expect(details).not.toBeNull();
    // Collapsed by default: a <details> without the `open` attribute.
    expect(details?.hasAttribute('open')).toBe(false);
    expect(container.querySelector('.reasoning-summary')?.textContent).toContain('Reasoning');
    expect(container.querySelector('.reasoning-content')?.textContent).toContain('create_schema');
  });

  it('renders no reasoning section when the assistant message has none', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({ reasoning: undefined }),
    });
    expect(container.querySelector('details.reasoning-block')).toBeNull();
  });

  it('renders no reasoning section for a user message even if reasoning is set', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({ role: 'user', content: 'Hi', reasoning: 'should not show' }),
    });
    expect(container.querySelector('details.reasoning-block')).toBeNull();
  });
});
