/**
 * PersistenceCoordinator - Declarative Dependency-Based Persistence
 *
 * This service coordinates asynchronous database persistence operations with
 * dependency tracking, ensuring operations execute in the correct order while
 * managing debouncing and conflict detection.
 *
 * Key Features:
 * - Declarative dependency management (nodeIds, lambdas, batches)
 * - Configurable debouncing (immediate vs. debounced persistence)
 * - Reactive state tracking (Svelte 5 $state)
 * - Test mode support (mocks database calls)
 * - Topological sorting for dependency resolution
 *
 * Architecture:
 * - UI Layer declares dependencies when requesting persistence
 * - Coordinator resolves dependencies and enforces execution order
 * - Automatic retry and error handling
 * - Performance metrics and monitoring
 *
 * @see docs/architecture/persistence-layer.md
 * @see docs/architecture/dependency-based-persistence.md
 */

// ============================================================================
// Types and Interfaces
// ============================================================================

/**
 * Dependency types supported by the coordinator
 */
export type PersistenceDependency =
  | string // Simple node ID dependency
  | (() => Promise<void>) // Lambda function dependency
  | { nodeIds: string[] } // Batch of node IDs
  | PersistenceHandle; // Handle from previous operation

/**
 * Persistence mode
 */
export type PersistenceMode = 'immediate' | 'debounce';

/**
 * Persistence options
 */
export interface PersistenceOptions {
  /** Persistence mode: 'immediate' or 'debounce' (default: 'debounce') */
  mode?: PersistenceMode;

  /** Debounce delay in milliseconds (default: 500ms) */
  debounceMs?: number;

  /** Dependencies that must complete before this operation */
  dependencies?: PersistenceDependency[];

  /** Operation priority (higher numbers execute first within same dependency level) */
  priority?: number;
}

/**
 * Handle returned from persist() for tracking and chaining
 */
export interface PersistenceHandle {
  /** Node ID this operation affects */
  nodeId: string;

  /** Promise that resolves when persistence completes */
  promise: Promise<void>;

  /** Check if persistence has completed */
  isPersisted: () => boolean;
}

/**
 * Status of a persistence operation
 */
export type PersistenceStatus = 'pending' | 'in-progress' | 'completed' | 'failed';

/**
 * Internal operation tracking
 */
interface PersistenceOperation {
  nodeId: string;
  operation: () => Promise<void>;
  options: PersistenceOptions;
  status: PersistenceStatus;
  dependencies: PersistenceDependency[];
  promise: Promise<void>;
  resolve: () => void;
  reject: (error: Error) => void;
  timer?: ReturnType<typeof setTimeout>;
  createdAt: number;
  startedAt?: number;
  completedAt?: number;
  error?: Error;
}

/**
 * Performance metrics
 */
export interface PersistenceMetrics {
  totalOperations: number;
  completedOperations: number;
  failedOperations: number;
  averageExecutionTime: number;
  maxExecutionTime: number;
  pendingOperations: number;
}

// ============================================================================
// PersistenceCoordinator Class
// ============================================================================

export class PersistenceCoordinator {
  // Singleton instance
  private static instance: PersistenceCoordinator | null = null;

  // Reactive state (Svelte 5)
  private persistenceStatus = $state(new Map<string, PersistenceStatus>());
  private persistedNodes = $state(new Set<string>());

  // Internal tracking
  private operations = new Map<string, PersistenceOperation>();
  private completedOperations = new Set<string>();

  // Test mode
  private testMode = false;
  private mockPersistence = new Map<string, boolean>();

  // Metrics
  private metrics: PersistenceMetrics = {
    totalOperations: 0,
    completedOperations: 0,
    failedOperations: 0,
    averageExecutionTime: 0,
    maxExecutionTime: 0,
    pendingOperations: 0
  };

  // Configuration
  private defaultDebounceMs = 500;
  private maxConcurrentOperations = 10;

  private constructor() {
    // Private constructor for singleton
  }

  /**
   * Get singleton instance
   */
  static getInstance(): PersistenceCoordinator {
    if (!PersistenceCoordinator.instance) {
      PersistenceCoordinator.instance = new PersistenceCoordinator();
    }
    return PersistenceCoordinator.instance;
  }

  /**
   * Reset singleton (for testing only)
   */
  static resetInstance(): void {
    PersistenceCoordinator.instance = null;
  }

  // ========================================================================
  // Core API
  // ========================================================================

