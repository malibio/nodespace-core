<!--
  CodeMirror Editor Wrapper
  
  Svelte wrapper for CodeMirror 6 with basic markdown support.
  Provides clean integration with NodeSpace's BaseNode component.
-->

<script lang="ts">
  import { createEventDispatcher, onMount, onDestroy } from 'svelte';
  import { EditorView, highlightSpecialChars, drawSelection } from '@codemirror/view';
  import { EditorState, Compartment } from '@codemirror/state';
  import { markdown } from '@codemirror/lang-markdown';

  // Props
  export let content: string = '';
  export let editable: boolean = true;
  export let multiline: boolean = false;
  export let focused: boolean = false;
  export let className: string = '';

  // Component state
  let editorContainer: HTMLDivElement;
  let editorView: EditorView | null = null;
  let isInternalUpdate = false;
  
  // Compartments for reconfigurable extensions
  const editableCompartment = new Compartment();

  // Event dispatcher
  const dispatch = createEventDispatcher<{
    input: { content: string };
    focus: {};
    blur: {};
  }>();

  // Create the editor
  function createEditor() {
    if (!editorContainer) return;

    const state = EditorState.create({
      doc: content,
      extensions: [
        // Basic functionality
        highlightSpecialChars(),
        drawSelection(),
        markdown(),
        EditorView.theme({
          '&': {
            fontSize: '14px',
            lineHeight: '1.4'
          },
          '.cm-content': {
            padding: '0',
            minHeight: multiline ? '20px' : '20px',
            fontFamily: 'inherit',
            color: 'hsl(var(--foreground))',
            backgroundColor: 'transparent'
          },
          '.cm-focused': {
            outline: 'none'
          },
          '.cm-editor': {
            borderRadius: '0'
          },
          '.cm-scroller': {
            fontFamily: 'inherit'
          },
          // Hide line numbers for clean appearance
          '.cm-lineNumbers': {
            display: 'none'
          },
          '.cm-foldGutter': {
            display: 'none'
          },
          // Single line mode styling
          ...(multiline ? {} : {
            '.cm-content': {
              whiteSpace: 'nowrap',
              overflowX: 'auto',
              overflowY: 'hidden'
            },
            '.cm-line': {
              whiteSpace: 'nowrap'
            }
          })
        }),
        EditorView.updateListener.of((update) => {
          if (update.docChanged && !isInternalUpdate) {
            const newContent = update.state.doc.toString();
            
            // For single-line mode, replace newlines with spaces
            const processedContent = multiline ? newContent : newContent.replace(/\n/g, ' ');
            
            content = processedContent;
            dispatch('input', { content: processedContent });
          }
        }),
        // Focus/blur handlers
        EditorView.focusChangeEffect.of((state, focusing) => {
          if (focusing) {
            dispatch('focus', {});
          } else {
            dispatch('blur', {});
          }
          return null;
        }),
        // Handle single-line mode Enter key behavior
        EditorView.domEventHandlers({
          keydown: (event, view) => {
            if (!multiline && event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              // Blur the editor to exit edit mode (BaseNode handles this)
              view.contentDOM.blur();
              return true;
            }
            return false;
          }
        }),
        editableCompartment.of(EditorState.readOnly.of(!editable))
      ]
    });

    editorView = new EditorView({
      state,
      parent: editorContainer
    });

    // Auto-focus if needed
    if (focused) {
      editorView.focus();
    }
  }

  // Update editor content when prop changes
  $: if (editorView && content !== editorView.state.doc.toString()) {
    isInternalUpdate = true;
    editorView.dispatch({
      changes: {
        from: 0,
        to: editorView.state.doc.length,
        insert: content
      }
    });
    isInternalUpdate = false;
  }

  // Handle focus changes
  $: if (editorView) {
    if (focused && !editorView.hasFocus) {
      editorView.focus();
    }
  }

  // Handle editable changes
  $: if (editorView) {
    editorView.dispatch({
      effects: editableCompartment.reconfigure(EditorState.readOnly.of(!editable))
    });
  }

  onMount(() => {
    createEditor();
  });

  onDestroy(() => {
    if (editorView) {
      editorView.destroy();
      editorView = null;
    }
  });

  // Public API
  export function focus() {
    if (editorView) {
      editorView.focus();
    }
  }

  export function blur() {
    if (editorView) {
      editorView.contentDOM.blur();
    }
  }

  export function getTextLength(): number {
    return editorView?.state.doc.length || 0;
  }

  export function setSelection(from: number, to?: number) {
    if (editorView) {
      const toPos = to !== undefined ? to : from;
      editorView.dispatch({
        selection: { anchor: from, head: toPos }
      });
    }
  }

  // Expose container element for positioning calculations
  export function getContainer(): HTMLDivElement | undefined {
    return editorContainer;
  }
</script>

<div
  bind:this={editorContainer}
  class="codemirror-wrapper {className}"
  class:codemirror-wrapper--single={!multiline}
  class:codemirror-wrapper--multiline={multiline}
></div>

<style>
  .codemirror-wrapper {
    width: 100%;
    font-family: inherit;
    font-size: inherit;
    line-height: inherit;
  }

  .codemirror-wrapper--single :global(.cm-content) {
    max-height: 20px;
    overflow-y: hidden;
  }

  .codemirror-wrapper--multiline :global(.cm-content) {
    min-height: 20px;
  }

  /* Ensure CodeMirror integrates seamlessly with BaseNode styling */
  .codemirror-wrapper :global(.cm-editor) {
    background: transparent;
    border: none;
  }

  .codemirror-wrapper :global(.cm-content) {
    background: transparent;
    caret-color: hsl(var(--foreground));
  }

  .codemirror-wrapper :global(.cm-cursor) {
    border-left-color: hsl(var(--foreground));
  }

  .codemirror-wrapper :global(.cm-placeholder) {
    color: hsl(var(--muted-foreground));
    font-style: italic;
  }
</style>