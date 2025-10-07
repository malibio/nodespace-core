/**
 * Global database write queue to prevent SQLite "database is locked" errors
 *
 * SQLite only allows one write transaction at a time. This queue serializes
 * all database write operations to prevent concurrent access errors.
 *
 * Usage:
 * ```typescript
 * import { queueDatabaseWrite } from '$lib/utils/databaseWriteQueue';
 *
 * await queueDatabaseWrite(() => databaseService.updateNode(nodeId, updates));
 * ```
 */

let pendingDatabaseWritePromise: Promise<void> | null = null;
let queueDepth = 0;

/**
 * Queue a database write operation to ensure it doesn't overlap with other writes
 * This prevents SQLite "database is locked" errors by serializing all writes
 *
 * @param operation - The async operation to execute (should return a Promise)
 * @returns The result of the operation
 */
export async function queueDatabaseWrite<T>(operation: () => Promise<T>): Promise<T> {
  const previousPromise = pendingDatabaseWritePromise;

  // Track queue depth for monitoring
  queueDepth++;
  if (queueDepth > 10) {
    console.warn('[DatabaseWriteQueue] High queue depth detected:', queueDepth);
  }

  // Create a new promise that waits for the previous operation before running ours
  let resolveOurPromise: (() => void) | undefined;
  pendingDatabaseWritePromise = new Promise<void>((resolve) => {
    resolveOurPromise = resolve;
  });

  // Wait for previous operation to complete
  if (previousPromise) {
    try {
      await previousPromise;
    } catch {
      // Ignore errors from previous operations
    }
  }

  // Now execute our operation
  try {
    const result = await operation();
    if (resolveOurPromise) {
      resolveOurPromise();
    }
    queueDepth--;
    return result;
  } catch (error) {
    if (resolveOurPromise) {
      resolveOurPromise();
    }
    queueDepth--;
    throw error;
  }
}

/**
 * Get current queue depth for monitoring/debugging
 * @returns Current number of operations waiting in queue
 */
export function getQueueDepth(): number {
  return queueDepth;
}
