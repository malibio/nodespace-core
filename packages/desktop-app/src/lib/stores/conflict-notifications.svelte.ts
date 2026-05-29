import { createLogger } from '$lib/utils/logger';

const log = createLogger('ConflictNotifications');

export interface ConflictNotification {
  id: string;
  nodeId: string;
  message: string;
  conflictType: 'concurrent-edit' | 'version-mismatch' | 'deleted-node';
  createdAt: number;
}

class ConflictNotificationStore {
  notifications = $state<ConflictNotification[]>([]);

  add(notification: Omit<ConflictNotification, 'id' | 'createdAt'>): string {
    const id = `conflict-${Date.now()}-${Math.random().toString(36).slice(2, 7)}`;
    const entry: ConflictNotification = { ...notification, id, createdAt: Date.now() };
    this.notifications = [...this.notifications, entry];
    log.debug('Conflict notification added', { id, nodeId: notification.nodeId });
    return id;
  }

  dismiss(id: string): void {
    this.notifications = this.notifications.filter((n) => n.id !== id);
  }

  dismissAll(): void {
    this.notifications = [];
  }
}

export const conflictNotifications = new ConflictNotificationStore();
