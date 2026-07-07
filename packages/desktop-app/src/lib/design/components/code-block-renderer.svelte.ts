/**
 * CodeBlockRenderer — reactive render pipeline for a single code-block node.
 *
 * ADR-049: the syntax-highlight / mermaid render results live here as `$state` on a
 * store class; the component reads them directly and re-renders when they change. The
 * component triggers a render by calling `render()` / `renderPreview()` (an imperative
 * push into this async subsystem) rather than an `$effect` that writes reactive state.
 *
 * Staleness is handled by monotonic sequence counters so an out-of-order async result
 * from a superseded input is discarded instead of clobbering the current output.
 */

import { highlightCode, type HighlightLine } from '$lib/services/syntax-highlight';
import { renderMermaid } from '$lib/services/mermaid-render';

const PREVIEW_DEBOUNCE_MS = 300;

export class CodeBlockRenderer {
  /** Highlighted lines for non-mermaid, non-plaintext code. */
  highlightedLines = $state<HighlightLine[] | null>(null);
  /** Rendered mermaid diagram SVG (injected into the DOM by the component). */
  mermaidSvg = $state<string | null>(null);
  /** User-facing mermaid render error, if the diagram failed. */
  mermaidError = $state<string | null>(null);

  /** Split-panel live-preview SVG (mermaid edit mode). */
  previewSvg = $state<string | null>(null);
  /** Split-panel live-preview error. */
  previewError = $state<string | null>(null);

  // Monotonic counters to discard stale async results when inputs change rapidly.
  #renderSeq = 0;
  #previewSeq = 0;
  #previewTimer: ReturnType<typeof setTimeout> | undefined;

  /**
   * Render the display view for the given code/language/theme. Skips highlighting for
   * plaintext. `nodeId` scopes the mermaid render so concurrent blocks don't collide.
   */
  render(code: string, language: string, nodeId: string, isDark: boolean): void {
    const seq = ++this.#renderSeq;

    if (language === 'mermaid') {
      this.highlightedLines = null;
      this.mermaidError = null; // reset stale error so it doesn't flash while pending
      renderMermaid(code, nodeId, isDark).then((svg) => {
        if (seq !== this.#renderSeq) return; // superseded — preserve existing diagram
        if (svg !== null) {
          this.mermaidSvg = svg;
        } else {
          // Definitive failure: no diagram to preserve.
          this.mermaidSvg = null;
          this.mermaidError = 'Diagram rendering failed. Check your Mermaid syntax.';
        }
      });
    } else if (language !== 'plaintext') {
      this.mermaidSvg = null;
      this.mermaidError = null;
      highlightCode(code, language, isDark).then((lines) => {
        if (seq !== this.#renderSeq) return; // superseded — discard
        this.highlightedLines = lines;
      });
    } else {
      this.highlightedLines = null;
      this.mermaidSvg = null;
      this.mermaidError = null;
    }
  }

  /** Debounced mermaid live preview for the split-panel editor. */
  renderPreview(code: string, nodeId: string, isDark: boolean): void {
    if (this.#previewTimer) clearTimeout(this.#previewTimer);
    this.#previewTimer = setTimeout(() => {
      const seq = ++this.#previewSeq;
      renderMermaid(code, `${nodeId}-preview`, isDark).then((svg) => {
        if (seq !== this.#previewSeq) return; // superseded — discard
        if (svg !== null) {
          this.previewSvg = svg;
          this.previewError = null;
        } else {
          this.previewSvg = null;
          this.previewError = 'Syntax error — check your Mermaid definition.';
        }
      });
    }, PREVIEW_DEBOUNCE_MS);
  }

  /** Clear the split-panel preview (leaving edit mode or switching away from mermaid). */
  clearPreview(): void {
    if (this.#previewTimer) {
      clearTimeout(this.#previewTimer);
      this.#previewTimer = undefined;
    }
    this.previewSvg = null;
    this.previewError = null;
  }

  /** Cancel any pending debounce timer (component teardown). */
  destroy(): void {
    if (this.#previewTimer) {
      clearTimeout(this.#previewTimer);
      this.#previewTimer = undefined;
    }
  }
}
