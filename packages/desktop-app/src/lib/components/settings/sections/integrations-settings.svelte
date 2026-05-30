<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('IntegrationsSettings');

  interface IntegrationStatus {
    claudeCodeDetected: boolean;
    pathAlreadyConfigured: boolean;
  }

  interface SkillSetupResult {
    success: boolean;
    agentsInstalled: string[];
    cliOnPath: boolean;
    cliWarning: string | null;
    error: string | null;
  }

  let status = $state<IntegrationStatus | null>(null);
  let skillResult = $state<SkillSetupResult | null>(null);

  let pathWorking = $state(false);
  let skillWorking = $state(false);
  let pathFeedback = $state<{ ok: boolean; message: string } | null>(null);
  let skillFeedback = $state<{ ok: boolean; message: string } | null>(null);

  async function loadStatus() {
    try {
      status = await invoke<IntegrationStatus>('get_integrations_status');
      skillResult = await invoke<SkillSetupResult>('get_skill_setup_status');
    } catch (err) {
      log.warn('Could not load integration status', err);
    }
  }

  onMount(loadStatus);

  // ── PATH ──────────────────────────────────────────────────────────────────

  async function addToPath() {
    pathWorking = true;
    pathFeedback = null;
    try {
      await invoke('configure_path');
      status = await invoke<IntegrationStatus>('get_integrations_status');
      pathFeedback = { ok: true, message: '~/.nodespace/bin added to PATH in your shell profiles.' };
    } catch (err) {
      pathFeedback = { ok: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      pathWorking = false;
    }
  }

  async function removeFromPath() {
    pathWorking = true;
    pathFeedback = null;
    try {
      await invoke('remove_from_path');
      status = await invoke<IntegrationStatus>('get_integrations_status');
      pathFeedback = { ok: true, message: '~/.nodespace/bin removed from PATH in your shell profiles.' };
    } catch (err) {
      pathFeedback = { ok: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      pathWorking = false;
    }
  }

  // ── SKILL ─────────────────────────────────────────────────────────────────

  async function addSkill() {
    skillWorking = true;
    skillFeedback = null;
    try {
      const result = await invoke<SkillSetupResult>('install_skill', { force: true });
      skillResult = result;
      status = await invoke<IntegrationStatus>('get_integrations_status');
      if (result.success) {
        const installed = result.agentsInstalled.length > 0
          ? `Installed into: ${result.agentsInstalled.join(', ')}.`
          : 'Already up to date — no changes needed.';
        skillFeedback = { ok: true, message: installed };
      } else {
        skillFeedback = { ok: false, message: result.error ?? 'Skill installation failed.' };
      }
    } catch (err) {
      skillFeedback = { ok: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      skillWorking = false;
    }
  }

  async function removeSkill() {
    skillWorking = true;
    skillFeedback = null;
    try {
      await invoke('remove_skill');
      skillResult = await invoke<SkillSetupResult>('get_skill_setup_status');
      status = await invoke<IntegrationStatus>('get_integrations_status');
      skillFeedback = { ok: true, message: 'NodeSpace skill removed from Claude Code.' };
    } catch (err) {
      skillFeedback = { ok: false, message: err instanceof Error ? err.message : String(err) };
    } finally {
      skillWorking = false;
    }
  }

  const pathIsConfigured = $derived(status?.pathAlreadyConfigured ?? false);
  const skillIsInstalled = $derived(skillResult?.success ?? false);
  const claudeDetected = $derived(status?.claudeCodeDetected ?? false);
</script>

<div class="integrations-settings">
  <h2 class="section-title">Integrations</h2>
  <p class="section-description">
    Manage how NodeSpace integrates with your shell and AI agents.
  </p>

  <!-- CLI PATH -->
  <div class="card">
    <div class="card-header">
      <div class="card-title-row">
        <span class="card-title">CLI PATH</span>
        {#if status === null}
          <span class="badge badge-muted">Checking…</span>
        {:else if pathIsConfigured}
          <span class="badge badge-ok">Configured</span>
        {:else}
          <span class="badge badge-warn">Not configured</span>
        {/if}
      </div>
      <p class="card-description">
        Adds <code>~/.nodespace/bin</code> to your PATH so you can run
        <code>nodespace</code> from any terminal session.
      </p>
    </div>

    {#if pathFeedback !== null}
      <div class={pathFeedback.ok ? 'success-banner' : 'error-banner'}>
        {pathFeedback.message}
      </div>
    {/if}

    <div class="card-actions">
      {#if pathIsConfigured}
        <button class="secondary-button" onclick={removeFromPath} disabled={pathWorking}>
          {pathWorking ? 'Removing…' : 'Remove from PATH'}
        </button>
      {:else}
        <button class="primary-button" onclick={addToPath} disabled={pathWorking}>
          {pathWorking ? 'Adding…' : 'Add to PATH'}
        </button>
      {/if}
    </div>
  </div>

  <!-- Claude Code Skill -->
  <div class="card" class:card-disabled={!claudeDetected}>
    <div class="card-header">
      <div class="card-title-row">
        <span class="card-title">Claude Code Skill</span>
        {#if !claudeDetected}
          <span class="badge badge-muted">Claude Code not detected</span>
        {:else if status === null}
          <span class="badge badge-muted">Checking…</span>
        {:else if skillIsInstalled}
          <span class="badge badge-ok">Installed</span>
        {:else}
          <span class="badge badge-warn">Not installed</span>
        {/if}
      </div>
      <p class="card-description">
        {#if !claudeDetected}
          Claude Code is not installed. Install it to enable NodeSpace tools in Claude Code CLI sessions.
        {:else}
          NodeSpace tools available in Claude Code CLI sessions via
          <code>~/.claude/skills/nodespace/SKILL.md</code>.
        {/if}
      </p>
    </div>

    {#if claudeDetected && skillResult?.cliWarning}
      <div class="warning-banner">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="15" height="15" aria-hidden="true">
          <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
        <span>{skillResult.cliWarning}</span>
      </div>
    {/if}

    {#if skillFeedback !== null}
      <div class={skillFeedback.ok ? 'success-banner' : 'error-banner'}>
        {skillFeedback.message}
      </div>
    {/if}

    <div class="card-actions">
      {#if !claudeDetected}
        <button class="primary-button" disabled>Add Skill</button>
      {:else if skillIsInstalled}
        <button class="primary-button" onclick={addSkill} disabled={skillWorking}>
          {skillWorking ? 'Reinstalling…' : 'Reinstall Skill'}
        </button>
        <button class="secondary-button" onclick={removeSkill} disabled={skillWorking}>
          {skillWorking ? 'Removing…' : 'Remove Skill'}
        </button>
      {:else}
        <button class="primary-button" onclick={addSkill} disabled={skillWorking}>
          {skillWorking ? 'Installing…' : 'Add Skill'}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .integrations-settings {
    max-width: 640px;
  }

  .section-title {
    font-size: 1.25rem;
    font-weight: 600;
    margin: 0 0 0.375rem;
    color: hsl(var(--foreground));
  }

  .section-description {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    margin: 0 0 1.5rem;
    line-height: 1.5;
  }

  .card {
    border: 1px solid hsl(var(--border));
    border-radius: 0.5rem;
    padding: 1.25rem;
    background: hsl(var(--card));
    margin-bottom: 1rem;
  }

  .card-disabled {
    opacity: 0.6;
  }

  .card-header {
    margin-bottom: 1rem;
  }

  .card-title-row {
    display: flex;
    align-items: center;
    gap: 0.625rem;
    margin-bottom: 0.375rem;
  }

  .card-title {
    font-size: 0.9375rem;
    font-weight: 600;
    color: hsl(var(--foreground));
  }

  .card-description {
    font-size: 0.875rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
    line-height: 1.5;
  }

  .card-description code {
    font-size: 0.8125rem;
    background: hsl(var(--muted));
    padding: 0.1em 0.3em;
    border-radius: 3px;
    color: hsl(var(--foreground));
  }

  .badge {
    font-size: 0.75rem;
    font-weight: 500;
    padding: 0.125rem 0.5rem;
    border-radius: 9999px;
  }

  .badge-ok {
    background: hsl(142 76% 36% / 0.12);
    color: hsl(142 76% 30%);
    border: 1px solid hsl(142 76% 36% / 0.25);
  }

  .badge-warn {
    background: hsl(38 92% 50% / 0.12);
    color: hsl(38 70% 35%);
    border: 1px solid hsl(38 92% 50% / 0.25);
  }

  .badge-muted {
    background: hsl(var(--muted));
    color: hsl(var(--muted-foreground));
    border: 1px solid hsl(var(--border));
  }

  .warning-banner {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    font-size: 0.875rem;
    color: hsl(38 70% 35%);
    background: hsl(38 92% 50% / 0.08);
    border: 1px solid hsl(38 92% 50% / 0.2);
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1rem;
    line-height: 1.5;
  }

  .warning-banner svg {
    flex-shrink: 0;
    margin-top: 2px;
  }

  .success-banner {
    font-size: 0.875rem;
    color: hsl(142 76% 30%);
    background: hsl(142 76% 36% / 0.1);
    border: 1px solid hsl(142 76% 36% / 0.25);
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1rem;
    line-height: 1.5;
  }

  .error-banner {
    font-size: 0.875rem;
    color: hsl(var(--destructive-foreground));
    background: hsl(var(--destructive) / 0.1);
    border: 1px solid hsl(var(--destructive) / 0.3);
    border-radius: 0.375rem;
    padding: 0.625rem 0.875rem;
    margin-bottom: 1rem;
    line-height: 1.5;
  }

  .card-actions {
    display: flex;
    gap: 0.75rem;
  }

  .primary-button {
    padding: 0.4375rem 1rem;
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

  .secondary-button {
    padding: 0.4375rem 1rem;
    border-radius: 0.375rem;
    border: 1px solid hsl(var(--border));
    background: transparent;
    color: hsl(var(--foreground));
    font-size: 0.875rem;
    font-weight: 500;
    cursor: pointer;
    transition: background 0.15s;
  }

  .secondary-button:hover:not(:disabled) {
    background: hsl(var(--muted) / 0.5);
  }

  .secondary-button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
</style>
