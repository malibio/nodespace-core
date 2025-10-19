<!--
  OrderedListIcon - Numbered List Icon for Ordered List Nodes

  CSS-based approach with numbered list overlay icon
  Displays Material Design numbered list icon on circular background

  Design System Reference: docs/design-system/patterns/foundations/icon-systems.html → OrderedListNode Icon System
-->

<script lang="ts">
  let {
    size = 20,
    hasChildren = false,
    className = ''
  }: {
    size?: number;
    hasChildren?: boolean;
    className?: string;
  } = $props();
</script>

<div
  class="ns-icon ordered-list-icon {className}"
  class:ordered-list-icon-with-ring={hasChildren}
  style="--icon-size: {size}px"
  role="img"
  aria-label="Ordered list icon"
>
  {#if hasChildren}
    <div class="node-ring"></div>
  {/if}
  <div class="list-circle"></div>
</div>

<style>
  .ns-icon {
    width: var(--icon-size, 20px);
    height: var(--icon-size, 20px);
    position: relative;
    display: block;
    flex-shrink: 0;
  }

  .ordered-list-icon-with-ring {
    width: calc(var(--icon-size, 20px) + 4px);
    height: calc(var(--icon-size, 20px) + 4px);
  }

  /* Ordered list circle - 16px circle at (2,2) position in 20px container */
  .list-circle {
    width: 16px;
    height: 16px;
    border-radius: 50%;
    position: absolute;
    top: 2px;
    left: 2px;
    background: hsl(var(--node-text));
  }

  /* Numbered list overlay - Material Design format_list_numbered icon */
  .list-circle::after {
    content: '';
    position: absolute;
    left: 50%;
    top: 50%;
    width: 16px;
    height: 16px;
    transform: translate(-50%, -50%);
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' height='16px' viewBox='0 0 24 24' width='16px' fill='%23fcfcfc'%3E%3Cpath d='M0 0h24v24H0V0z' fill='none'/%3E%3Cpath d='M2 17h2v.5H3v1h1v.5H2v1h3v-4H2v1zm1-9h1V4H2v1h1v3zm-1 3h1.8L2 13.1v.9h3v-1H3.2L5 10.9V10H2v1zm5-6v2h14V5H7zm0 14h14v-2H7v2zm0-6h14v-2H7v2z'/%3E%3C/svg%3E");
    background-repeat: no-repeat;
    background-position: center;
    background-size: 16px 16px;
  }

  /* Dark theme - numbered list icon uses dark background color */
  :global(.dark) .list-circle::after {
    background-image: url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' height='16px' viewBox='0 0 24 24' width='16px' fill='%23252523'%3E%3Cpath d='M0 0h24v24H0V0z' fill='none'/%3E%3Cpath d='M2 17h2v.5H3v1h1v.5H2v1h3v-4H2v1zm1-9h1V4H2v1h1v3zm-1 3h1.8L2 13.1v.9h3v-1H3.2L5 10.9V10H2v1zm5-6v2h14V5H7zm0 14h14v-2H7v2zm0-6h14v-2H7v2z'/%3E%3C/svg%3E");
  }

  /* Parent node ring - NOT used for ordered-list since canHaveChildren: false */
  .node-ring {
    width: 20px;
    height: 20px;
    border-radius: 50%;
    border: 2px solid hsl(var(--node-text) / 0.5);
    box-sizing: border-box;
    position: absolute;
    top: 0;
    left: 0;
  }
</style>
