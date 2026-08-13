/**
 * SimplePersistenceCoordinator settles a superseded write's promise on every
 * supersede path, not just clearQueued's OCC-conflict path.
 *
 * A caller that awaits a persist() handle must see it settle exactly once —
 * with an OperationCancelledError when a newer write for the same node
 * supersedes it — whether it was cancelled while still on its debounce
 * timer or dropped from the single-slot queue behind an in-flight write.
 *
 * Exercises SimplePersistenceCoordinator directly (exported for this
 * purpose) rather than through SharedNodeStore's full update pipeline, so
 * these tests are precise about which write's promise settles and how.
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import {
  SimplePersistenceCoordinator,
  OperationCancelledError
} from '../../lib/services/shared-node-store.svelte';

describe('SimplePersistenceCoordinator - promise settlement on supersede', () => {
  let coordinator: SimplePersistenceCoordinator;

  beforeEach(() => {
    SimplePersistenceCoordinator.resetInstance();
    coordinator = SimplePersistenceCoordinator.getInstance();
  });

  afterEach(() => {
    SimplePersistenceCoordinator.resetInstance();
  });

  it('rejects a debounced write with OperationCancelledError when a newer write for the same node supersedes it before its debounce timer fires', async () => {
    const nodeId = 'node-cancel-1';
    let opACalls = 0;
    let opBCalls = 0;
    const opA = async () => {
      opACalls++;
    };
    const opB = async () => {
      opBCalls++;
    };

    const handleA = coordinator.persist(nodeId, opA, { mode: 'debounce' });

    // Supersede immediately - well before A's debounce timer fires - via
    // cancelPending(), the same path startBatch() and a fresh persist() call
    // for an idle node both go through.
    const handleB = coordinator.persist(nodeId, opB, { mode: 'debounce' });

    await expect(handleA.promise).rejects.toBeInstanceOf(OperationCancelledError);
    expect(opACalls).toBe(0);

    // B is unaffected by A's cancellation and still completes once its own
    // debounce timer elapses.
    await handleB.promise;
    expect(opBCalls).toBe(1);
  }, 2000);

  it('rejects the superseded queued write with OperationCancelledError when a second write queues up behind an in-flight write, keeping only the latest of the two', async () => {
    const nodeId = 'node-queue-1';

    let resolveOp1: () => void = () => {};
    let op1Calls = 0;
    let op2Calls = 0;
    let op3Calls = 0;
    const op1 = () =>
      new Promise<void>((resolve) => {
        op1Calls++;
        resolveOp1 = resolve;
      });
    const op2 = async () => {
      op2Calls++;
    };
    const op3 = async () => {
      op3Calls++;
    };

    // op1 starts executing immediately and stays in flight until resolved
    // below, so both op2 and op3 arrive while a write is in-flight.
    const handle1 = coordinator.persist(nodeId, op1, { mode: 'immediate' });
    expect(coordinator.isExecuting(nodeId)).toBe(true);

    // op2 queues behind the in-flight op1 (single-slot queue).
    const handle2 = coordinator.persist(nodeId, op2, { mode: 'debounce' });

    // op3 arrives before op2 has run, superseding it in that same slot.
    const handle3 = coordinator.persist(nodeId, op3, { mode: 'debounce' });

    // The superseded queue entry (op2) must settle, not hang forever - this
    // is exactly what previously leaked one unsettled promise per keystroke
    // during an in-flight write.
    await expect(handle2.promise).rejects.toBeInstanceOf(OperationCancelledError);
    expect(op2Calls).toBe(0);

    // Let op1 finish; the surviving queued write (op3) takes over and runs.
    resolveOp1();
    await handle1.promise;
    await handle3.promise;

    expect(op1Calls).toBe(1);
    expect(op3Calls).toBe(1);
  }, 5000);

  it('does not settle an in-flight write as cancelled when cancelPending() races it, so the real outcome still lands', async () => {
    const nodeId = 'node-inflight-cancel-1';

    let rejectOp1: (error: Error) => void = () => {};
    let op1Calls = 0;
    const realFailure = new Error('backend write actually failed');
    const op1 = () =>
      new Promise<void>((_resolve, reject) => {
        op1Calls++;
        rejectOp1 = reject;
      });

    // op1 starts executing immediately (mirrors persistBatchedChanges'
    // immediate-mode batch commit) and stays in flight until rejected below.
    const handle1 = coordinator.persist(nodeId, op1, { mode: 'immediate' });
    expect(coordinator.isExecuting(nodeId)).toBe(true);

    // startBatch() calls cancelPending() unconditionally, with no isExecuting
    // guard, while this write is still in flight. It must NOT settle
    // handle1.promise as cancelled: op1 is still genuinely running, and a
    // premature "cancelled" settlement here would mask op1's real outcome
    // (a later resolve/reject on an already-settled promise is a silent
    // no-op) — turning a real write failure into a silently swallowed one.
    coordinator.cancelPending(nodeId);

    let settled = false;
    void handle1.promise.catch(() => {}).finally(() => {
      settled = true;
    });
    await Promise.resolve();
    await Promise.resolve();
    expect(settled).toBe(false);

    // The real outcome (here, a genuine failure) still reaches the caller
    // once op1 actually completes — not masked as OperationCancelledError.
    rejectOp1(realFailure);
    await expect(handle1.promise).rejects.toBe(realFailure);
    expect(op1Calls).toBe(1);
  }, 2000);
});
