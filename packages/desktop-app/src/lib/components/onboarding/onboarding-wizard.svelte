<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('OnboardingWizard');

  let {
    open = false,
    onClose,
  }: {
    open: boolean;
    onClose: () => void;
  } = $props();

  // ── types ──────────────────────────────────────────────────────────────────

  type WizardStep = 'path' | 'mcp' | 'skill' | 'summary';

  interface OnboardingStatus {
    completed: boolean;
    pathConfigured: boolean;
    mcpConfigured: boolean;
    skillConfigured: boolean;
    claudeDesktopDetected: boolean;
    claudeCodeDetected: boolean;
    pathAlreadyConfigured: boolean;
  }

  // ── state ──────────────────────────────────────────────────────────────────

  let currentStep = $state<WizardStep>('path');
  let isLoading = $state(false);
  let stepSuccess = $state(false);
  let stepError = $state<string | null>(null);

  // Which steps are active (some may be skipped if prerequisites missing)
  let showMcp = $state(false);
  let showSkill = $state(false);

  // What was actually configured (for summary)
  let pathDone = $state(false);
  let mcpDone = $state(false);
  let skillDone = $state(false);

  // Whether the PATH export was already present before we ran
  let pathWasAlreadyConfigured = $state(false);

  // ── derived step sequence ──────────────────────────────────────────────────

  const stepSequence = $derived(
    (() => {
      const steps: WizardStep[] = ['path'];
      if (showMcp) steps.push('mcp');
      if (showSkill) steps.push('skill');
      steps.push('summary');
      return steps;
    })()
  );

  function nextStep() {
    const seq = stepSequence;
    const idx = seq.indexOf(currentStep);
    if (idx !== -1 && idx < seq.length - 1) {
      currentStep = seq[idx + 1];
      stepSuccess = false;
      stepError = null;
    }
  }

  // ── mount: probe environment ───────────────────────────────────────────────

  onMount(() => {
    invoke<OnboardingStatus>('check_onboarding_status')
      .then((status) => {
        showMcp = status.claudeDesktopDetected;
        showSkill = status.claudeCodeDetected;
        pathWasAlreadyConfigured = status.pathAlreadyConfigured;
        log.debug('Onboarding status loaded', {
          showMcp,
          showSkill,
          pathAlreadyConfigured: status.pathAlreadyConfigured,
        });
      })
      .catch((err) => {
        log.warn('Could not load onboarding status', err);
      });
  });

  // ── step actions ───────────────────────────────────────────────────────────

  async function handleConfigurePath() {
    isLoading = true;
    stepError = null;
    try {
      await invoke('configure_path');
      pathDone = true;
      stepSuccess = true;
      log.info('PATH configured successfully');
    } catch (err) {
      stepError = err instanceof Error ? err.message : String(err);
      log.error('Failed to configure PATH', err);
    } finally {
      isLoading = false;
    }
  }

  async function handleConfigureMcp() {
    isLoading = true;
    stepError = null;
    try {
      await invoke('configure_mcp');
      mcpDone = true;
      stepSuccess = true;
      log.info('MCP configured successfully');
    } catch (err) {
      stepError = err instanceof Error ? err.message : String(err);
      log.error('Failed to configure MCP', err);
    } finally {
      isLoading = false;
    }
  }

  async function handleConfigureSkill() {
    isLoading = true;
    stepError = null;
    try {
      await invoke('configure_skill');
      skillDone = true;
      stepSuccess = true;
      log.info('Skill configured successfully');
    } catch (err) {
      stepError = err instanceof Error ? err.message : String(err);
      log.error('Failed to configure skill', err);
    } finally {
      isLoading = false;
    }
  }

  function skipCurrentStep() {
    log.debug('Skipped step', { step: currentStep });
    nextStep();
  }

  async function finishWizard() {
    try {
      await invoke('complete_onboarding', {
        pathConfigured: pathDone,
        mcpConfigured: mcpDone,
        skillConfigured: skillDone,
      });
      log.info('Onboarding completed', { pathDone, mcpDone, skillDone });
    } catch (err) {
      log.warn('Could not persist onboarding completion', err);
    }
    onClose();
  }

  // ── dialog keyboard / backdrop ─────────────────────────────────────────────

  function handleBackdropClick(event: MouseEvent) {
    if (event.target === event.currentTarget) {
      onClose();
    }
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === 'Escape') {
      onClose();
    }
  }
</script>

