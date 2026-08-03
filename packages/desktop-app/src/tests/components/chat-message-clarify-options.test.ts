/* global HTMLButtonElement */
/**
 * ChatMessage Component Tests — route_clarify option chip rendering (#1930)
 *
 * Verifies that a clarify turn's structured `options` render as clickable
 * choices distinct from ordinary markdown, that clicking one invokes
 * `onSelectOption`, and that options are only clickable on the latest message.
 */

import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@testing-library/svelte';
import ChatMessage from '$lib/components/chat/chat-message.svelte';
import type { DisplayMessage } from '$lib/components/chat/types';

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
    content:
      'I can take that a couple of ways. Did you want to track debts or search notes?\n\n' +
      '- Track who owes me money\n- Search existing notes',
    toolExecutions: [],
    timestamp: 0,
    ...overrides,
  };
}

describe('ChatMessage clarify options', () => {
  it('renders each option as a clickable button, not markdown bullet prose', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({
        question: 'Did you want to track debts or search notes?',
        options: ['Track who owes me money', 'Search existing notes'],
      }),
      isLatest: true,
      onSelectOption: vi.fn(),
    });

    const buttons = container.querySelectorAll('button.clarify-option');
    expect(buttons.length).toBe(2);
    expect(buttons[0].textContent?.trim()).toBe('Track who owes me money');
    expect(buttons[1].textContent?.trim()).toBe('Search existing notes');
    // The question itself still renders as the message text.
    expect(container.querySelector('.message-content')?.textContent).toContain(
      'Did you want to track debts or search notes?'
    );
  });

  it('calls onSelectOption with the chosen option text when clicked', async () => {
    const onSelectOption = vi.fn();
    const { container } = render(ChatMessage, {
      message: makeMessage({
        question: 'Did you want to track debts or search notes?',
        options: ['Track who owes me money', 'Search existing notes'],
      }),
      isLatest: true,
      onSelectOption,
    });

    const button = container.querySelectorAll('button.clarify-option')[1];
    await fireEvent.click(button);
    expect(onSelectOption).toHaveBeenCalledWith('Search existing notes');
  });

  it('renders no option chips for an ordinary reply with no options', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({ options: undefined, question: undefined }),
      isLatest: true,
      onSelectOption: vi.fn(),
    });
    expect(container.querySelector('.clarify-options')).toBeNull();
  });

  it('disables option buttons once the turn is no longer the latest message', () => {
    const { container } = render(ChatMessage, {
      message: makeMessage({
        question: 'Did you want to track debts or search notes?',
        options: ['Track who owes me money', 'Search existing notes'],
      }),
      isLatest: false,
      onSelectOption: vi.fn(),
    });

    const buttons = container.querySelectorAll('button.clarify-option');
    expect(buttons.length).toBe(2);
    for (const button of buttons) {
      expect((button as HTMLButtonElement).disabled).toBe(true);
    }
  });
});
