import { describe, it, expect } from 'vitest';
import { extractResourceRoot } from '../install.js';

describe('extractResourceRoot', () => {
  it('returns no resourceRoot and all args unchanged when the flag is absent', () => {
    const { rest, resourceRoot } = extractResourceRoot(['install', 'claude-code']);
    expect(resourceRoot).toBeUndefined();
    expect(rest).toEqual(['install', 'claude-code']);
  });

  it('extracts --resource-root and its value, leaving the remaining args in order', () => {
    const { rest, resourceRoot } = extractResourceRoot([
      'install',
      'claude-code',
      '--resource-root',
      '/path/to/resources/skill'
    ]);
    expect(resourceRoot).toBe('/path/to/resources/skill');
    expect(rest).toEqual(['install', 'claude-code']);
  });

  it('extracts --resource-root when it appears before the positional args', () => {
    const { rest, resourceRoot } = extractResourceRoot([
      '--resource-root',
      '/path/to/resources/skill',
      'install'
    ]);
    expect(resourceRoot).toBe('/path/to/resources/skill');
    expect(rest).toEqual(['install']);
  });

  it('extracts --resource-root with no other args at all', () => {
    const { rest, resourceRoot } = extractResourceRoot(['--resource-root', '/only/this']);
    expect(resourceRoot).toBe('/only/this');
    expect(rest).toEqual([]);
  });

  it('handles a bare --resource-root with no following value without throwing', () => {
    const { rest, resourceRoot } = extractResourceRoot(['install', '--resource-root']);
    // The next token (which doesn't exist) is consumed as the value; there is
    // nothing left to hand `install()` — deliberately not special-cased, this
    // matches every other CLI flag's "you must actually pass a value" shape.
    expect(resourceRoot).toBeUndefined();
    expect(rest).toEqual(['install']);
  });
});
