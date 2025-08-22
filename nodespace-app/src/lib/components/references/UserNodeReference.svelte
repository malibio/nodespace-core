<!--
  UserNodeReference - Placeholder User Reference Component
  
  Simple placeholder that uses BaseNodeReference with user-specific styling.
  Can be enhanced later with rich user features.
-->

<script lang="ts">
  import BaseNodeReference from './BaseNodeReference.svelte';
  
  // Basic user props
  export let nodeId: string;
  export let content: string;
  export let href: string;
  export let isOnline: boolean = false;
  export let displayName: string = '';
  
  // Optional props
  export let className: string = '';
  export let style: string = '';
  
  $: userIcon = isOnline ? '🟢👤' : '👤';
  $: userClassName = `ns-user-ref ${isOnline ? 'ns-user-ref--online' : 'ns-user-ref--offline'} ${className}`;
  $: name = displayName || content;
</script>

<BaseNodeReference
  {nodeId}
  {href}
  content=""
  nodeType="user"
  className={userClassName}
  {style}
  icon="👤"
  ariaLabel="User: {name} ({isOnline ? 'online' : 'offline'})"
  on:nodeClick
>
  <span slot="content" class="user-content">
    {name}
    {#if isOnline}
      <span class="online-indicator" title="Online">🟢</span>
    {/if}
  </span>
</BaseNodeReference>

<style>
  .user-content {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  
  .online-indicator {
    font-size: 0.625rem;
    line-height: 1;
  }
  
  :global(.ns-user-ref--online) {
    border-color: hsl(var(--chart-1) / 0.5);
  }
  
  :global(.ns-user-ref--offline) {
    opacity: 0.8;
  }
</style>