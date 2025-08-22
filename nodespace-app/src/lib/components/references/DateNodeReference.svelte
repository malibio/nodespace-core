<!--
  DateNodeReference - Placeholder Date Reference Component
  
  Simple placeholder that uses BaseNodeReference with date-specific styling.
  Can be enhanced later with rich date features.
-->

<script lang="ts">
  import BaseNodeReference from './BaseNodeReference.svelte';
  
  // Basic date props
  export let nodeId: string;
  export let content: string;
  export let href: string;
  export let date: Date | undefined = undefined;
  export let isToday: boolean = false;
  export let isPast: boolean = false;
  
  // Optional props
  export let className: string = '';
  export let style: string = '';
  
  $: calendarIcon = isToday ? '📅' : isPast ? '📆' : '🗓️';
  $: dateClassName = `ns-date-ref ${isToday ? 'ns-date-ref--today' : ''} ${isPast ? 'ns-date-ref--past' : 'ns-date-ref--future'} ${className}`;
</script>

<BaseNodeReference
  {nodeId}
  {href}
  content=""
  nodeType="date"
  className={dateClassName}
  {style}
  icon={calendarIcon}
  ariaLabel="Date: {content}{isToday ? ' (Today)' : ''}"
  on:nodeClick
>
  <span slot="content" class="date-content">
    {content}
    {#if isToday}
      <span class="today-badge">Today</span>
    {/if}
  </span>
</BaseNodeReference>

<style>
  .date-content {
    display: flex;
    align-items: center;
    gap: 0.25rem;
  }
  
  .today-badge {
    background: hsl(var(--primary));
    color: hsl(var(--primary-foreground));
    font-size: 0.625rem;
    padding: 0.125rem 0.25rem;
    border-radius: 0.125rem;
    font-weight: 500;
  }
  
  :global(.ns-date-ref--today) {
    background: hsl(var(--primary) / 0.1);
    border-color: hsl(var(--primary) / 0.3);
  }
  
  :global(.ns-date-ref--past) {
    opacity: 0.8;
  }
</style>