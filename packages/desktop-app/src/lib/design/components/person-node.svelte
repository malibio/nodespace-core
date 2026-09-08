<!--
  PersonNode - Wraps BaseNode for identity nodes (contacts, collaborators, stakeholders)

  Display identity is composed by the person schema's title_template
  ("{first_name} {last_name}"), so this node is read-only inline — same
  `readonly`/`displayContentIsPlaceholder` treatment node-row.svelte's
  BaseNode fallback branch gives other title_template-driven types.
  PersonNode has its own lazy-loaded node component (this file), so it
  never reaches that fallback branch and must compute the same thing
  itself. first_name/last_name/email all live in properties.person and are
  editable via the PersonSchemaForm property panel.
-->

<script lang="ts">
  import { createEventDispatcher } from 'svelte';
  import BaseNode from './base-node.svelte';
  import { pluginRegistry } from '$lib/plugins/plugin-registry';
  import { sharedNodeStore } from '$lib/services/shared-node-store.svelte';
  import { HAS_RESOLVED_CHARACTER_RE } from '$lib/utils/node-display-title';
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

  const title = $derived(sharedNodeStore.getNode(nodeId)?.title);
  const titleResolved = $derived(!!title && HAS_RESOLVED_CHARACTER_RE.test(title));
  const displayContent = $derived(
    titleResolved ? (title as string) : (pluginRegistry.getTitleTemplate('person') ?? '')
  );
</script>

<BaseNode
  {nodeId}
  {nodeType}
  {autoFocus}
  bind:content
  {children}
  {editableConfig}
  readonly
  displayContentIsPlaceholder={!titleResolved}
  {displayContent}
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