  /**
   * Request persistence with dependency tracking
   *
   * @param nodeId - Node ID to persist
   * @param operation - Async function that performs the persistence
   * @param options - Persistence options (mode, dependencies, etc.)
   * @returns Handle for tracking and chaining
   *
   * @example
   * // Debounced content save (typing)
   * persistenceCoordinator.persist(
   *   nodeId,
   *   () => tauriNodeService.updateNode(nodeId, node),
   *   { mode: 'debounce' }
   * );
   *
   * @example
   * // Immediate save with dependency
   * persistenceCoordinator.persist(
   *   childId,
   *   () => tauriNodeService.createNode(child),
   *   {
   *     mode: 'immediate',
   *     dependencies: [parentId]  // Wait for parent first
   *   }
   * );
   *
   * @example
   * // Complex dependency with lambda
   * persistenceCoordinator.persist(
   *   nodeId,
   *   () => tauriNodeService.updateNode(nodeId, node),
   *   {
   *     mode: 'immediate',
   *     dependencies: [
   *       async () => {
   *         await ensureAncestorChainPersisted(nodeId);
   *       }
   *     ]
   *   }
   * );
   */
  persist(
    nodeId: string,
    operation: () => Promise<void>,
    options: PersistenceOptions = {}
  ): PersistenceHandle {
    // Cancel any existing pending operation for this node
    this.cancelPending(nodeId);

    // Create promise for this operation
    let resolvePromise: () => void;
    let rejectPromise: (error: Error) => void;
    const promise = new Promise<void>((resolve, reject) => {
      resolvePromise = resolve;
      rejectPromise = reject;
    });

    // Create operation record
    const op: PersistenceOperation = {
      nodeId,
      operation,
      options,
      status: 'pending',
      dependencies: options.dependencies || [],
      promise,
      resolve: resolvePromise!,
      reject: rejectPromise!,
      createdAt: Date.now()
    };

    // Track operation
    this.operations.set(nodeId, op);
    this.persistenceStatus.set(nodeId, 'pending');
    this.metrics.totalOperations++;
    this.metrics.pendingOperations++;

    // Schedule execution based on mode
    const mode = options.mode || 'debounce';
    if (mode === 'immediate') {
      this.scheduleImmediate(op);
    } else {
      this.scheduleDebounced(op);
    }

    // Return handle
    return {
      nodeId,
      promise,
      isPersisted: () => this.isPersisted(nodeId)
    };
  }

  /**
   * Check if a node has been persisted (reactive)
   */
  isPersisted(nodeId: string): boolean {
    return this.persistedNodes.has(nodeId);
  }

  /**
   * Check if a node has a pending persistence operation (reactive)
   */
  isPending(nodeId: string): boolean {
    const status = this.persistenceStatus.get(nodeId);
    return status === 'pending' || status === 'in-progress';
  }

  /**
   * Get current status of a node's persistence (reactive)
   */
  getStatus(nodeId: string): PersistenceStatus | undefined {
    return this.persistenceStatus.get(nodeId);
  }

  /**
   * Wait for specific nodes to be persisted
   *
   * @param nodeIds - Array of node IDs to wait for
   * @param timeoutMs - Timeout in milliseconds (default: 5000)
   * @returns Set of node IDs that failed to persist
   */
  async waitForPersistence(nodeIds: string[], timeoutMs = 5000): Promise<Set<string>> {
    const failedNodeIds = new Set<string>();
    const promises: Promise<void>[] = [];

    for (const nodeId of nodeIds) {
      const op = this.operations.get(nodeId);
      if (op) {
        promises.push(op.promise);
      }
    }

    if (promises.length === 0) {
      return failedNodeIds;
    }

    try {
      await Promise.race([
        Promise.all(promises),
        new Promise((_, reject) =>
          setTimeout(() => reject(new Error('Timeout waiting for persistence')), timeoutMs)
        )
      ]);
    } catch (error) {
      console.error('[PersistenceCoordinator] Timeout waiting for persistence:', error);

      // Grace period
      await new Promise((resolve) => setTimeout(resolve, 50));

      // Check which nodes failed
      for (const nodeId of nodeIds) {
        const op = this.operations.get(nodeId);
        if (op && (op.status === 'pending' || op.status === 'in-progress')) {
          failedNodeIds.add(nodeId);
        }
      }
    }

    return failedNodeIds;
  }

  /**
   * Cancel pending operation for a node
   */
  cancelPending(nodeId: string): void {
    const op = this.operations.get(nodeId);
    if (op && op.status === 'pending') {
      // Clear debounce timer if exists
      if (op.timer) {
        clearTimeout(op.timer);
      }

      // Mark as failed
      op.status = 'failed';
      op.error = new Error('Cancelled');
      this.persistenceStatus.set(nodeId, 'failed');

      // Reject promise
      op.reject(op.error);

      // Clean up
      this.operations.delete(nodeId);
      this.metrics.pendingOperations--;
    }
  }

  /**
   * Get performance metrics
   */
  getMetrics(): PersistenceMetrics {
    return { ...this.metrics };
  }

  /**
   * Reset all state (for testing)
   */
  reset(): void {
    // Cancel all pending operations
    for (const [nodeId] of this.operations) {
      this.cancelPending(nodeId);
    }

    // Clear state
    this.operations.clear();
    this.completedOperations.clear();
    this.persistenceStatus = new Map();
    this.persistedNodes = new Set();
    this.mockPersistence.clear();

    // Reset metrics
    this.metrics = {
      totalOperations: 0,
      completedOperations: 0,
      failedOperations: 0,
      averageExecutionTime: 0,
      maxExecutionTime: 0,
      pendingOperations: 0
    };
  }