{#if open}
  <div
    class="onboarding-backdrop"
    onclick={handleBackdropClick}
    onkeydown={handleKeydown}
    role="dialog"
    aria-modal="true"
    aria-label="First-launch setup"
    tabindex="-1"
  >
    <div class="onboarding-dialog">
      <!-- Close button -->
      <button class="close-button" onclick={onClose} aria-label="Close dialog">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="18" height="18">
          <line x1="18" y1="6" x2="6" y2="18" />
          <line x1="6" y1="6" x2="18" y2="18" />
        </svg>
      </button>

      <!-- Step indicator -->
      <div class="step-indicator" aria-hidden="true">
        {#each stepSequence as step (step)}
          <span
            class="step-dot"
            class:active={currentStep === step}
            class:done={stepSequence.indexOf(step) < stepSequence.indexOf(currentStep)}
          ></span>
        {/each}
      </div>

      <!-- ── PATH step ──────────────────────────────────────────────────── -->
      {#if currentStep === 'path'}
        <div class="onboarding-header">
          <h2>Add NodeSpace to your terminal?</h2>
          <p>
            Adds <code>~/.nodespace/bin</code> to your <code>PATH</code> so you can run
            <code>nodespace</code> from any terminal.
          </p>
        </div>

        {#if pathWasAlreadyConfigured}
          <div class="info-banner">
            Already configured — your shell profile already includes the NodeSpace path.
          </div>
          <div class="step-actions">
            <button class="primary-button" onclick={nextStep}>Next</button>
          </div>
        {:else if stepSuccess}
          <div class="success-banner">
            Added to <code>~/.zshrc</code> and/or <code>~/.bash_profile</code>. Open a new terminal
            to apply.
          </div>
          <div class="step-actions">
            <button class="primary-button" onclick={nextStep}>Next</button>
          </div>
        {:else}
          {#if stepError}
            <div class="error-banner">{stepError}</div>
          {/if}
          <div class="step-actions">
            <button class="primary-button" onclick={handleConfigurePath} disabled={isLoading}>
              {isLoading ? 'Configuring…' : 'Add to PATH'}
            </button>
            <button class="skip-button" onclick={skipCurrentStep} disabled={isLoading}>Skip</button>
          </div>
        {/if}
      {/if}

      <!-- ── MCP step ───────────────────────────────────────────────────── -->
      {#if currentStep === 'mcp'}
        <div class="onboarding-header">
          <h2>Connect NodeSpace to Claude?</h2>
          <p>
            Registers NodeSpace as an MCP server in Claude Desktop so Claude can read and write
            your knowledge graph directly.
          </p>
        </div>

        {#if stepSuccess}
          <div class="success-banner">
            NodeSpace added to <code>claude_desktop_config.json</code>. Restart Claude Desktop to
            activate.
          </div>
          <div class="step-actions">
            <button class="primary-button" onclick={nextStep}>Next</button>
          </div>
        {:else}
          {#if stepError}
            <div class="error-banner">{stepError}</div>
          {/if}
          <div class="step-actions">
            <button class="primary-button" onclick={handleConfigureMcp} disabled={isLoading}>
              {isLoading ? 'Configuring…' : 'Connect Claude Desktop'}
            </button>
            <button class="skip-button" onclick={skipCurrentStep} disabled={isLoading}>Skip</button>
          </div>
        {/if}
      {/if}

      <!-- ── Skill step ─────────────────────────────────────────────────── -->
      {#if currentStep === 'skill'}
        <div class="onboarding-header">
          <h2>Add NodeSpace to Claude Code?</h2>
          <p>
            Installs a skill file at <code>~/.claude/skills/nodespace/SKILL.md</code> so Claude
            Code knows how to interact with your knowledge graph.
          </p>
        </div>

        {#if stepSuccess}
          <div class="success-banner">
            Skill file written. Claude Code will pick it up automatically on the next session.
          </div>
          <div class="step-actions">
            <button class="primary-button" onclick={nextStep}>Next</button>
          </div>
        {:else}
          {#if stepError}
            <div class="error-banner">{stepError}</div>
          {/if}
          <div class="step-actions">
            <button class="primary-button" onclick={handleConfigureSkill} disabled={isLoading}>
              {isLoading ? 'Installing…' : 'Add Skill'}
            </button>
            <button class="skip-button" onclick={skipCurrentStep} disabled={isLoading}>Skip</button>
          </div>
        {/if}
      {/if}

      <!-- ── Summary step ───────────────────────────────────────────────── -->
      {#if currentStep === 'summary'}
        <div class="onboarding-header">
          <h2>You're all set!</h2>
          <p>Here's a summary of what was configured.</p>
        </div>

        <ul class="summary-list">
          <li class:configured={pathDone || pathWasAlreadyConfigured} class:skipped={!pathDone && !pathWasAlreadyConfigured}>
            <span class="summary-icon">
              {#if pathDone || pathWasAlreadyConfigured}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                  <polyline points="20 6 9 17 4 12" />
                </svg>
              {:else}
                <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                  <line x1="18" y1="6" x2="6" y2="18" />
                  <line x1="6" y1="6" x2="18" y2="18" />
                </svg>
              {/if}
            </span>
            <span>
              Terminal PATH
              {#if pathWasAlreadyConfigured && !pathDone}
                <span class="summary-note">(already configured)</span>
              {:else if !pathDone}
                <span class="summary-note">(skipped)</span>
              {/if}
            </span>
          </li>

          {#if showMcp}
            <li class:configured={mcpDone} class:skipped={!mcpDone}>
              <span class="summary-icon">
                {#if mcpDone}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                    <line x1="18" y1="6" x2="6" y2="18" />
                    <line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                {/if}
              </span>
              <span>
                Claude Desktop MCP
                {#if !mcpDone}<span class="summary-note">(skipped)</span>{/if}
              </span>
            </li>
          {/if}

          {#if showSkill}
            <li class:configured={skillDone} class:skipped={!skillDone}>
              <span class="summary-icon">
                {#if skillDone}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                    <polyline points="20 6 9 17 4 12" />
                  </svg>
                {:else}
                  <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" width="14" height="14">
                    <line x1="18" y1="6" x2="6" y2="18" />
                    <line x1="6" y1="6" x2="18" y2="18" />
                  </svg>
                {/if}
              </span>
              <span>
                Claude Code skill
                {#if !skillDone}<span class="summary-note">(skipped)</span>{/if}
              </span>
            </li>
          {/if}
        </ul>

        <p class="settings-hint">
          You can revisit these integrations at any time in
          <strong>Settings &rarr; Integrations</strong>.
        </p>

        <div class="step-actions">
          <button class="primary-button" onclick={finishWizard}>Open NodeSpace</button>
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .onboarding-backdrop {
    position: fixed;
    inset: 0;
    background: hsl(0 0% 0% / 0.5);
    display: flex;
    align-items: center;
    justify-content: center;
    z-index: 200;
    padding: 1rem;
  }

  .onboarding-dialog {
    background: hsl(var(--background));
    border: 1px solid hsl(var(--border));
    border-radius: 0.75rem;
    padding: 2rem;
    max-width: 30rem;
    width: 100%;
    position: relative;
    max-height: 85vh;
    overflow-y: auto;
    box-shadow: 0 12px 40px hsl(0 0% 0% / 0.15);
  }

  .close-button {
    position: absolute;
    top: 0.75rem;
    right: 0.75rem;
    background: none;
    border: none;
    cursor: pointer;
    color: hsl(var(--muted-foreground));
    padding: 0.25rem;
    border-radius: 0.25rem;
    display: flex;
    align-items: center;
  }

  .close-button:hover {
    color: hsl(var(--foreground));
  }

  /* Step dots */
  .step-indicator {
    display: flex;
    gap: 0.375rem;
    margin-bottom: 1.5rem;
  }

  .step-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: hsl(var(--muted-foreground) / 0.3);
    transition: background 0.15s;
  }

  .step-dot.active {
    background: hsl(var(--primary));
    width: 18px;
    border-radius: 3px;
  }

  .step-dot.done {
    background: hsl(var(--primary) / 0.5);
  }

  /* Header */
  .onboarding-header {
    margin-bottom: 1.5rem;
  }

  .onboarding-header h2 {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0 0 0.375rem;
    color: hsl(var(--foreground));
  }

  .onboarding-header p {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    line-height: 1.5;
  }

  .onboarding-header code {
    font-size: 0.8125rem;
    background: hsl(var(--muted));
    padding: 0.1em 0.3em;
    border-radius: 3px;
    color: hsl(var(--foreground));
  }

  /* Banners */
  .success-banner {
    font-size: 0.875rem;
    color: hsl(142 76% 30%);
    background: hsl(142 76% 36% / 0.1);
    border: 1px solid hsl(142 76% 36% / 0.25);
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1.25rem;
    line-height: 1.5;
  }

  .success-banner code {
    font-size: 0.8125rem;
    background: hsl(142 76% 36% / 0.12);
    padding: 0.1em 0.3em;
    border-radius: 3px;
  }

  .info-banner {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted) / 0.5);
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1.25rem;
    line-height: 1.5;
  }

  .error-banner {
    font-size: 0.875rem;
    color: hsl(var(--destructive-foreground));
    background: hsl(var(--destructive) / 0.1);
    border: 1px solid hsl(var(--destructive) / 0.3);
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1.25rem;
    line-height: 1.5;
  }

  /* Actions */
  .step-actions {
    display: flex;
    align-items: center;
    gap: 0.75rem;
  }

  .primary-button {
    padding: 0.5rem 1.25rem;
    border-radius: 0.375rem;
    border: none;
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: opacity 0.15s;
  }

  .primary-button:hover:not(:disabled) {
    opacity: 0.9;
  }

  .primary-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .skip-button {
    background: none;
    border: none;
    cursor: pointer;
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    padding: 0.5rem 0.25rem;
  }

  .skip-button:hover:not(:disabled) {
    color: hsl(var(--foreground));
  }

  .skip-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  /* Summary */
  .summary-list {
    list-style: none;
    margin: 0 0 1.25rem;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.625rem;
  }

  .summary-list li {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
  }

  .summary-list li.configured {
    color: hsl(var(--foreground));
  }

  .summary-icon {
    display: flex;
    align-items: center;
    flex-shrink: 0;
  }

  .summary-list li.configured .summary-icon {
    color: hsl(142 76% 36%);
  }

  .summary-list li.skipped .summary-icon {
    color: hsl(var(--muted-foreground) / 0.5);
  }

  .summary-note {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin-left: 0.25rem;
  }

  .settings-hint {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0 0 1.5rem;
    line-height: 1.5;
  }
</style>
