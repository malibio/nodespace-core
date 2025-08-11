<!--
  Performance Testing Page
  
  Dedicated page for running CodeMirror performance benchmarks
-->

<script lang="ts">
  import TestRunner from '$lib/performance/test-runner.svelte';
</script>

<svelte:head>
  <title>Performance Benchmarks - NodeSpace</title>
  <meta name="description" content="CodeMirror performance benchmarks for NodeSpace" />
</svelte:head>

<TestRunner />