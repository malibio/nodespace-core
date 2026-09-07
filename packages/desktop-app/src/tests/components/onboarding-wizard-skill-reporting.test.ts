/**
 * After `configure_skill` resolves, the wizard must name which agents
 * actually got the skill, not just say "Claude Code skill" regardless of
 * what happened. The bug this covers had a correct multi-agent install
 * (Claude Code AND Gemini CLI) reading as if Gemini had never been touched
 * at all, because nothing in the UI ever showed the real per-agent result.
 */

import { describe, it, expect, beforeEach, vi } from 'vitest';
import { tick } from 'svelte';
import { render, fireEvent } from '@testing-library/svelte';

const mockInvoke = vi.fn();
import { mockTauriCore } from '../helpers/mock-tauri-core';

vi.mock('@tauri-apps/api/core', () =>
  mockTauriCore({ invoke: (...args: unknown[]) => mockInvoke(...args) })
);

import OnboardingWizard from '$lib/components/onboarding/onboarding-wizard.svelte';

function buttonByText(root: HTMLElement, text: string): HTMLElement {
  const btn = Array.from(root.querySelectorAll<HTMLElement>('button')).find(
    (b) => b.textContent?.trim() === text
  );
  if (!btn) throw new Error(`button "${text}" not found`);
  return btn;
}

describe('OnboardingWizard skill-install reporting', () => {
  let onClose: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    onClose = vi.fn();
    mockInvoke.mockReset();
  });

  it('names every agent actually installed into, not just Claude Code', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_onboarding_status') {
        return Promise.resolve({
          completed: false,
          pathConfigured: false,
          skillConfigured: false,
          claudeCodeDetected: true,
          pathAlreadyConfigured: true // path step auto-advances
        });
      }
      if (cmd === 'configure_skill') {
        return Promise.resolve({
          success: true,
          agentsInstalled: ['claude-code', 'gemini'],
          agentsSkipped: [],
          cliOnPath: true,
          cliWarning: null,
          error: null,
          failureIsNew: false
        });
      }
      return Promise.resolve();
    });

    const { container } = render(OnboardingWizard, { props: { open: true, onClose } });
    await tick(); // path step already configured -> shows "Next"

    await fireEvent.click(buttonByText(container, 'Next')); // -> skill step
    await tick();

    await fireEvent.click(buttonByText(container, 'Add Skill'));
    await tick();
    await tick(); // let the invoke() promise resolve and stepSuccess flip

    const banner = container.querySelector('.success-banner');
    expect(banner?.textContent).toContain('Claude Code');
    expect(banner?.textContent).toContain('Gemini CLI');

    await fireEvent.click(buttonByText(container, 'Next')); // -> summary step
    await tick();

    const summary = container.querySelector('.summary-list');
    expect(summary?.textContent).toContain('Claude Code');
    expect(summary?.textContent).toContain('Gemini CLI');
  });

  it('reports a detected-but-skipped agent with its reason, distinct from a successful install', async () => {
    mockInvoke.mockImplementation((cmd: string) => {
      if (cmd === 'check_onboarding_status') {
        return Promise.resolve({
          completed: false,
          pathConfigured: false,
          skillConfigured: false,
          claudeCodeDetected: true,
          pathAlreadyConfigured: true
        });
      }
      if (cmd === 'configure_skill') {
        return Promise.resolve({
          success: true,
          agentsInstalled: ['claude-code'],
          agentsSkipped: [
            { agent: 'codex', reason: 'detected but no files to install (package may be incomplete)' }
          ],
          cliOnPath: true,
          cliWarning: null,
          error: null,
          failureIsNew: false
        });
      }
      return Promise.resolve();
    });

    const { container } = render(OnboardingWizard, { props: { open: true, onClose } });
    await tick();

    await fireEvent.click(buttonByText(container, 'Next'));
    await tick();
    await fireEvent.click(buttonByText(container, 'Add Skill'));
    await tick();
    await tick();

    expect(container.textContent).toContain('Codex');
    expect(container.textContent).toContain('detected but no files to install');
  });
});
