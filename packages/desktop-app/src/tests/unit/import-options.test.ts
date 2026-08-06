import { describe, it, expect } from 'vitest';
import {
  defaultImportModalState,
  importOptionsFromModal,
  type ImportModalState,
} from '$lib/services/import-options';

const FOLDER = '/Users/example/notes';

/** All-checked (default) state, seeded with a chosen folder. */
function checkedState(overrides: Partial<ImportModalState> = {}): ImportModalState {
  return { ...defaultImportModalState(), folderPath: FOLDER, ...overrides };
}

describe('defaultImportModalState', () => {
  it('pre-checks all four options and starts with no folder', () => {
    expect(defaultImportModalState()).toEqual({
      folderPath: '',
      excludeAgentFiles: true,
      skipHidden: true,
      includeSubfolders: true,
      mirrorCollections: true,
    });
  });
});

describe('importOptionsFromModal', () => {
  it('maps the all-checked default to the correct opt-out wire values', () => {
    // Every filter checkbox is ON ⇒ inverted opt-out flags are all false;
    // auto_collection_routing is a positive flag ⇒ true.
    expect(importOptionsFromModal(checkedState())).toEqual({
      base_directory: FOLDER,
      include_agent_files: false,
      include_hidden: false,
      no_recursion: false,
      auto_collection_routing: true,
    });
  });

  it('maps the all-unchecked state to the inverse', () => {
    const state = checkedState({
      excludeAgentFiles: false,
      skipHidden: false,
      includeSubfolders: false,
      mirrorCollections: false,
    });
    expect(importOptionsFromModal(state)).toEqual({
      base_directory: FOLDER,
      include_agent_files: true,
      include_hidden: true,
      no_recursion: true,
      auto_collection_routing: false,
    });
  });

  it('always carries the chosen folder into base_directory', () => {
    const other = '/tmp/somewhere/else';
    expect(importOptionsFromModal(checkedState({ folderPath: other })).base_directory).toBe(other);
  });

  it('inverts excludeAgentFiles independently (unchecking it includes agent files)', () => {
    const opts = importOptionsFromModal(checkedState({ excludeAgentFiles: false }));
    expect(opts.include_agent_files).toBe(true);
    // Other flags stay at their checked-defaults.
    expect(opts.include_hidden).toBe(false);
    expect(opts.no_recursion).toBe(false);
    expect(opts.auto_collection_routing).toBe(true);
  });

  it('inverts skipHidden independently (unchecking it includes hidden entries)', () => {
    const opts = importOptionsFromModal(checkedState({ skipHidden: false }));
    expect(opts.include_hidden).toBe(true);
    expect(opts.include_agent_files).toBe(false);
    expect(opts.no_recursion).toBe(false);
    expect(opts.auto_collection_routing).toBe(true);
  });

  it('inverts includeSubfolders independently (unchecking it disables recursion)', () => {
    const opts = importOptionsFromModal(checkedState({ includeSubfolders: false }));
    expect(opts.no_recursion).toBe(true);
    expect(opts.include_agent_files).toBe(false);
    expect(opts.include_hidden).toBe(false);
    expect(opts.auto_collection_routing).toBe(true);
  });

  it('does NOT invert mirrorCollections (positive flag)', () => {
    const opts = importOptionsFromModal(checkedState({ mirrorCollections: false }));
    expect(opts.auto_collection_routing).toBe(false);
    // Unrelated flags unchanged.
    expect(opts.include_agent_files).toBe(false);
    expect(opts.include_hidden).toBe(false);
    expect(opts.no_recursion).toBe(false);
  });
});
