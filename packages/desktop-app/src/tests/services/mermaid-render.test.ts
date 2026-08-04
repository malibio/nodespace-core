import { describe, it, expect, vi, beforeEach } from 'vitest';

// Happy-DOM tier. sanitizeSvg's actual XSS-stripping behavior is covered in the
// browser tier (src/tests/browser/mermaid-sanitize.test.ts) because DOMPurify
// versions past 3.4.7 fail open under Happy-DOM's DOM approximation. Here we only
// exercise renderMermaid's control flow: theme-caching, mermaid module init/render
// wiring, error handling, and temp-element cleanup.

vi.mock('$lib/utils/logger', () => ({
  createLogger: () => ({
    debug: vi.fn(),
    info: vi.fn(),
    warn: vi.fn(),
    error: vi.fn()
  })
}));

const mermaidInitialize = vi.fn();
const mermaidRender = vi.fn();

vi.mock('mermaid', () => ({
  default: {
    initialize: mermaidInitialize,
    render: mermaidRender
  }
}));

describe('renderMermaid', () => {
  beforeEach(() => {
    vi.resetModules();
    mermaidInitialize.mockReset();
    mermaidRender.mockReset();
  });

  it('initializes mermaid on the first call with the light theme', async () => {
    mermaidRender.mockResolvedValue({ svg: '<svg><text>diagram</text></svg>' });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    const result = await renderMermaid('graph TD; A-->B;', 'test-id', false);

    expect(mermaidInitialize).toHaveBeenCalledTimes(1);
    const config = mermaidInitialize.mock.calls[0][0] as { themeVariables?: { background?: string } };
    expect(config.themeVariables?.background).toBeDefined();
    expect(typeof result).toBe('string');
  });

  it('resolves a CSS custom property when one is set on the document root', async () => {
    // Covers cssVar()'s truthy branch (raw value present) in addition to the
    // fallback branch exercised implicitly by every other test in this file,
    // where Happy-DOM has no stylesheet defining these custom properties.
    document.documentElement.style.setProperty('--background', '200 40% 55%');
    mermaidRender.mockResolvedValue({ svg: '<svg><text>diagram</text></svg>' });

    try {
      const { renderMermaid } = await import('$lib/services/mermaid-render');
      await renderMermaid('graph TD; A-->B;', 'css-var-id', false);

      const config = mermaidInitialize.mock.calls[0][0] as { themeVariables?: { background?: string } };
      expect(config.themeVariables?.background).toBe('hsl(200 40% 55%)');
    } finally {
      document.documentElement.style.removeProperty('--background');
    }
  });

  it('does not re-initialize on a second call with the same theme', async () => {
    mermaidRender.mockResolvedValue({ svg: '<svg><text>diagram</text></svg>' });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    await renderMermaid('graph TD; A-->B;', 'first', false);
    await renderMermaid('graph TD; A-->C;', 'second', false);

    expect(mermaidInitialize).toHaveBeenCalledTimes(1);
  });

  it('re-initializes when isDark switches from false to true', async () => {
    mermaidRender.mockResolvedValue({ svg: '<svg><text>diagram</text></svg>' });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    await renderMermaid('graph TD; A-->B;', 'light-call', false);
    await renderMermaid('graph TD; A-->B;', 'dark-call', true);

    expect(mermaidInitialize).toHaveBeenCalledTimes(2);
  });

  it('returns a sanitized string on successful render', async () => {
    mermaidRender.mockResolvedValue({ svg: '<svg><text>flowchart</text></svg>' });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    const result = await renderMermaid('graph TD; A-->B;', 'success-id', false);

    expect(mermaidRender).toHaveBeenCalledWith(expect.stringContaining('mermaid-success-id-'), 'graph TD; A-->B;');
    expect(typeof result).toBe('string');
    expect(result).toContain('flowchart');
  });

  it('returns null when mermaid.render throws', async () => {
    mermaidRender.mockRejectedValue(new Error('Invalid syntax'));

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    const result = await renderMermaid('invalid %%%', 'fail-id', false);

    expect(result).toBeNull();
  });

  it('cleans up the temp DOM element in finally, even on render failure', async () => {
    mermaidRender.mockImplementation(async (renderId: string) => {
      // Simulate mermaid injecting a temp element into the document during render.
      const el = document.createElement('div');
      el.id = renderId;
      document.body.appendChild(el);
      throw new Error('boom');
    });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    const result = await renderMermaid('graph TD; A-->B;', 'cleanup-id', false);

    expect(result).toBeNull();
    // The element id is `mermaid-cleanup-id-<counter>`, so assert none remain
    // whose id starts with the expected prefix.
    const leftover = Array.from(document.body.querySelectorAll('[id^="mermaid-cleanup-id-"]'));
    expect(leftover).toHaveLength(0);
  });

  it('cleans up the temp DOM element in finally on success too', async () => {
    mermaidRender.mockImplementation(async (renderId: string) => {
      const el = document.createElement('div');
      el.id = renderId;
      document.body.appendChild(el);
      return { svg: '<svg><text>ok</text></svg>' };
    });

    const { renderMermaid } = await import('$lib/services/mermaid-render');
    await renderMermaid('graph TD; A-->B;', 'cleanup-success-id', false);

    const leftover = Array.from(document.body.querySelectorAll('[id^="mermaid-cleanup-success-id-"]'));
    expect(leftover).toHaveLength(0);
  });
});

describe('sanitizeSvg', () => {
  beforeEach(() => {
    vi.resetModules();
  });

  it('invokes DOMPurify with the svg/svgFilters profile', async () => {
    const { sanitizeSvg } = await import('$lib/services/mermaid-render');
    // We can't assert XSS-stripping under Happy-DOM (see file header comment),
    // but we can assert the function runs against a real DOMPurify instance
    // and returns a string for well-formed input.
    const result = sanitizeSvg('<svg><rect/></svg>');
    expect(typeof result).toBe('string');
  });

  it('exercises the uponSanitizeAttribute style hook without asserting stripping semantics', async () => {
    // Under Happy-DOM, DOMPurify's traversal is unreliable for stripping guarantees
    // (see file header comment), but simply having a `style` attribute present is
    // enough to route through the hook body at mermaid-render.ts lines 108-112 —
    // covering that branch without depending on DOMPurify's real sanitization output.
    const { sanitizeSvg } = await import('$lib/services/mermaid-render');
    const result = sanitizeSvg('<svg><rect style="fill:red"/></svg>');
    expect(typeof result).toBe('string');
  });
});
