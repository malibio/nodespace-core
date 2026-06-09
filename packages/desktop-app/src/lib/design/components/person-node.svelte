<!--
  PersonNode - Wraps BaseNode for identity nodes (contacts, collaborators)

  Renders inline with a person icon. Name is stored as node content;
  email lives in properties.person.email.
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from './base-node.svelte';

  let {
    nodeId,
    nodeType = 'person',
    autoFocus = false,
    content = $bindable(''),
    children = [],
    metadata = {}
  }: {
    nodeId: string;
    nodeType?: string;
    autoFocus?: boolean;
    content?: string;
    children?: string[];
    metadata?: Record<string, unknown>;
  } = $props();

  const dispatch = createEventDispatcher();

  const editableConfig = { allowMultiline: false };

  function forwardEvent<T>(eventName: string) {
    return (event: CustomEvent<T>) => dispatch(eventName, event.detail);
  }
</script>

<div class="person-node-wrapper">
  <BaseNode
    {nodeId}
    {nodeType}
    {autoFocus}
    bind:content
    {children}
    {editableConfig}
    {metadata}
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
</div>
