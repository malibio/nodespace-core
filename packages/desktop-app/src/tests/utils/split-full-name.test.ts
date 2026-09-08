import { describe, it, expect } from 'vitest';
import { splitFullName } from '$lib/utils/split-full-name';

describe('splitFullName', () => {
  it('splits a simple two-part name on the first space', () => {
    expect(splitFullName('Alice Example')).toEqual({ firstName: 'Alice', lastName: 'Example' });
  });

  it('keeps a middle name/suffix together as the last name', () => {
    expect(splitFullName('Jane Q. Doe')).toEqual({ firstName: 'Jane', lastName: 'Q. Doe' });
  });

  it('a single-word name yields an empty last name rather than guessing', () => {
    expect(splitFullName('Cher')).toEqual({ firstName: 'Cher', lastName: '' });
  });

  it('an empty string yields both fields empty', () => {
    expect(splitFullName('')).toEqual({ firstName: '', lastName: '' });
  });

  it('trims surrounding whitespace before splitting', () => {
    expect(splitFullName('  Alice   Example  ')).toEqual({
      firstName: 'Alice',
      lastName: 'Example'
    });
  });
});
