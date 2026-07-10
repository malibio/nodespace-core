/**
 * True when running under Vitest. Single source of truth for the VITEST-env
 * check shared by logger.ts and debug-channel.ts (both need to no-op their
 * Tauri `invoke` calls under test) — kept as its own module so neither of
 * those two files (which import from each other) needs to depend on the
 * other for this check.
 */
export const isVitest =
  (typeof import.meta !== 'undefined' && import.meta.env?.VITEST === 'true') ||
  (typeof process !== 'undefined' && process.env?.VITEST === 'true');
