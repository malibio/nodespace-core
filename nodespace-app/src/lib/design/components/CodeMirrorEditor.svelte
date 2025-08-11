<!--
  CodeMirrorEditor Component
  
  Minimal CodeMirror 6 integration for NodeSpace's hybrid markdown rendering.
  Replaces textarea-based editing with native CodeMirror capabilities.
-->

<script lang="ts">
  import { createEventDispatcher, onMount, onDestroy } from 'svelte';
  // Optimized imports for tree shaking - import only what we use
  import { EditorView, type ViewUpdate } from '@codemirror/view';
  import { EditorState } from '@codemirror/state';
  import { markdown as markdownSupport } from '@codemirror/lang-markdown';

  // Props
  export let content = '';
  export let multiline = false;
  export let markdown = false;
  export let editable = true;
  export let placeholder = '';

  // Internal state
  let editorElement: HTMLDivElement;
  let editorView: EditorView | undefined;
  let internalContent = content;

  const dispatch = createEventDispatcher<{
    contentChanged: { content: string };
  }>();

  // Reactive updates
  $: if (editorView && content !== internalContent) {
    updateEditorContent(content);
  }

  // Create extensions based on props
  function createExtensions() {
    const extensions = [
      // Basic theme and styling
      EditorView.theme({
        '&': {
          fontSize: '14px',
          lineHeight: '1.4',
          fontFamily: 'inherit'
        },
        '.cm-content': {
          padding: '0',
          minHeight: '20px',
          caretColor: 'hsl(var(--foreground))',
          color: 'hsl(var(--foreground))'
        },
        '.cm-focused': {
          outline: 'none'
        },
        '.cm-editor': {
          outline: 'none',
          background: 'transparent'
        },
        '.cm-scroller': {
          fontFamily: 'inherit',
          fontSize: 'inherit',
          lineHeight: 'inherit'
        },
        '.cm-line': {
          wordBreak: 'break-word'
        },
        '&.cm-focused .cm-selectionBackground': {
          backgroundColor: 'hsl(var(--accent) / 0.3)'
        },
        '.cm-selectionBackground': {
          backgroundColor: 'hsl(var(--accent) / 0.2)'
        }
      }),

      // NodeSpace hybrid markdown theme - applied when markdown prop is true
      ...(markdown
        ? [
            EditorView.theme({
              // H1 styling - 2rem, font-weight: 600, line-height: 1.38em
              '.cm-header1, .ͼ1.cm-header1': {
                fontSize: '2rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.38em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // H2 styling - 1.5rem, font-weight: 600, line-height: 1.38em  
              '.cm-header2, .ͼ1.cm-header2': {
                fontSize: '1.5rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.38em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // H3 styling - 1.2rem, font-weight: 600, line-height: 1.15em
              '.cm-header3, .ͼ1.cm-header3': {
                fontSize: '1.2rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.15em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // H4 styling - 1rem, font-weight: 600, line-height: 1.15em
              '.cm-header4, .ͼ1.cm-header4': {
                fontSize: '1rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.15em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // H5 styling - 0.9rem, font-weight: 600, line-height: 1.15em
              '.cm-header5, .ͼ1.cm-header5': {
                fontSize: '0.9rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.15em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // H6 styling - 0.8rem, font-weight: 600, line-height: 1.15em
              '.cm-header6, .ͼ1.cm-header6': {
                fontSize: '0.8rem !important',
                fontWeight: '600 !important',
                lineHeight: '1.15em !important',
                color: 'hsl(var(--foreground)) !important'
              },
              // Bold text styling
              '.cm-emphasis, .ͼ1.cm-emphasis': {
                fontWeight: '600 !important'
              },
              // Italic text styling
              '.cm-strong, .ͼ1.cm-strong': {
                fontStyle: 'italic !important'
              },
              // Inline code styling
              '.cm-monospace, .ͼ1.cm-monospace': {
                backgroundColor: 'hsl(var(--muted) / 0.3) !important',
                padding: '0.125rem 0.25rem !important',
                borderRadius: '0.25rem !important',
                fontSize: '0.9em !important',
                fontFamily: 'ui-monospace, "Cascadia Code", "Source Code Pro", Menlo, Consolas, "DejaVu Sans Mono", monospace !important'
              },
              // Link styling
              '.cm-link, .ͼ1.cm-link': {
                color: 'hsl(var(--primary)) !important',
                textDecoration: 'underline !important'
              },
              // Ensure markdown marks (#, **, *, `, etc.) remain visible but muted
              '.cm-processingInstruction, .ͼ1.cm-processingInstruction': {
                color: 'hsl(var(--muted-foreground)) !important',
                fontWeight: 'inherit !important',
                fontSize: 'inherit !important'
              }
            })
          ]
        : []),

      // Update listener with debouncing
      EditorView.updateListener.of((update: ViewUpdate) => {
        if (update.docChanged) {
          const newContent = update.state.doc.toString();
          if (newContent !== internalContent) {
            internalContent = newContent;
            // CodeMirror naturally debounces rapid changes
            dispatch('contentChanged', { content: newContent });
          }
        }
      }),

      // Placeholder functionality through CSS
      EditorView.theme({
        '.cm-content[data-placeholder]::before': {
          content: 'attr(data-placeholder)',
          position: 'absolute',
          color: 'hsl(var(--muted-foreground))',
          fontStyle: 'italic',
          pointerEvents: 'none',
          display: content ? 'none' : 'block'
        }
      }),

      // Add placeholder attribute when empty
      content === '' && placeholder
        ? EditorView.contentAttributes.of({ 'data-placeholder': placeholder })
        : []
    ];

    // Add markdown support if enabled
    if (markdown) {
      extensions.push(markdownSupport());
    }

    // Handle single-line mode
    if (!multiline) {
      extensions.push(
        // Prevent line breaks
        EditorState.transactionFilter.of((tr) => {
          if (tr.docChanged) {
            const text = tr.newDoc.toString();
            if (text.includes('\n')) {
              // Replace newlines with spaces
              const cleanText = text.replace(/\n/g, ' ');
              return {
                changes: {
                  from: 0,
                  to: tr.newDoc.length,
                  insert: cleanText
                }
              };
            }
          }
          return tr;
        }),

        // Handle Enter key to exit single-line mode
        EditorView.domEventHandlers({
          keydown(event) {
            if (event.key === 'Enter' && !event.shiftKey) {
              event.preventDefault();
              // Blur the editor to exit edit mode
              editorView?.contentDOM.blur();
              return true;
            }
            return false;
          }
        })
      );
    }

    // Handle read-only mode
    if (!editable) {
      extensions.push(EditorState.readOnly.of(true));
    }

    return extensions;
  }

  // Update editor content
  function updateEditorContent(newContent: string) {
    if (!editorView) return;

    const currentContent = editorView.state.doc.toString();
    if (currentContent !== newContent) {
      editorView.dispatch({
        changes: {
          from: 0,
          to: currentContent.length,
          insert: newContent
        }
      });
      internalContent = newContent;
    }
  }

  // Initialize CodeMirror
  onMount(() => {
    if (!editorElement) return;

    const state = EditorState.create({
      doc: content,
      extensions: createExtensions()
    });

    editorView = new EditorView({
      state,
      parent: editorElement
    });

    internalContent = content;
  });

  // Cleanup on destroy
  onDestroy(() => {
    if (editorView) {
      editorView.destroy();
      editorView = undefined;
    }
  });

  // Expose focus method for external use
  export function focus() {
    editorView?.focus();
  }

  // Expose blur method for external use
  export function blur() {
    editorView?.contentDOM.blur();
  }
</script>

<div
  bind:this={editorElement}
  class="codemirror-editor"
  class:codemirror-editor--multiline={multiline}
  class:codemirror-editor--single={!multiline}
  class:codemirror-editor--readonly={!editable}
></div>

<style>
  .codemirror-editor {
    width: 100%;
    background: transparent;
  }

  .codemirror-editor--single :global(.cm-content) {
    overflow-x: auto;
    overflow-y: hidden;
    white-space: nowrap;
    height: 20px;
    max-height: 20px;
  }

  .codemirror-editor--multiline :global(.cm-content) {
    white-space: pre-wrap;
    overflow-y: auto;
    min-height: 20px;
  }

  .codemirror-editor--readonly {
    cursor: default;
  }

  .codemirror-editor--readonly :global(.cm-content) {
    cursor: default;
  }
</style>
