import { describe, it, expect } from 'vitest';
import {
  isValidNodeRefId,
  splitTextIntoRefSegments
} from '$lib/design/components/wikilink-refs';

const UUID = '550e8400-e29b-41d4-a716-446655440000';
const UUID_UPPER = '550E8400-E29B-41D4-A716-446655440000';
const DATE = '2025-10-24';

describe('isValidNodeRefId', () => {
  it('accepts a lowercase UUID', () => {
    expect(isValidNodeRefId(UUID)).toBe(true);
  });

  it('rejects an uppercase UUID (backend is lowercase-only; stays literal text)', () => {
    expect(isValidNodeRefId(UUID_UPPER)).toBe(false);
  });

  it('accepts a valid ISO date', () => {
    expect(isValidNodeRefId(DATE)).toBe(true);
  });

  it('accepts a node/-prefixed UUID', () => {
    expect(isValidNodeRefId(`node/${UUID}`)).toBe(true);
  });

  it('accepts a node/-prefixed date', () => {
    expect(isValidNodeRefId(`node/${DATE}`)).toBe(true);
  });

  it('rejects a plain title token', () => {
    expect(isValidNodeRefId('TODO')).toBe(false);
    expect(isValidNodeRefId('some page title')).toBe(false);
  });

  it('rejects an empty token', () => {
    expect(isValidNodeRefId('')).toBe(false);
  });

  it('rejects a non-id kebab string', () => {
    expect(isValidNodeRefId('not-a-real-id')).toBe(false);
  });

  it('rejects a malformed / impossible date', () => {
    expect(isValidNodeRefId('2025-13-45')).toBe(false);
    expect(isValidNodeRefId('2025-02-30')).toBe(false);
    expect(isValidNodeRefId('2025-1-1')).toBe(false);
  });

  it('rejects a UUID with the wrong shape', () => {
    expect(isValidNodeRefId('550e8400e29b41d4a716446655440000')).toBe(false);
    expect(isValidNodeRefId(`${UUID}-extra`)).toBe(false);
  });
});

describe('splitTextIntoRefSegments', () => {
  it('returns no segments for empty text', () => {
    expect(splitTextIntoRefSegments('')).toEqual([]);
  });

  it('returns a single text segment when there are no wikilinks', () => {
    expect(splitTextIntoRefSegments('just plain text')).toEqual([
      { kind: 'text', value: 'just plain text' }
    ]);
  });

  it('splits a single valid UUID ref preserving surrounding text', () => {
    expect(splitTextIntoRefSegments(`See [[${UUID}]] here`)).toEqual([
      { kind: 'text', value: 'See ' },
      { kind: 'ref', id: UUID },
      { kind: 'text', value: ' here' }
    ]);
  });

  it('emits a ref-only segment when the whole string is a wikilink', () => {
    expect(splitTextIntoRefSegments(`[[${UUID}]]`)).toEqual([{ kind: 'ref', id: UUID }]);
  });

  it('splits a valid date ref', () => {
    expect(splitTextIntoRefSegments(`log for [[${DATE}]].`)).toEqual([
      { kind: 'text', value: 'log for ' },
      { kind: 'ref', id: DATE },
      { kind: 'text', value: '.' }
    ]);
  });

  it('strips a node/ prefix and emits the bare id', () => {
    expect(splitTextIntoRefSegments(`ref [[node/${UUID}]]`)).toEqual([
      { kind: 'text', value: 'ref ' },
      { kind: 'ref', id: UUID }
    ]);
  });

  it('handles multiple refs with text between and around them', () => {
    const b = '11111111-2222-4333-8444-555555555555';
    expect(splitTextIntoRefSegments(`a [[${UUID}]] b [[${b}]] c`)).toEqual([
      { kind: 'text', value: 'a ' },
      { kind: 'ref', id: UUID },
      { kind: 'text', value: ' b ' },
      { kind: 'ref', id: b },
      { kind: 'text', value: ' c' }
    ]);
  });

  it('handles adjacent refs with no text between', () => {
    const b = '11111111-2222-4333-8444-555555555555';
    expect(splitTextIntoRefSegments(`[[${UUID}]][[${b}]]`)).toEqual([
      { kind: 'ref', id: UUID },
      { kind: 'ref', id: b }
    ]);
  });

  it('leaves a junk token like [[TODO]] as literal text', () => {
    expect(splitTextIntoRefSegments('a [[TODO]] b')).toEqual([
      { kind: 'text', value: 'a [[TODO]] b' }
    ]);
  });

  it('leaves an empty [[]] as literal text', () => {
    expect(splitTextIntoRefSegments('before [[]] after')).toEqual([
      { kind: 'text', value: 'before [[]] after' }
    ]);
  });

  it('leaves a whitespace-only [[ ]] as literal text', () => {
    expect(splitTextIntoRefSegments('x [[ ]] y')).toEqual([
      { kind: 'text', value: 'x [[ ]] y' }
    ]);
  });

  it('leaves a title-phrase wikilink as literal text', () => {
    expect(splitTextIntoRefSegments('[[some page title]]')).toEqual([
      { kind: 'text', value: '[[some page title]]' }
    ]);
  });

  it('does not treat nested brackets [[[uuid]]] as a ref', () => {
    const input = `[[[${UUID}]]]`;
    const result = splitTextIntoRefSegments(input);
    // Whole thing stays literal, and the text is preserved exactly.
    expect(result.every((s) => s.kind === 'text')).toBe(true);
    expect(result.map((s) => (s.kind === 'text' ? s.value : '')).join('')).toBe(input);
  });

  it('preserves surrounding text exactly around a mix of valid and invalid tokens', () => {
    const input = `Start [[TODO]] mid [[${UUID}]] end [[]] done`;
    const result = splitTextIntoRefSegments(input);
    expect(result).toEqual([
      { kind: 'text', value: 'Start [[TODO]] mid ' },
      { kind: 'ref', id: UUID },
      { kind: 'text', value: ' end [[]] done' }
    ]);
    // Rejoining text + literal wikilinks reproduces the non-ref portions.
    const rejoined = result
      .map((s) => (s.kind === 'text' ? s.value : `[[${s.id}]]`))
      .join('');
    expect(rejoined).toBe(input);
  });
});
