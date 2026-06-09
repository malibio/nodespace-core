<!--
  PersonNode - Wraps BaseNode for identity nodes (contacts, collaborators, stakeholders)

  Name is stored as node content; email lives in properties.person.email and
  is editable via the PersonSchemaForm property panel.
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from './base-node.svelte';
  import type { NodeComponentProps } from '$lib/types/node-viewers.js';

  let {
    nodeId,
    nodeType = 'person',
    autoFocus = false,
    content = $bindable(''),
    children = []
  }: NodeComponentProps = $props();

  const dispatch = createEventDispatcher();

  const editableConfig = { allowMultiline: false };

  function forwardEvent<T>(eventName: string) {
    return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
  }
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {autoFocus}
  bind:content
  {children}
  {editableConfig}
  on:createNewNode={forwardEvent('createNewNode')}
  on:contentChanged={forwardEvent('contentChanged')}
  on:indentNode={forwardEvent('indentNode')}
  on:outdentNode={forwardEvent('outdentNode')}
  on:navigateArrow={forwardEvent('navigateArrow')}
  on:combineWithPrevious={forwardEvent('combineWithPrevious')}
  on:deleteNode={forwardEvent('deleteNode')}
  on:focus={forwardEvent('focus')}
  on:blur={forwardEvent('blur')}
  on:nodeReferenceSelected={forwardEvent('nodeReferenceSelected')}
  on:slashCommandSelected={forwardEvent('slashCommandSelected')}
  on:nodeTypeChanged={forwardEvent('nodeTypeChanged')}
  on:iconClick={forwardEvent('iconClick')}
/>
