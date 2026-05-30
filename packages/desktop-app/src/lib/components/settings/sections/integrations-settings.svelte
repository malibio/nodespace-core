<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('IntegrationsSettings');

  interface SkillSetupResult {
    success: boolean;
    agentsInstalled: string[];
    cliOnPath: boolean;
    cliWarning: string | null;
    error: string | null;
  }

  let status = $state<SkillSetupResult | null>(null);
  let isInstalling = $state(false);
  let lastResult = $state<SkillSetupResult | null>(null);

  onMount(async () => {
    try {
      status = await invoke<SkillSetupResult>('get_skill_setup_status');
    } catch (err) {
      log.warn('Could not load skill setup status', err);
    }
  });

  async function reinstallSkill() {
    isInstalling = true;
    lastResult = null;
    try {
      const result = await invoke<SkillSetupResult>('install_skill', { force: true });
      lastResult = result;
      status = result;
      log.info('Skill reinstalled', result);
    } catch (err) {
      log.error('Failed to reinstall skill', err);
      lastResult = {
        success: false,
        agentsInstalled: [],
        cliOnPath: false,
        cliWarning: null,
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      isInstalling = false;
    }
  }
</script>

<div class="integrations-settings">
  <h2 class="section-title">Integrations</h2>
  <p class="section-description">
    NodeSpace installs a skill file into detected AI agents so they can interact with your
    knowledge graph without manual setup.
  </p>

  <div class="card">
    <div class="card-header">
      <div class="card-title-row">
        <span class="card-title">NodeSpace Skill</span>
        {#if status?.success}
          <span class="badge badge-ok">Installed</span>
        {:else if status !== null}
          <span class="badge badge-warn">Not installed</span>
        {/if}
      </div>
      <p class="card-description">
        Copies <code>SKILL.md</code> and agent shims into each detected agent's skills directory
        (Claude Code, Codex, Gemini, OpenCode).
      </p>
    </div>

    {#if status?.cliWarning}
      <div class="warning-banner">
        <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" width="15" height="15" aria-hidden="true">
          <path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z" />
          <line x1="12" y1="9" x2="12" y2="13" />
          <line x1="12" y1="17" x2="12.01" y2="17" />
        </svg>
        <span>{status.cliWarning}</span>
      </div>
    {/if}

    {#if lastResult !== null}
      {#if lastResult.success}
        <div class="success-banner">
          {#if lastResult.agentsInstalled.length > 0}
            Installed into: {lastResult.agentsInstalled.join(', ')}.
          {:else}
            Already up to date — no changes needed.
          {/if}
        </div>
      {:else if lastResult.error}
        <div class="error-banner">{lastResult.error}</div>
      {/if}
    {/if}

    <div class="card-actions">
      <button class="primary-button" onclick={reinstallSkill} disabled={isInstalling}>
        {#if isInstalling}
          Installing…
        {:else if status?.success}
          Reinstall Skill
        {:else}
          Install Skill
        {/if}
      </button>
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
</style>
