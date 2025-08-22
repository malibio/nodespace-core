<!--
  TaskNodeReference - Placeholder Task Reference Component
  
  Simple placeholder that uses BaseNodeReference with task-specific styling.
  Can be enhanced later with rich task features.
-->

<script lang="ts">
  import BaseNodeReference from './BaseNodeReference.svelte';
  
  // Basic task props
  export let nodeId: string;
  export let content: string;
  export let href: string;
  export let completed: boolean = false;
  export let priority: 'low' | 'medium' | 'high' | 'critical' = 'medium';
  
  // Optional props
  export let className: string = '';
  export let style: string = '';
  
  $: checkboxIcon = completed ? '☑' : '☐';
  $: taskClassName = `ns-task-ref--${priority} ${completed ? 'ns-task-ref--completed' : ''} ${className}`;
</script>

<BaseNodeReference
  {nodeId}
  {href}
  content=""
  nodeType="task"
  className={taskClassName}
  {style}
  icon={checkboxIcon}
  ariaLabel="Task: {content} ({completed ? 'completed' : 'pending'})"
  on:nodeClick
>
  <span slot="content" class="task-content" class:completed>
    {content}
  </span>
</BaseNodeReference>

<style>
  .task-content.completed {
    text-decoration: line-through;
    opacity: 0.7;
  }
  
  :global(.ns-task-ref--completed) {
    background: hsl(var(--muted) / 0.5);
  }
  
  :global(.ns-task-ref--high) {
    border-left: 0.125rem solid hsl(var(--destructive));
  }
  
  :global(.ns-task-ref--critical) {
    border-left: 0.125rem solid hsl(var(--destructive));
    box-shadow: 0 0 0 1px hsl(var(--destructive) / 0.3);
  }
</style>