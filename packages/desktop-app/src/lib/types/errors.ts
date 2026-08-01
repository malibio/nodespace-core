/**
 * Error Types for NodeSpace
 *
 * Structured error handling for Tauri commands and service operations.
 */

/**
 * Structured error from Tauri commands
 *
 * Provides better observability and debugging by including error codes
 * and optional details alongside user-facing messages.
 */
export interface CommandError {
  /** User-facing error message */
  message: string;

  /** Machine-readable error code (e.g., "NODE_SERVICE_ERROR", "INVALID_NODE_TYPE") */
  code?: string;

  /** Optional detailed error information for debugging */
  details?: string;
}

/**
 * Type guard to check if an error is a CommandError
 */
export function isCommandError(error: unknown): error is CommandError {
  if (typeof error !== 'object' || error === null) return false;

  const err = error as Record<string, unknown>;
  return typeof err.message === 'string';
}

/**
 * Convert unknown error to user-friendly Error instance
 */
export function toError(error: unknown): Error {
  if (error instanceof Error) {
    return error;
  }

  if (isCommandError(error)) {
    return new Error(error.message);
  }

  if (typeof error === 'string') {
    return new Error(error);
  }

  return new Error('Unknown error occurred');
}

/**
 * Database initialization errors
 */
export class DatabaseInitializationError extends Error {
  constructor(
    message: string,
    public readonly details?: string
  ) {
    super(message);
    this.name = 'DatabaseInitializationError';
  }
}

/**
 * Node operation errors
 */
export class NodeOperationError extends Error {
  constructor(
    message: string,
    public readonly nodeId?: string,
    public readonly operation?: string
  ) {
    super(message);
    this.name = 'NodeOperationError';
  }
}

/**
 * Structured conflict payload carried by VERSION_CONFLICT CommandErrors.
 * Mirrors the JSON emitted by the daemon's x-version-conflict metadata header.
 */
export interface VersionConflictData {
  /** Node ID that had the conflict */
  node_id: string;

  /** Version the client expected */
  expected: number;

  /** Actual current version in database */
  actual: number;

  /** Full current node state from database for client-side hydration */
  current_node: import('./node').Node | null;
}

/**
 * A CommandError that carries a structured version-conflict payload.
 * Produced by the daemon → Tauri command → frontend pipeline when two
 * writers race on the same node version.
 */
export interface VersionConflictCommandError extends CommandError {
  code: 'VERSION_CONFLICT';
  conflictData: VersionConflictData;
}

/**
 * Type guard: returns true when the thrown value is a VERSION_CONFLICT
 * CommandError carrying the daemon's structured conflict payload.
 *
 * Matches the gRPC/Tauri shape: { code: "VERSION_CONFLICT", conflictData: {...} }
 */
export function isVersionConflict(
  error: unknown
): error is VersionConflictCommandError {
  if (typeof error !== 'object' || error === null) return false;

  const err = error as Record<string, unknown>;
  if (err.code !== 'VERSION_CONFLICT') return false;
  if (typeof err.conflictData !== 'object' || err.conflictData === null) return false;

  const cd = err.conflictData as Record<string, unknown>;
  return typeof cd.node_id === 'string';
}

/**
 * Structured refusal payload carried by INACCESSIBLE_DESCENDANTS CommandErrors.
 * Mirrors the JSON emitted by the daemon's x-inaccessible-descendants metadata header.
 *
 * `inaccessibleCount` is the ONLY disclosure — no ids, names, or types of the
 * inaccessible nodes. This is deliberate (ADR-041): the actor may not learn anything
 * about content they can't read beyond "N items exist that you don't have access to".
 */
export interface InaccessibleDescendantsData {
  inaccessible_count: number;
}

/**
 * A CommandError produced when a cascade delete is refused because the subtree
 * contains nodes the actor cannot read (ADR-041 "CASCADE requires read access
 * across the whole subtree"). No node was deleted.
 */
export interface InaccessibleDescendantsCommandError extends CommandError {
  code: 'INACCESSIBLE_DESCENDANTS';
  conflictData: InaccessibleDescendantsData;
}

/**
 * Type guard: returns true when the thrown value is an INACCESSIBLE_DESCENDANTS
 * CommandError carrying the daemon's structured refusal payload.
 *
 * Matches the gRPC/Tauri shape: { code: "INACCESSIBLE_DESCENDANTS", conflictData: {...} }
 */
export function isInaccessibleDescendants(
  error: unknown
): error is InaccessibleDescendantsCommandError {
  if (typeof error !== 'object' || error === null) return false;

  const err = error as Record<string, unknown>;
  if (err.code !== 'INACCESSIBLE_DESCENDANTS') return false;
  if (typeof err.conflictData !== 'object' || err.conflictData === null) return false;

  const cd = err.conflictData as Record<string, unknown>;
  return typeof cd.inaccessible_count === 'number';
}
