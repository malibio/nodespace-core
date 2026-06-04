<!--
  AiChatNodeViewer - Page-level viewer for AI chat conversation nodes

  Per ADR-034, `ai-chat` is one node type with four provider modes. This is THE
  single dispatcher for the type. It renders a header (title + provider dropdown)
  and routes on `properties.provider` (+ `properties.model`):
    - unset                         → just the dropdown prompt (a fresh `/ai-chat`
      node hasn't chosen a mode yet).
    - pty                           → embedded terminal session (AiChatPtySession):
      the terminal IS the node's viewer; capture backfills the node.
    - native | ollama, no model     → model picker (AiChatModelPicker).
    - native | ollama, with model   → message UI (chat input + streamed messages[]).
    - openai                        → disabled in the dropdown (no backend yet).

  Node-as-message-queue architecture: the node is the single source of truth.
  - Frontend only writes `updateNode` to append user messages.
  - LocalAgentService in the daemon reacts to node changes and drives inference.
  - Streaming tokens arrive via Tauri events (local-agent://chunk) and accumulate
    in a local `streamingContent` buffer. The buffer is cleared when WatchNodes
    delivers the completed assistant message.
  - Typing indicator driven by `node.properties['ai-chat']['status'] === 'processing'`.
-->

<script lang="ts">
  /* global HTMLSelectElement */
  import { onMount, onDestroy, tick } from 'svelte';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import ChatMessage from '$lib/components/chat/chat-message.svelte';
  import ChatInput from '$lib/components/chat/chat-input.svelte';
  import AiChatPtySession from './ai-chat-pty-session.svelte';
  import AiChatModelPicker from './ai-chat-model-picker.svelte';
  import type { DisplayMessage } from '$lib/components/chat/types';
  import type { StreamingChunk } from '$lib/types/agent-types';
  import { AGENT_EVENTS } from '$lib/types/agent-types';
  import {
    localAgentCancelTurn,
    ensureModelReady,
    ollamaAvailable,
  } from '$lib/services/tauri-commands';
  import { backendAdapter } from '$lib/services/backend-adapter';
  import { statusBar } from '$lib/stores/status-bar';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AiChatNodeViewer');

  /** Provider modes that render the message UI (ADR-034 modes 2a/2b/2c). */
  const MESSAGE_PROVIDERS = ['native', 'ollama', 'openai'] as const;
  type Provider = (typeof MESSAGE_PROVIDERS)[number] | 'pty';

  const PROVIDER_OPTIONS: {
    id: Provider;
    label: string;
    unavailable?: boolean;
  }[] = [
    { id: 'native', label: 'Built-in model' },
    { id: 'ollama', label: 'Ollama' },
    { id: 'openai', label: 'OpenAI endpoint', unavailable: true },
    { id: 'pty', label: 'Agent (terminal)' },
  ];

  let {
    nodeId,
    onTitleChange,
  }: {
    nodeId: string;
    onTitleChange?: (_title: string) => void;
  } = $props();

  // --- State ---
  let messagesContainer: HTMLDivElement | undefined = $state();
  /** In-flight token buffer. Cleared when WatchNodes delivers the completed message. */
  let streamingContent = $state('');
  let sendError = $state<string | null>(null);
  let nodeReady = $state(false);
  let ollamaReady = $state(false);
  let eventUnlisteners: Array<() => void> = [];

  const SOFT_MESSAGE_CAP = 500;

  // --- Reactive node lookup ---
  const node = $derived(sharedNodeStore.getNode(nodeId));

  function getProp(props: Record<string, unknown> | undefined, key: string): unknown {
    if (!props) return undefined;
    const ns = props['ai-chat'] as Record<string, unknown> | undefined;
    return ns?.[key] ?? props[key];
  }

  const provider = $derived(getProp(node?.properties, 'provider') as Provider | undefined);
  const model = $derived((getProp(node?.properties, 'model') as string) ?? '');
  const isMessageProvider = $derived(
    provider !== undefined && (MESSAGE_PROVIDERS as readonly string[]).includes(provider)
  );
  const lifecycleStatus = $derived((getProp(node?.properties, 'status') as string) ?? 'active');

  /** True while the daemon is processing an inference turn for this node. */
  const isProcessing = $derived(
    (node?.properties?.['ai-chat'] as Record<string, unknown> | undefined)?.['status'] === 'processing'
  );

  /** Messages from the persisted node, mapped to DisplayMessage for rendering. */
  const persistedMessages: DisplayMessage[] = $derived.by(() => {
    const msgs = (node?.properties?.['ai-chat'] as Record<string, unknown> | undefined)?.['messages'];
    if (!Array.isArray(msgs)) return [];
    return msgs
      .filter((m: Record<string, unknown>) => {
        const role = m['role'] as string;
        return role === 'user' || role === 'assistant';
      })
      .map((m: Record<string, unknown>, idx: number) => ({
        id: `persisted-${idx}-${m['timestamp'] ?? idx}`,
        role: (m['role'] as DisplayMessage['role']) ?? 'user',
        content: (m['content'] as string) ?? '',
        toolExecutions: [],
        timestamp: m['timestamp']
          ? new Date(m['timestamp'] as string).getTime()
          : Date.now(),
      }));
  });

  /** All messages to display: persisted + optional streaming overlay. */
  const displayMessages: DisplayMessage[] = $derived.by(() => {
    if (!streamingContent) return persistedMessages;
    // Append a live assistant message for the in-flight tokens.
    return [
      ...persistedMessages,
      {
        id: 'streaming',
        role: 'assistant' as const,
        content: streamingContent,
        toolExecutions: [],
        timestamp: Date.now(),
      },
    ];
  });

  const showMessageCap = $derived(persistedMessages.length >= SOFT_MESSAGE_CAP);

  /** Persist a provider mode change onto the node. */
  function selectProvider(p: Provider): void {
    if (p === provider) return;
    const current = sharedNodeStore.getNode(nodeId);
    const existingProps = current?.properties ?? {};
    const existingNs = (existingProps['ai-chat'] as Record<string, unknown>) ?? {};
    const nsWithoutModel = Object.fromEntries(
      Object.entries(existingNs).filter(([k]) => k !== 'model')
    );
    sharedNodeStore.updateNode(
      nodeId,
      { properties: { ...existingProps, 'ai-chat': { ...nsWithoutModel, provider: p } } },
      { type: 'viewer', viewerId: 'ai-chat-viewer' }
    );
  }

  function onProviderChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as Provider;
    selectProvider(value);
  }

  function isTauri(): boolean {
    return (
      typeof window !== 'undefined' &&
      ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
    );
  }

  function cleanupListeners(): void {
    for (const unlisten of eventUnlisteners) unlisten();
    eventUnlisteners = [];
  }

  /**
   * Send a user message: append it to the node's ai-chat messages via updateNode.
   * The daemon reacts to the NodeUpdated event and drives inference.
   * Model must be loaded first via ensureModelReady.
   */
  async function handleSend(content: string): Promise<void> {
    const trimmed = content.trim();
    if (!trimmed || isProcessing || !model) return;

    sendError = null;

    // Ensure the model is loaded before the first message.
    try {
      await ensureModelReady(model);
    } catch (err) {
      const msg =
        err instanceof Error ? err.message : ((err as Record<string, unknown>)?.message as string) ?? String(err);
      sendError = msg;
      statusBar.error(`Model error: ${msg}`);
      return;
    }

    // Append user message to node — daemon will react and start inference.
    const current = sharedNodeStore.getNode(nodeId);
    if (!current) {
      sendError = 'Node not found';
      return;
    }

    const existingNs = (current.properties?.['ai-chat'] as Record<string, unknown>) ?? {};
    const existingMessages = Array.isArray(existingNs['messages']) ? existingNs['messages'] : [];
    const newMessage = {
      role: 'user',
      content: trimmed,
      timestamp: new Date().toISOString(),
    };

    sharedNodeStore.updateNode(
      nodeId,
      {
        properties: {
          ...current.properties,
          'ai-chat': {
            ...existingNs,
            messages: [...existingMessages, newMessage],
          },
        },
      },
      { type: 'viewer', viewerId: 'ai-chat-viewer' }
    );

    await scrollToBottom();
  }

  async function handleCancel(): Promise<void> {
    if (!isProcessing) return;
    try {
      await localAgentCancelTurn(nodeId);
    } catch (err) {
      log.warn('Failed to cancel turn', { error: String(err) });
    }
  }

  async function scrollToBottom(): Promise<void> {
    await tick();
    if (messagesContainer) {
      messagesContainer.scrollTop = messagesContainer.scrollHeight;
    }
  }

  // --- Lifecycle ---

  onMount(async () => {
    log.debug('AiChatNodeViewer mounted', { nodeId });

    let hydrationSucceeded = false;
    try {
      if (!sharedNodeStore.getNode(nodeId)) {
        try {
          const fetched = await backendAdapter.getNode(nodeId);
          if (fetched) {
            sharedNodeStore.setNode(fetched, {
              type: 'viewer',
              viewerId: 'ai-chat-viewer-hydration',
            });
          } else {
            log.warn('ai-chat node not found in backend', { nodeId });
          }
        } catch (err) {
          log.warn('Failed to hydrate ai-chat node by id', { nodeId, error: String(err) });
        }
      }

      // Only mark ready if the node is actually in the store — interactive
      // elements (provider select, model picker) call updateNode which silently
      // drops writes for non-existent nodes.
      if (!sharedNodeStore.getNode(nodeId)) {
        log.error('Node still not in store after hydration, staying in loading state', { nodeId });
        return;
      }

      hydrationSucceeded = true;

      if (node?.content) onTitleChange?.(node.content);

      if (isTauri()) {
        try {
          ollamaReady = await ollamaAvailable();
        } catch (err) {
          log.debug('Ollama availability check failed', { error: String(err) });
        }

        // Subscribe to streaming token events for this node.
        const { listen } = await import('@tauri-apps/api/event');

        const unlistenChunk = await listen<StreamingChunk & { node_id?: string }>(
          AGENT_EVENTS.LOCAL_AGENT_CHUNK,
          (event) => {
            const chunk = event.payload;
            // Filter to only this node's events.
            if (chunk.node_id && chunk.node_id !== nodeId) return;

            if (chunk.type === 'token') {
              streamingContent += chunk.text ?? '';
              scrollToBottom();
            } else if (chunk.type === 'done') {
              // WatchNodes will deliver the completed assistant message.
              // Clear the streaming buffer — but wait one tick to avoid flash.
              tick().then(() => { streamingContent = ''; });
            } else if (chunk.type === 'error') {
              sendError = (chunk as unknown as { error_message?: string }).error_message ?? 'Inference error';
              streamingContent = '';
            }
          }
        );
        eventUnlisteners.push(unlistenChunk);

        const unlistenError = await listen<string>(AGENT_EVENTS.LOCAL_AGENT_ERROR, (event) => {
          log.error('Agent error', { error: event.payload });
          sendError = event.payload;
          streamingContent = '';
        });
        eventUnlisteners.push(unlistenError);
      }
    } finally {
      if (hydrationSucceeded) nodeReady = true;
    }
  });

  onDestroy(() => {
    cleanupListeners();
  });

  // Auto-scroll as messages stream/append.
  $effect(() => {
    void displayMessages.length;
    scrollToBottom();
  });

  // Clear streaming buffer when WatchNodes delivers a completed assistant message
  // (the node now has the message persisted, so the overlay is no longer needed).
  $effect(() => {
    if (!isProcessing && streamingContent) {
      // Check if the last persisted message is from the assistant — means the
      // daemon wrote the completed message, so we can clear the buffer.
      const msgs = persistedMessages;
      if (msgs.length > 0 && msgs[msgs.length - 1].role === 'assistant') {
        streamingContent = '';
      }
    }
  });

  // Update title when node content changes.
  $effect(() => {
    if (node?.content) onTitleChange?.(node.content);
  });

  // If onMount couldn't mark nodeReady (node wasn't in store yet), watch for the
  // node to arrive via WatchNodes and complete setup reactively — no tab reopen needed.
  $effect(() => {
    if (!nodeReady && node) {
      nodeReady = true;
    }
  });
</script>

<div class="ai-chat-viewer">
  <!-- Header (shown in every mode): title + provider dropdown. -->
  <div class="chat-viewer-header">
    <div class="chat-viewer-header-left">
      <h2 class="chat-viewer-title">{node?.content ?? 'AI Chat'}</h2>
      <div class="chat-viewer-meta">
        {#if model}
          <span class="meta-badge meta-model">{model}</span>
        {/if}
        <span class="meta-badge" class:meta-archived={lifecycleStatus === 'archived'}>
          {lifecycleStatus}
        </span>
      </div>
    </div>
    <div class="chat-viewer-header-right">
      <select
        class="provider-select"
        aria-label="Conversation provider"
        value={provider ?? ''}
        onchange={onProviderChange}
      >
        {#if provider === undefined}
          <option value="" disabled selected>Choose a provider…</option>
        {/if}
        {#each PROVIDER_OPTIONS as opt (opt.id)}
          {@const ollamaUnavailable = opt.id === 'ollama' && !ollamaReady}
          <option value={opt.id} disabled={opt.unavailable || ollamaUnavailable}>
            {opt.label}{opt.unavailable
              ? ' (coming soon)'
              : ollamaUnavailable
                ? ' (not running)'
                : ''}
          </option>
        {/each}
      </select>
    </div>
  </div>

  {#if !nodeReady}
    <div class="provider-prompt">
      <p class="provider-prompt-text">Loading…</p>
    </div>
  {:else if provider === undefined}
    <div class="provider-prompt">
      <p class="provider-prompt-text">Choose how this conversation is powered</p>
      <p class="provider-prompt-hint">
        Pick a provider from the dropdown in the top-right to get started.
      </p>
    </div>
  {:else if provider === 'pty'}
    <AiChatPtySession {nodeId} />
  {:else if isMessageProvider && !model}
    <AiChatModelPicker
      {nodeId}
      provider={provider as 'native' | 'ollama'}
      onSelect={(modelId) => {
        // Always read the latest in-memory state — `selectProvider` may have
        // written to the store moments before this fires and `node` ($derived)
        // may not have re-evaluated yet in this non-reactive callback context.
        const current = sharedNodeStore.getNode(nodeId);
        if (!current) return;
        const existingNs = (current.properties?.['ai-chat'] as Record<string, unknown>) ?? {};
        sharedNodeStore.updateNode(
          nodeId,
          { properties: { ...current.properties, 'ai-chat': { ...existingNs, provider: provider ?? 'native', model: modelId, status: 'active' } } },
          { type: 'viewer', viewerId: 'ai-chat-viewer' }
        );
      }}
    />
  {:else if isMessageProvider}
    <div
      class="chat-viewer-messages"
      bind:this={messagesContainer}
      role="list"
      aria-label="Chat conversation"
    >
      {#if displayMessages.length === 0}
        <div class="empty-conversation">
          <div class="empty-conversation-icon">
            <svg
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              stroke-width="1.5"
              width="48"
              height="48"
            >
              <path d="M21 15a2 2 0 0 1-2 2H7l-4 4V5a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2z" />
            </svg>
          </div>
          <p class="empty-conversation-text">Start a conversation</p>
          <p class="empty-conversation-hint">Type a message below to begin</p>
        </div>
      {:else}
        {#each displayMessages as message (message.id)}
          <ChatMessage {message} />
        {/each}
      {/if}

      {#if isProcessing}
        <div class="typing-indicator" aria-label="AI is thinking">
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
          <button class="cancel-turn-btn" onclick={handleCancel} aria-label="Cancel response">
            Stop
          </button>
        </div>
      {/if}

      {#if showMessageCap}
        <div class="message-cap-nudge" role="alert">
          <p>
            This conversation has {persistedMessages.length} messages. Consider starting a new chat for
            better performance.
          </p>
        </div>
      {/if}
    </div>

    {#if sendError}
      <div class="send-error" role="alert">{sendError}</div>
    {/if}

    {#if lifecycleStatus !== 'archived'}
      <ChatInput
        onSend={handleSend}
        disabled={isProcessing}
        placeholder={isProcessing ? 'AI is responding...' : 'Type a message...'}
      />
    {:else}
      <div class="archived-notice">This conversation is archived and read-only.</div>
    {/if}
  {/if}
</div>

<style>
  .ai-chat-viewer {
    display: flex;
    flex-direction: column;
    height: 100%;
    background: hsl(var(--background));
  }

  .chat-viewer-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.75rem 1rem;
    border-bottom: 1px solid hsl(var(--border));
    background: hsl(var(--background));
    flex-shrink: 0;
    gap: 0.75rem;
  }

  .chat-viewer-header-left {
    display: flex;
    flex-direction: column;
    gap: 0.25rem;
    min-width: 0;
  }

  .chat-viewer-header-right {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-shrink: 0;
  }

  .chat-viewer-title {
    font-size: 1rem;
    font-weight: 600;
    margin: 0;
    color: hsl(var(--foreground));
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .chat-viewer-meta {
    display: flex;
    gap: 0.375rem;
    flex-wrap: wrap;
  }

  .meta-badge {
    font-size: 0.6875rem;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted));
    padding: 0.0625rem 0.375rem;
    border-radius: 9999px;
    text-transform: lowercase;
  }

  .meta-model {
    font-family: monospace;
    font-size: 0.625rem;
  }

  .meta-archived {
    color: hsl(var(--destructive));
    background: hsl(var(--destructive) / 0.1);
  }

  .provider-select {
    font-size: 0.8125rem;
    padding: 0.3125rem 0.5rem;
    border: 1px solid hsl(var(--border));
    border-radius: 0.375rem;
    background: hsl(var(--background));
    color: hsl(var(--foreground));
    cursor: pointer;
    max-width: 12rem;
  }

  .provider-select:hover {
    border-color: hsl(var(--ring));
  }

  .provider-prompt {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 0.5rem;
    padding: 2rem 1rem;
    text-align: center;
  }

  .provider-prompt-text {
    margin: 0;
    font-size: 0.9375rem;
    font-weight: 500;
    color: hsl(var(--foreground));
  }

  .provider-prompt-hint {
    margin: 0;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }

  .chat-viewer-messages {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem 0;
  }

  .empty-conversation {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    height: 100%;
    padding: 2rem;
    text-align: center;
  }

  .empty-conversation-icon {
    color: hsl(var(--muted-foreground) / 0.5);
    margin-bottom: 1rem;
  }

  .empty-conversation-text {
    font-size: 1rem;
    font-weight: 500;
    color: hsl(var(--foreground));
    margin: 0 0 0.5rem;
  }

  .empty-conversation-hint {
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    margin: 0;
  }

  .typing-indicator {
    display: flex;
    gap: 0.25rem;
    padding: 0.75rem 1.5rem;
    align-items: center;
  }

  .typing-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: hsl(var(--muted-foreground));
    animation: typing-bounce 1.2s ease-in-out infinite;
  }

  .typing-dot:nth-child(2) {
    animation-delay: 0.15s;
  }

  .typing-dot:nth-child(3) {
    animation-delay: 0.3s;
  }

  .cancel-turn-btn {
    margin-left: 0.5rem;
    padding: 0.125rem 0.5rem;
    font-size: 0.75rem;
    background: transparent;
    border: 1px solid hsl(var(--muted-foreground) / 0.4);
    border-radius: 0.25rem;
    color: hsl(var(--muted-foreground));
    cursor: pointer;
  }

  .cancel-turn-btn:hover {
    border-color: hsl(var(--destructive));
    color: hsl(var(--destructive));
  }

  @keyframes typing-bounce {
    0%,
    60%,
    100% {
      transform: translateY(0);
      opacity: 0.4;
    }
    30% {
      transform: translateY(-4px);
      opacity: 1;
    }
  }

  .message-cap-nudge {
    margin: 0.5rem 1rem;
    padding: 0.75rem 1rem;
    background: hsl(var(--accent) / 0.1);
    border: 1px solid hsl(var(--accent) / 0.3);
    border-radius: 0.5rem;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
  }

  .message-cap-nudge p {
    margin: 0;
  }

  .send-error {
    margin: 0.5rem 1rem;
    padding: 0.5rem 0.75rem;
    border-radius: 0.375rem;
    background: hsl(0 72% 51% / 0.1);
    border: 1px solid hsl(0 72% 51% / 0.3);
    color: hsl(0 72% 51%);
    font-size: 0.8125rem;
  }

  .archived-notice {
    padding: 0.75rem 1rem;
    text-align: center;
    font-size: 0.8125rem;
    color: hsl(var(--muted-foreground));
    background: hsl(var(--muted) / 0.5);
    border-top: 1px solid hsl(var(--border));
  }
</style>