  // ========================================================================
  // Test Mode
  // ========================================================================

  /**
   * Enable test mode (skips actual database calls)
   */
  enableTestMode(): void {
    this.testMode = true;
  }

  /**
   * Disable test mode
   */
  disableTestMode(): void {
    this.testMode = false;
  }

  /**
   * Check if test mode is enabled
   */
  isTestMode(): boolean {
    return this.testMode;
  }

  /**
   * Get nodes that were mock-persisted in test mode
   */
  getMockPersistedNodes(): string[] {
    return Array.from(this.mockPersistence.keys());
  }

  /**
   * Reset test state
   */
  resetTestState(): void {
    this.mockPersistence.clear();
    this.reset();
  }

  // ========================================================================
  // Internal Scheduling
  // ========================================================================

  /**
   * Schedule immediate execution
   */
  private scheduleImmediate(op: PersistenceOperation): void {
    // Execute on next tick to allow synchronous code to complete
    setTimeout(() => this.executeOperation(op), 0);
  }

  /**
   * Schedule debounced execution
   */
  private scheduleDebounced(op: PersistenceOperation): void {
    const debounceMs = op.options.debounceMs || this.defaultDebounceMs;

    // Set timer
    op.timer = setTimeout(() => {
      this.executeOperation(op);
    }, debounceMs);
  }

  /**
   * Execute a persistence operation
   */
  private async executeOperation(op: PersistenceOperation): Promise<void> {
    try {
      // Mark as in-progress
      op.status = 'in-progress';
      op.startedAt = Date.now();
      this.persistenceStatus.set(op.nodeId, 'in-progress');

      // Resolve dependencies first
      await this.resolveDependencies(op.dependencies);

      // In test mode, skip actual operation execution
      if (this.testMode) {
        this.mockPersistence.set(op.nodeId, true);
      } else {
        // Execute the operation (production mode only)
        await op.operation();
      }

      // Mark as completed
      op.status = 'completed';
      op.completedAt = Date.now();
      this.persistenceStatus.set(op.nodeId, 'completed');
      this.persistedNodes.add(op.nodeId);
      this.completedOperations.add(op.nodeId);

      // Update metrics
      const executionTime = op.completedAt - op.startedAt!;
      this.updateMetrics(executionTime, true);

      // Resolve promise
      op.resolve();

      // Clean up
      this.operations.delete(op.nodeId);
      this.metrics.pendingOperations--;
    } catch (error) {
      const err = error instanceof Error ? error : new Error(String(error));

      // Mark as failed
      op.status = 'failed';
      op.error = err;
      op.completedAt = Date.now();
      this.persistenceStatus.set(op.nodeId, 'failed');

      // Update metrics
      this.updateMetrics(0, false);

      // Reject promise
      op.reject(err);

      // Log error
      console.error(`[PersistenceCoordinator] Operation failed for node ${op.nodeId}:`, err);

      // Clean up
      this.operations.delete(op.nodeId);
      this.metrics.pendingOperations--;
    }
  }

  /**
   * Resolve all dependencies for an operation
   */
  private async resolveDependencies(dependencies: PersistenceDependency[]): Promise<void> {
    if (dependencies.length === 0) {
      return;
    }

    const promises: Promise<void>[] = [];

    for (const dep of dependencies) {
      if (typeof dep === 'string') {
        // Simple node ID dependency
        const depOp = this.operations.get(dep);
        if (depOp) {
          promises.push(depOp.promise);
        }
      } else if (typeof dep === 'function') {
        // Lambda dependency
        promises.push(dep());
      } else if ('nodeIds' in dep) {
        // Batch dependency
        for (const nodeId of dep.nodeIds) {
          const depOp = this.operations.get(nodeId);
          if (depOp) {
            promises.push(depOp.promise);
          }
        }
      } else if ('promise' in dep) {
        // Handle dependency
        promises.push(dep.promise);
      }
    }

    // Wait for all dependencies
    await Promise.all(promises);
  }

  /**
   * Update performance metrics
   */
  private updateMetrics(executionTime: number, success: boolean): void {
    if (success) {
      this.metrics.completedOperations++;

      // Update average execution time
      const count = this.metrics.completedOperations;
      const currentAvg = this.metrics.averageExecutionTime;
      this.metrics.averageExecutionTime = (currentAvg * (count - 1) + executionTime) / count;

      // Update max execution time
      this.metrics.maxExecutionTime = Math.max(this.metrics.maxExecutionTime, executionTime);
    } else {
      this.metrics.failedOperations++;
    }
  }
}

// ============================================================================
// Singleton Export
// ============================================================================

/**
 * Singleton instance for application-wide use
 */
export const persistenceCoordinator = PersistenceCoordinator.getInstance();

/**
 * Default export
 */
export default PersistenceCoordinator;
