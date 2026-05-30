// services/hierarchy-sync.ts

import type { ReactiveStructureTree } from '$lib/stores/reactive-structure-tree.svelte';
import { createLogger } from '$lib/utils/logger';

const log = createLogger('HierarchySync');

export interface HasChildPayload {
  parentId: string;
  childId: string;
  order?: unknown; // typed unknown, runtime-checked
}

/**
 * Apply a has_child relationship:created event to the structureTree.
 * Order-fallback contract (single authoritative implementation):
 *  - If incoming order is a real number (typeof === 'number'), use it directly (0/negative count).
 *  - Else if child already exists under this parent, preserve existing order.
 *  - Else append at tail: lastSibling.order + 1 + tiny jitter (Math.random() * 0.001).
 * Date.now() is NEVER used as a fallback.
 */
export function applyHasChildCreated(
  structureTree: ReactiveStructureTree,
  payload: HasChildPayload
): void {
  const { parentId, childId } = payload;
  const incomingOrder = typeof payload.order === 'number' ? payload.order : undefined;

  const siblings = structureTree.getChildrenWithOrder(parentId);
  let order: number;

  if (incomingOrder !== undefined) {
    order = incomingOrder;
  } else {
    const existing = siblings.find((c) => c.nodeId === childId);
    if (existing) {
      order = existing.order;
    } else {
      const lastOrder = siblings[siblings.length - 1]?.order ?? 0;
      order = lastOrder + 1 + Math.random() * 0.001;
    }
  }

  structureTree.addChild({ parentId, childId, order });
}

/**
 * Apply a has_child relationship:updated event to the structureTree.
 * Logs a warning if order is missing (should always be present for updated events).
 */
export function applyHasChildUpdated(
  structureTree: ReactiveStructureTree,
  payload: HasChildPayload
): void {
  const { parentId, childId } = payload;
  const incomingOrder = typeof payload.order === 'number' ? payload.order : undefined;
  if (incomingOrder === undefined) {
    log.warn('relationship:updated missing order for has_child', { parentId, childId });
    return;
  }
  structureTree.updateChildOrder(parentId, childId, incomingOrder);
}

/**
 * Apply a has_child relationship:deleted event to the structureTree.
 */
export function applyHasChildDeleted(
  structureTree: ReactiveStructureTree,
  payload: { parentId: string; childId: string }
): void {
  structureTree.removeChild({ parentId: payload.parentId, childId: payload.childId, order: 0 });
}
