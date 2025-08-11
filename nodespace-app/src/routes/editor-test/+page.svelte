<!--
  Simple Editor Test Page
  
  Single TextNode for testing hybrid markdown rendering behavior
-->

<script lang="ts">
  import TextNode from '$lib/components/TextNode.svelte';
</script>

<svelte:head>
  <title>Editor Test - NodeSpace</title>
</svelte:head>

<div class="editor-test">
  <h1>Editor Test Page</h1>
  <p>Testing hybrid markdown rendering with auto-expanding TextNode</p>
  
  <div class="editor-container">
    <TextNode 
      nodeId="test-editor"
      content="# Start typing here..."
      editable={true}
      markdown={true}
      placeholder="Click to start editing. Try: # Headers, **bold**, *italic*, [links](url)"
      autoSave={false}
    />
  </div>
</div>

<style>
  .editor-test {
    max-width: 800px;
    margin: 0 auto;
    padding: 2rem;
    min-height: 100vh;
  }
  
  .editor-test h1 {
    margin-bottom: 0.5rem;
    color: hsl(var(--foreground));
  }
  
  .editor-test p {
    margin-bottom: 2rem;
    color: hsl(var(--muted-foreground));
  }
  
  .editor-container {
    width: 100%;
    border: none; /* Remove border completely */
    border-radius: 0;
    padding: 0.5rem; /* Reduced padding */
    background: hsl(var(--background));
  }
  
  /* Ensure the TextNode takes full width but starts single-line */
  .editor-container :global(.ns-node) {
    width: 100%;
  }
  
  .editor-container :global(.ns-node__content) {
    width: 100%;
  }
  
  .editor-container :global(.ns-node__display) {
    min-height: auto; /* Let it be natural height */
    display: flex;
    align-items: flex-start; /* Top align instead of center */
  }
  
  /* CodeMirror should start single-line height and grow as needed */
  .editor-container :global(.codemirror-editor) {
    width: 100%;
  }
  
  .editor-container :global(.cm-content) {
    white-space: pre-wrap !important;
    overflow-wrap: break-word !important;
  }
</style>