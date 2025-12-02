<!--
  Query Node Component

  Displays query nodes with their description and filter criteria.
  Query nodes are primarily created by AI/MCP, not manually.

  Full query execution and visualization will be implemented in Issue #443.
  This is a minimal working component for the plugin system.
-->

<script lang="ts">
	import BaseNode from '$lib/design/components/base-node.svelte';

	// Props using Svelte 5 runes mode - same interface as BaseNode
	let {
		nodeId,
		nodeType = 'query',
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
</script>

<!-- Query nodes use BaseNode for standard node behavior -->
<BaseNode {nodeId} {nodeType} {autoFocus} bind:content {children} {metadata} />

<style>
	/* Query nodes use default BaseNode styling */
</style>
