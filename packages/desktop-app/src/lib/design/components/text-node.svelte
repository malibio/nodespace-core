<!--
  TextNode - Wraps BaseNode for text content editing

  Individual text node component that provides smart multiline behavior
  based on header level:
  - Headers (h1-h6): Single-line only for semantic integrity
  - Regular text: Multi-line with Shift+Enter support
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from '$lib/design/components/base-node.svelte';

  const dispatch = createEventDispatcher();

  // Props
  let {
    nodeId,
    autoFocus = false,
    content = $bindable(''),
    nodeType = 'text',
    children = []
  }: {
    nodeId: string;
    autoFocus?: boolean;
    content?: string;
    nodeType?: string;
    children?: string[];
  } = $props();

  // Text nodes always allow multiline editing
  const editableConfig = {
    allowMultiline: true
  };

  function handleNodeTypeChanged(e: CustomEvent) {
    dispatch('nodeTypeChanged', e.detail);
  }
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {autoFocus}
  bind:content
  {children}
  {editableConfig}
  on:createNewNode
  on:contentChanged
  on:indentNode
  on:outdentNode
  on:navigateArrow
  on:combineWithPrevious
  on:slashCommandSelected
  on:nodeTypeChanged={handleNodeTypeChanged}
  on:deleteNode
  on:focus
  on:blur
/>
