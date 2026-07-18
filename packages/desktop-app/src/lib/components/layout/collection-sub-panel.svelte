<!--
  CollectionSubPanel - Slide-out panel showing collection member nodes

  Displays when a collection is clicked in the navigation sidebar.
  Slides in from the left, adjacent to the sidebar.
-->

<script lang="ts">
  import Icon, { type IconName } from '$lib/design/icons/icon.svelte';
  import type { CollectionMember } from '$lib/stores/collections.svelte';

  interface Props {
    open: boolean;
    collectionName: string;
    members: CollectionMember[];
    onClose: () => void;
    onNodeClick: (_nodeId: string, _nodeType: string) => void;
    /** Open the collection's own page (Contents / Collaboration tabs). */
    onOpenCollection: () => void;
  }

  let { open, collectionName, members, onClose, onNodeClick, onOpenCollection }: Props = $props();

  // Map content node types to the closest available icon (see icon.svelte for
  // the full IconName set). Anything unmapped falls back to the generic text glyph.
  function getNodeIcon(nodeType: string): IconName {
    const iconMap: Record<string, IconName> = {
      date: 'calendar',
      task: 'taskIncomplete',
      checkbox: 'taskIncomplete',
      'ai-chat': 'aiSquare',
      prompt: 'aiSquare',
      skill: 'aiSquare',
      query: 'aiSquare',
      text: 'text',
      header: 'text',
      'code-block': 'text',
      'quote-block': 'text',
      'ordered-list': 'text'
    };
    return iconMap[nodeType] ?? 'text';
  }

  // Nicely-cased display labels for known content types; anything else is
  // title-cased from its raw id (e.g. 'my-thing' -> 'My Thing').
  const NODE_TYPE_LABELS: Record<string, string> = {
    text: 'Text',
    header: 'Header',
    task: 'Task',
    checkbox: 'Checkbox',
    'code-block': 'Code',
    'quote-block': 'Quote',
    'ordered-list': 'List',
    'ai-chat': 'AI Chat',
    query: 'Query',
    prompt: 'Prompt',
    skill: 'Skill',
    date: 'Date'
  };

  function humanizeNodeType(nodeType: string): string {
    return (
      NODE_TYPE_LABELS[nodeType] ??
      nodeType
        .split(/[-_]/)
        .filter(Boolean)
        .map((word) => word.charAt(0).toUpperCase() + word.slice(1))
        .join(' ')
    );
  }

  // A muted placeholder for content with no name, so blank rows read as
  // "Untitled text" / "Untitled task" instead of appearing empty/unclickable.
  function fallbackName(nodeType: string): string {
    return `Untitled ${humanizeNodeType(nodeType).toLowerCase()}`;
  }
</script>

<div class="sub-panel" class:open>
  <div class="sub-panel-header">
    <button
      class="sub-panel-title"
      onclick={onOpenCollection}
      title="Open collection (manage members & collaboration)"
    >
      <span class="sub-panel-title-text">{collectionName}</span>
      <Icon name="chevronRight" size={14} />
    </button>
    <button class="close-btn" onclick={onClose} aria-label="Close panel">
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2">
        <path d="M18 6L6 18M6 6l12 12" />
      </svg>
    </button>
  </div>

  <ul class="node-list">
    {#each members as member (member.id)}
      {@const trimmedName = member.name.trim()}
      <li>
        <button class="node-item" onclick={() => onNodeClick(member.id, member.nodeType)}>
          <Icon name={getNodeIcon(member.nodeType)} size={16} />
          <span class="node-name" class:node-name--untitled={!trimmedName}>
            {trimmedName || fallbackName(member.nodeType)}
          </span>
          <span class="node-type-tag">{humanizeNodeType(member.nodeType)}</span>
        </button>
      </li>
    {/each}
    {#if members.length === 0}
      <li class="empty-state">No nodes in this collection</li>
    {/if}
  </ul>
</div>

<style>
  .sub-panel {
    position: absolute;
    left: var(--sidebar-width, 240px); /* Adjacent to expanded sidebar */
    top: 0;
    width: var(--sidebar-width, 240px);
    height: 100%;
    background: hsl(var(--sidebar-background));
    border-right: 1px solid hsl(var(--border));
    box-shadow: 2px 0 8px rgba(0, 0, 0, 0.1);
    transform: translateX(-100%);
    opacity: 0;
    transition:
      transform 250ms ease-out,
      opacity 250ms ease-out;
    z-index: 20;
    display: flex;
    flex-direction: column;
    pointer-events: none;
  }

  .sub-panel.open {
    transform: translateX(0);
    opacity: 1;
    pointer-events: auto;
  }

  .sub-panel-header {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 1rem;
    border-bottom: 1px solid hsl(var(--border));
    flex-shrink: 0;
  }

  .sub-panel-title {
    display: flex;
    align-items: center;
    gap: 0.25rem;
    min-width: 0;
    font-size: 0.875rem;
    font-weight: 600;
    color: hsl(var(--foreground));
    background: none;
    border: none;
    padding: 0.125rem 0.25rem;
    margin: -0.125rem -0.25rem;
    border-radius: 4px;
    cursor: pointer;
    transition: background-color 0.2s;
  }

  .sub-panel-title:hover {
    background: hsl(var(--border));
  }

  .sub-panel-title-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .close-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 24px;
    height: 24px;
    background: none;
    border: none;
    cursor: pointer;
    color: hsl(var(--muted-foreground));
    border-radius: 4px;
    transition:
      background-color 0.2s,
      color 0.2s;
    flex-shrink: 0;
  }

  .close-btn:hover {
    background: hsl(var(--border));
    color: hsl(var(--foreground));
  }

  .close-btn svg {
    width: 16px;
    height: 16px;
  }

  .node-list {
    flex: 1;
    overflow-y: auto;
    padding: 0.5rem 0;
    margin: 0;
    list-style: none;
  }

  .node-item {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    width: 100%;
    padding: 0.5rem 1rem;
    background: none;
    border: none;
    cursor: pointer;
    text-align: left;
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
    transition:
      background-color 0.2s,
      color 0.2s;
  }

  .node-item:hover {
    background: hsl(var(--border));
    color: hsl(var(--foreground));
  }

  .node-name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* Muted placeholder styling for blank/untitled content. */
  .node-name--untitled {
    font-style: italic;
    opacity: 0.7;
  }

  /* Small muted type tag so content types are distinguishable at a glance. */
  .node-type-tag {
    flex-shrink: 0;
    font-size: 0.6875rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: hsl(var(--muted-foreground));
    opacity: 0.65;
  }

  .empty-state {
    padding: 1rem;
    text-align: center;
    color: hsl(var(--muted-foreground));
    font-size: 0.875rem;
    font-style: italic;
  }
</style>
