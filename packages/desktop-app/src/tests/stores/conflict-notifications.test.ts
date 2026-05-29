import { describe, it, expect, beforeEach } from 'vitest';
import { conflictNotifications } from '../../lib/stores/conflict-notifications.svelte';

describe('conflictNotifications', () => {
  beforeEach(() => {
    conflictNotifications.dismissAll();
  });

  it('adds a notification and assigns a unique id', () => {
    const id = conflictNotifications.add({
      nodeId: 'node-1',
      message: 'Your edit was overwritten by another pane',
      conflictType: 'concurrent-edit'
    });

    expect(id).toBeTruthy();
    expect(conflictNotifications.notifications).toHaveLength(1);
    expect(conflictNotifications.notifications[0].nodeId).toBe('node-1');
    expect(conflictNotifications.notifications[0].id).toBe(id);
  });

  it('assigns unique ids for concurrent additions', () => {
    const id1 = conflictNotifications.add({
      nodeId: 'node-1',
      message: 'msg',
      conflictType: 'concurrent-edit'
    });
    const id2 = conflictNotifications.add({
      nodeId: 'node-2',
      message: 'msg',
      conflictType: 'version-mismatch'
    });

    expect(id1).not.toBe(id2);
    expect(conflictNotifications.notifications).toHaveLength(2);
  });

  it('dismisses a specific notification by id', () => {
    const id1 = conflictNotifications.add({
      nodeId: 'node-1',
      message: 'msg',
      conflictType: 'concurrent-edit'
    });
    conflictNotifications.add({
      nodeId: 'node-2',
      message: 'msg',
      conflictType: 'concurrent-edit'
    });

    conflictNotifications.dismiss(id1);

    expect(conflictNotifications.notifications).toHaveLength(1);
    expect(conflictNotifications.notifications[0].nodeId).toBe('node-2');
  });

  it('dismissAll clears all notifications', () => {
    conflictNotifications.add({ nodeId: 'a', message: 'msg', conflictType: 'concurrent-edit' });
    conflictNotifications.add({ nodeId: 'b', message: 'msg', conflictType: 'concurrent-edit' });

    conflictNotifications.dismissAll();

    expect(conflictNotifications.notifications).toHaveLength(0);
  });

  it('records createdAt timestamp', () => {
    const before = Date.now();
    conflictNotifications.add({
      nodeId: 'node-1',
      message: 'msg',
      conflictType: 'concurrent-edit'
    });
    const after = Date.now();

    const n = conflictNotifications.notifications[0];
    expect(n.createdAt).toBeGreaterThanOrEqual(before);
    expect(n.createdAt).toBeLessThanOrEqual(after);
  });

  it('ignores dismiss for unknown id', () => {
    conflictNotifications.add({
      nodeId: 'node-1',
      message: 'msg',
      conflictType: 'concurrent-edit'
    });

    // Should not throw
    conflictNotifications.dismiss('nonexistent-id');

    expect(conflictNotifications.notifications).toHaveLength(1);
  });
});
