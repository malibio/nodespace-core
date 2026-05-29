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

  Per-node, no shared store: this viewer owns its OWN daemon session
  (localAgentNewSession/localAgentSend/ensureModelReady + streaming events), keyed
  off the node's provider/model. One inference engine in the daemon serves every
  ai-chat node via independent sessions. Messages persist to node.properties.messages
  with debounced flush + tool-result archival per ADR-028.

  Follows the *NodeViewer pattern but does NOT wrap BaseNodeViewer because it
  renders a chat conversation rather than a hierarchical node collection.
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
  import type {
    ToolExecutionRecord,
    StreamingChunk,
    LocalAgentStatus,
    AgentTurnResult,
  } from '$lib/types/agent-types';
  import { AGENT_EVENTS } from '$lib/types/agent-types';
  import {
    localAgentNewSession,
    localAgentSend,
    localAgentCancel,
    localAgentEndSession,
    ensureModelReady,
    ollamaAvailable,
    getNode as fetchNode,
  } from '$lib/services/tauri-commands';
  import { statusBar } from '$lib/stores/status-bar';
  import { createLogger } from '$lib/utils/logger';

  const log = createLogger('AiChatNodeViewer');

  /** Provider modes that render the message UI (ADR-034 modes 2a/2b/2c). */
  const MESSAGE_PROVIDERS = ['native', 'ollama', 'openai'] as const;
  type Provider = (typeof MESSAGE_PROVIDERS)[number] | 'pty';

  const PROVIDER_OPTIONS: {
    id: Provider;
    label: string;
    /** Modes with no backend yet are always disabled in the dropdown. */
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
  let inMemoryMessages = $state<DisplayMessage[]>([]);
  let isStreaming = $state(false);
  let flushTimer: ReturnType<typeof setTimeout> | null = null;
  let hasUnsavedChanges = $state(false);
  let sendError = $state<string | null>(null);
  /** One-shot: persisted messages loaded once the node is first available. */
  let initialLoadDone = $state(false);

  /** Discovered once on mount: is a local Ollama reachable? Gates the dropdown option. */
  let ollamaReady = $state(false);

  /** This viewer's own daemon session id (Option 1: per-node, no shared store). */
  let sessionId: string | null = null;
  /** Set once the engine has been (re)installed for the current model. */
  let modelPrepared = false;
  let eventUnlisteners: Array<() => void> = [];

  /** Soft message cap -- show a nudge when conversation gets long */
  const SOFT_MESSAGE_CAP = 500;
  const FLUSH_DEBOUNCE_MS = 7_000; // 7 seconds (within 5-10s range per spec)

  // --- Reactive node lookup ---
  const node = $derived(sharedNodeStore.getNode(nodeId));
  // Undefined = placeholder (a fresh `/ai-chat` node before a mode is chosen).
  const provider = $derived(node?.properties?.provider as Provider | undefined);
  const model = $derived((node?.properties?.model as string) ?? '');
  const isMessageProvider = $derived(
    provider !== undefined && (MESSAGE_PROVIDERS as readonly string[]).includes(provider)
  );
  const status = $derived((node?.properties?.status as string) ?? 'active');
  const showMessageCap = $derived(inMemoryMessages.length >= SOFT_MESSAGE_CAP);

  /** Persist a provider mode change onto the node (placeholder → configured, or switch). */
  function selectProvider(p: Provider): void {
    if (p === provider) return;
    const current = sharedNodeStore.getNode(nodeId);
    // Switching mode drops any model chosen for the previous mode.
    const nextProps = { ...current?.properties, provider: p };
    delete (nextProps as Record<string, unknown>).model;
    sharedNodeStore.updateNode(
      nodeId,
      { properties: nextProps },
      { type: 'viewer', viewerId: 'ai-chat-viewer' },
      // Selecting a provider is an intentional configuration action, like a
      // nodeType conversion. It can fire right after the `/ai-chat` conversion,
      // so without this the update collides with that still-pending change in
      // the conflict window and gets silently dropped — leaving the picker inert.
      { skipConflictDetection: true }
    );
    // A model from a previous mode is no longer valid; force a fresh session.
    teardownSession();
  }

  function onProviderChange(e: Event): void {
    const value = (e.currentTarget as HTMLSelectElement).value as Provider;
    selectProvider(value);
  }

  // --- Load messages from persisted node properties ---
  function loadMessagesFromNode(): void {
    if (!node) return;
    const persisted = node.properties?.messages;
    if (!Array.isArray(persisted)) {
      inMemoryMessages = [];
      return;
    }
    inMemoryMessages = persisted.map((m: Record<string, unknown>, idx: number) => {
      const role = (m.role as string) ?? 'user';
      // For tool_call messages, map to assistant role with tool executions
      if (role === 'tool_call') {
        const toolExec: ToolExecutionRecord = {
          tool_call_id: `tc-${idx}`,
          name: (m.tool as string) ?? 'unknown',
          args: m.args ?? {},
          result: m.result_summary ?? null,
          is_error: m.status === 'error',
          duration_ms: (m.duration_ms as number) ?? 0,
        };
        return {
          id: `persisted-${idx}`,
          role: 'assistant' as const,
          content: '',
          toolExecutions: [toolExec],
          timestamp: m.timestamp ? new Date(m.timestamp as string).getTime() : Date.now(),
        };
      }
      return {
        id: `persisted-${idx}`,
        role: role as DisplayMessage['role'],
        content: (m.content as string) ?? '',
        toolExecutions: [],
        timestamp: m.timestamp ? new Date(m.timestamp as string).getTime() : Date.now(),
      };
    });
  }

  // --- Write buffering: archive tool results and debounced flush ---

  /** Convert DisplayMessage[] back to the persisted messages format,
   *  nulling full tool results (only result_summary kept). */
  function archiveMessages(msgs: DisplayMessage[]): Record<string, unknown>[] {
    const archived: Record<string, unknown>[] = [];
    for (const msg of msgs) {
      if (msg.toolExecutions.length > 0) {
        for (const te of msg.toolExecutions) {
          archived.push({
            role: 'tool_call',
            tool: te.name,
            args: te.args,
            status: te.is_error ? 'error' : 'completed',
            result_summary:
              typeof te.result === 'string'
                ? te.result
                : te.result != null
                  ? JSON.stringify(te.result).slice(0, 200)
                  : null,
            result: null, // Nulled at write time per ADR-028
            duration_ms: te.duration_ms,
            timestamp: new Date(msg.timestamp).toISOString(),
          });
        }
        // If the assistant message also had text content, emit it separately
        if (msg.content) {
          archived.push({
            role: 'assistant',
            content: msg.content,
            timestamp: new Date(msg.timestamp).toISOString(),
          });
        }
      } else {
        archived.push({
          role: msg.role,
          content: msg.content,
          timestamp: new Date(msg.timestamp).toISOString(),
        });
      }
    }
    return archived;
  }

  function scheduleFlush(): void {
    if (flushTimer) clearTimeout(flushTimer);
    hasUnsavedChanges = true;
    flushTimer = setTimeout(() => flushToStore(), FLUSH_DEBOUNCE_MS);
  }

  function flushToStore(): void {
    if (!node || !hasUnsavedChanges) return;
    if (flushTimer) {
      clearTimeout(flushTimer);
      flushTimer = null;
    }
    try {
      const archivedMessages = archiveMessages(inMemoryMessages);
      sharedNodeStore.updateNode(
        nodeId,
        {
          properties: {
            ...node.properties,
            messages: archivedMessages,
            last_active: new Date().toISOString(),
            context_tokens: estimateTokens(inMemoryMessages),
          },
        },
        { type: 'viewer', viewerId: 'ai-chat-viewer' }
      );
      hasUnsavedChanges = false;
      log.debug('Flushed messages to store', { messageCount: archivedMessages.length });
    } catch (err) {
      log.error('Failed to flush messages', err);
    }
  }

  /** Rough token estimate: ~4 chars per token */
  function estimateTokens(msgs: DisplayMessage[]): number {
    let chars = 0;
    for (const m of msgs) {
      chars += m.content.length;
    }
    return Math.ceil(chars / 4);
  }

  // --- Per-node send/stream (Option 1: own daemon session, no shared store) ---

  function isTauri(): boolean {
    return (
      typeof window !== 'undefined' &&
      ('__TAURI__' in window || '__TAURI_INTERNALS__' in window)
    );
  }

  function generateId(): string {
    return `msg-${Date.now()}-${Math.random().toString(36).slice(2, 9)}`;
  }

  function cleanupListeners(): void {
    for (const unlisten of eventUnlisteners) unlisten();
    eventUnlisteners = [];
  }

  /** Drop this viewer's daemon session (on mode/model switch or destroy). */
  function teardownSession(): void {
    // Cancel any in-flight generation before tearing the session down.
    if (isStreaming && sessionId && isTauri()) {
      localAgentCancel(sessionId).catch((err) => {
        log.warn('Failed to cancel agent generation', { error: String(err) });
      });
    }
    cleanupListeners();
    if (sessionId && isTauri()) {
      localAgentEndSession(sessionId).catch((err) => {
        log.warn('Failed to end local session', { error: String(err) });
      });
    }
    sessionId = null;
    modelPrepared = false;
  }

  /**
   * Ensure the node's model is downloaded + loaded and we hold a live session for
   * it. Streams download/load progress into a transient assistant message that is
   * removed once the model is ready. Returns false (with sendError set) on failure.
   */
  async function ensureSession(modelId: string): Promise<boolean> {
    const { listen } = await import('@tauri-apps/api/event');

    // Transient progress placeholder while the model prepares.
    const progressMessage: DisplayMessage = {
      id: generateId(),
      role: 'assistant',
      content: 'Preparing model...',
      toolExecutions: [],
      timestamp: Date.now(),
    };
    inMemoryMessages = [...inMemoryMessages, progressMessage];

    interface DownloadProgress {
      bytes_downloaded: number;
      bytes_total: number;
      speed_bps: number;
    }
    const unlistenDownload = await listen<DownloadProgress>(
      AGENT_EVENTS.MODEL_DOWNLOAD_PROGRESS,
      (event) => {
        const { bytes_downloaded, bytes_total, speed_bps } = event.payload;
        const pct = Math.round((bytes_downloaded / bytes_total) * 100);
        const mbDown = (bytes_downloaded / 1_000_000).toFixed(0);
        const mbTotal = (bytes_total / 1_000_000).toFixed(0);
        let eta = '';
        if (speed_bps > 0) {
          const remainingSec = Math.ceil((bytes_total - bytes_downloaded) / speed_bps);
          eta =
            remainingSec < 60
              ? ` — ~${remainingSec}s remaining`
              : ` — ~${Math.ceil(remainingSec / 60)} min remaining`;
        }
        progressMessage.content = `Downloading model for first use... ${mbDown}/${mbTotal} MB (${pct}%)${eta}`;
        inMemoryMessages = [...inMemoryMessages.slice(0, -1), { ...progressMessage }];
        statusBar.show(`Downloading model... ${mbDown}/${mbTotal} MB${eta}`, pct);
      }
    );

    interface ModelStatusEvent {
      status: string;
    }
    const unlistenStatus = await listen<ModelStatusEvent>(AGENT_EVENTS.MODEL_STATUS, (event) => {
      const { status: s } = event.payload;
      if (s === 'loading') {
        progressMessage.content = 'Loading model... this may take a moment on first use.';
        inMemoryMessages = [...inMemoryMessages.slice(0, -1), { ...progressMessage }];
        statusBar.show('Loading model...');
      } else if (s === 'ready') {
        statusBar.success('Model ready');
      }
    });

    try {
      statusBar.show(`Preparing ${modelId}...`);
      const engineSwapped = await ensureModelReady(modelId);

      // Create a session if the engine was (re-)installed (which drops all
      // existing sessions) or we don't hold one yet.
      if (engineSwapped || !sessionId || !modelPrepared) {
        if (sessionId && engineSwapped) {
          // Old session is dead after an engine swap.
          localAgentEndSession(sessionId).catch(() => {});
        }
        sessionId = await localAgentNewSession(modelId);
        modelPrepared = true;
        log.info('Session ready', { sessionId, modelId, engineSwapped });
      }
      return true;
    } catch (err) {
      const msg =
        typeof err === 'string'
          ? err
          : err instanceof Error
            ? err.message
            : ((err as Record<string, unknown>)?.message as string) ?? JSON.stringify(err);
      log.error('Model preparation failed', { modelId, error: msg, raw: err });
      sendError = msg;
      statusBar.error(`Model error: ${msg}`);
      return false;
    } finally {
      unlistenDownload();
      unlistenStatus();
      // Remove the transient progress placeholder.
      inMemoryMessages = inMemoryMessages.filter((m) => m.id !== progressMessage.id);
    }
  }

  async function handleSend(content: string): Promise<void> {
    const trimmed = content.trim();
    if (!trimmed || isStreaming || !model) return;

    sendError = null;

    const userMsg: DisplayMessage = {
      id: generateId(),
      role: 'user',
      content: trimmed,
      toolExecutions: [],
      timestamp: Date.now(),
    };
    inMemoryMessages = [...inMemoryMessages, userMsg];
    scheduleFlush();
    await scrollToBottom();

    if (!isTauri()) {
      // Browser dev: the daemon session API isn't bridged. Surface, don't crash.
      sendError = 'Sending requires the desktop app (the daemon session API is unavailable in the browser).';
      return;
    }

    isStreaming = true;
    try {
      if (!(await ensureSession(model))) return;
      if (!sessionId) return;

      // Assistant placeholder, filled by streaming chunks then the final turn.
      const assistantMessage: DisplayMessage = {
        id: generateId(),
        role: 'assistant',
        content: '',
        toolExecutions: [],
        timestamp: Date.now(),
      };
      inMemoryMessages = [...inMemoryMessages, assistantMessage];

      const { listen } = await import('@tauri-apps/api/event');

      const unlistenChunk = await listen<StreamingChunk>(
        AGENT_EVENTS.LOCAL_AGENT_CHUNK,
        (event) => {
          const chunk = event.payload;
          if (chunk.type === 'token') {
            assistantMessage.content += chunk.text;
            inMemoryMessages = [...inMemoryMessages.slice(0, -1), { ...assistantMessage }];
          }
        }
      );
      eventUnlisteners.push(unlistenChunk);

      const unlistenStatus = await listen<LocalAgentStatus>(
        AGENT_EVENTS.LOCAL_AGENT_STATUS,
        (event) => log.debug('Agent status update', { status: event.payload })
      );
      eventUnlisteners.push(unlistenStatus);

      const unlistenError = await listen<string>(AGENT_EVENTS.LOCAL_AGENT_ERROR, (event) => {
        log.error('Agent error', { error: event.payload });
        sendError = event.payload;
      });
      eventUnlisteners.push(unlistenError);

      const result: AgentTurnResult = await localAgentSend(sessionId, trimmed);

      const finalMessage: DisplayMessage = {
        ...assistantMessage,
        content: result.response || assistantMessage.content,
        toolExecutions: result.tool_calls_made,
      };
      inMemoryMessages = [...inMemoryMessages.slice(0, -1), finalMessage];
      scheduleFlush();

      log.debug('Agent turn complete', {
        messageId: finalMessage.id,
        toolCalls: result.tool_calls_made.length,
      });
    } catch (err) {
      const errorMsg = err instanceof Error ? err.message : 'Unknown agent error';
      log.error('Agent send error', { error: errorMsg });
      sendError = errorMsg;
    } finally {
      cleanupListeners();
      isStreaming = false;
      await scrollToBottom();
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

    // Hydration fallback: the viewer assumes the node is in sharedNodeStore, but
    // a node opened directly (e.g. after reload, before the tree has loaded it)
    // may be absent. Fetch it by id so the dispatcher always has properties to
    // route on — the root cause class behind the original "picker does nothing".
    if (!sharedNodeStore.getNode(nodeId)) {
      try {
        const fetched = await fetchNode(nodeId);
        if (fetched) {
          sharedNodeStore.setNode(fetched, {
            type: 'database',
            reason: 'ai-chat-viewer hydration fallback',
          });
        }
      } catch (err) {
        log.warn('Failed to hydrate ai-chat node by id', { nodeId, error: String(err) });
      }
    }

    if (node) {
      loadMessagesFromNode();
      initialLoadDone = true;
    }
    if (node?.content) onTitleChange?.(node.content);

    if (isTauri()) {
      try {
        ollamaReady = await ollamaAvailable();
      } catch (err) {
        log.debug('Ollama availability check failed', { error: String(err) });
      }
    }
  });

  onDestroy(() => {
    if (hasUnsavedChanges) flushToStore();
    if (flushTimer) clearTimeout(flushTimer);
    teardownSession();
  });

  // Auto-scroll as messages stream/append.
  $effect(() => {
    void inMemoryMessages.length;
    scrollToBottom();
  });

  // The viewer assumes its node is in sharedNodeStore. If it wasn't at mount (the
  // hydration fallback fetches it asynchronously), load the persisted messages
  // once the node first becomes available — guarded so a live conversation is
  // never reloaded out from under the user.
  $effect(() => {
    if (node && !initialLoadDone) {
      loadMessagesFromNode();
      initialLoadDone = true;
    }
  });

  // Update title when node content changes
  $effect(() => {
    if (node?.content) onTitleChange?.(node.content);
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
        <span class="meta-badge" class:meta-archived={status === 'archived'}>
          {status}
        </span>
      </div>
    </div>
    <div class="chat-viewer-header-right">
      {#if hasUnsavedChanges}
        <span class="save-indicator" title="Unsaved changes (auto-saving...)">
          <svg
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            stroke-width="2"
            width="14"
            height="14"
          >
            <circle cx="12" cy="12" r="3" />
          </svg>
        </span>
      {/if}
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

  {#if provider === undefined}
    <!-- Placeholder: prompt to pick a mode from the dropdown above. -->
    <div class="provider-prompt">
      <p class="provider-prompt-text">Choose how this conversation is powered</p>
      <p class="provider-prompt-hint">
        Pick a provider from the dropdown in the top-right to get started.
      </p>
    </div>
  {:else if provider === 'pty'}
    <!-- Mode 2d: embedded terminal IS the viewer. -->
    <AiChatPtySession {nodeId} />
  {:else if isMessageProvider && !model}
    <!-- native | ollama, no model chosen yet → model picker. -->
    <AiChatModelPicker {nodeId} provider={provider as 'native' | 'ollama'} />
  {:else if isMessageProvider}
    <!-- Modes 2a/2b/2c with a model: message UI. -->
    <div
      class="chat-viewer-messages"
      bind:this={messagesContainer}
      role="list"
      aria-label="Chat conversation"
    >
      {#if inMemoryMessages.length === 0}
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
        {#each inMemoryMessages as message (message.id)}
          <ChatMessage {message} />
        {/each}
      {/if}

      {#if isStreaming}
        <div class="typing-indicator" aria-label="AI is thinking">
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
          <span class="typing-dot"></span>
        </div>
      {/if}

      {#if showMessageCap}
        <div class="message-cap-nudge" role="alert">
          <p>
            This conversation has {inMemoryMessages.length} messages. Consider starting a new chat for
            better performance.
          </p>
        </div>
      {/if}
    </div>

    {#if sendError}
      <div class="send-error" role="alert">{sendError}</div>
    {/if}

    {#if status !== 'archived'}
      <ChatInput
        onSend={handleSend}
        disabled={isStreaming}
        placeholder={isStreaming ? 'AI is responding...' : 'Type a message...'}
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

  .save-indicator {
    display: flex;
    align-items: center;
    color: hsl(var(--muted-foreground));
    animation: pulse-save 1.5s ease-in-out infinite;
  }

  @keyframes pulse-save {
    0%,
    100% {
      opacity: 0.4;
    }
    50% {
      opacity: 1;
    }
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
