/**
 * status-bar component.
 *
 * Regression: a long-lived error message (statusBar.error() never
 * auto-dismisses, unlike .success()'s 5s timeout) with no wrap/truncate/
 * tooltip handling overflowed the fixed-height bar and clipped its own
 * actionable content — the exact shape of the "neither bun nor node found"
 * dual-runtime message. The full text must stay reachable via a `title`
 * tooltip even once the visible line is truncated.
 */
import { describe, it, expect, afterEach } from 'vitest';
import { render, cleanup } from '@testing-library/svelte';
import { statusBar } from '$lib/stores/status-bar.svelte';
import StatusBar from '$lib/components/status-bar.svelte';

describe('StatusBar', () => {
  afterEach(() => {
    cleanup();
    statusBar.clearMessage();
    statusBar.setEnabled(true);
  });

  it('carries the full message as a title attribute so a truncated line stays fully readable on hover', () => {
    const longMessage =
      'Neither `bun` nor `node` was found on $PATH. One of them is required to install ' +
      "NodeSpace's AI-agent integrations (Claude Code, Codex, Antigravity CLI, OpenCode). " +
      'Install Node from https://nodejs.org (or Bun from https://bun.sh) and relaunch ' +
      "NodeSpace — or ignore this if you don't use one of those agents.";
    statusBar.error(longMessage);

    const { container } = render(StatusBar);
    const messageEl = container.querySelector('.message');

    expect(messageEl).toBeTruthy();
    expect(messageEl?.textContent).toBe(longMessage);
    expect(messageEl?.getAttribute('title')).toBe(longMessage);
  });

  it('does not render a title attribute when there is no message', () => {
    statusBar.show('');

    const { container } = render(StatusBar);

    expect(container.querySelector('.message')).toBeNull();
  });

  it('a short success message still round-trips through the title attribute', () => {
    statusBar.success('Import complete');

    const { container } = render(StatusBar);
    const messageEl = container.querySelector('.message');

    expect(messageEl?.textContent).toBe('Import complete');
    expect(messageEl?.getAttribute('title')).toBe('Import complete');
  });
});
