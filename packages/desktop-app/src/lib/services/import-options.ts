/**
 * Import options mapping
 *
 * Pure, DOM-free translation from the import modal's four user-facing
 * checkboxes into the wire `ImportOptions` sent to the Rust Tauri command.
 *
 * The tricky part is the OPT-OUT inversion: three of the four checkboxes are
 * phrased as filters the user turns ON (default checked), but the backend
 * fields are opt-*in* booleans that default to `false` = filter active. So a
 * checked filter must be sent as its inverse (`false`). Only
 * `auto_collection_routing` is a positive flag (checked ⇒ `true`).
 *
 * All fields are emitted explicitly so behaviour never depends on backend
 * defaults for a partially-populated options object.
 */

import type { ImportOptions } from './import-service';

/**
 * User-facing modal state. Every flag maps to how the checkbox is drawn:
 * `true` = checked.
 */
export interface ImportModalState {
  /** Absolute path of the folder chosen via the native picker. */
  folderPath: string;
  /** "Exclude agent/design files (CLAUDE.md, AGENTS.md, DESIGN.md)" (default checked). */
  excludeAgentFiles: boolean;
  /** "Skip hidden files/folders" (default checked). */
  skipHidden: boolean;
  /** "Include sub-folders" (default checked). */
  includeSubfolders: boolean;
  /** "Create collections mirroring sub-folders" (default checked). */
  mirrorCollections: boolean;
}

/**
 * Default modal state: all four options pre-checked, no folder chosen yet.
 */
export function defaultImportModalState(): ImportModalState {
  return {
    folderPath: '',
    excludeAgentFiles: true,
    skipHidden: true,
    includeSubfolders: true,
    mirrorCollections: true,
  };
}

/**
 * Map the modal's checkbox state to the wire `ImportOptions`.
 *
 * Inversions (checked filter ⇒ send `false`):
 *  - excludeAgentFiles  → include_agent_files = !excludeAgentFiles
 *  - skipHidden         → include_hidden       = !skipHidden
 *  - includeSubfolders  → no_recursion         = !includeSubfolders
 *
 * Positive (not inverted):
 *  - mirrorCollections  → auto_collection_routing = mirrorCollections
 *
 * `base_directory` is set to the chosen folder so auto-routing can compute
 * each file's collection path relative to it.
 */
export function importOptionsFromModal(state: ImportModalState): ImportOptions {
  return {
    base_directory: state.folderPath,
    include_agent_files: !state.excludeAgentFiles,
    include_hidden: !state.skipHidden,
    no_recursion: !state.includeSubfolders,
    auto_collection_routing: state.mirrorCollections,
  };
}
