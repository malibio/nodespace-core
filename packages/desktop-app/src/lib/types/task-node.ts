/**
 * Type-Safe Task Node Wrapper
 *
 * Provides ergonomic, type-safe access to task node properties
 * while maintaining the universal Node storage model.
 *
 * @example
 * ```typescript
 * import { Node } from '$lib/types/node';
 * import { TaskNode, isTaskNode, getTaskStatus, setTaskStatus } from '$lib/types/task-node';
 *
 * // Type guard
 * if (isTaskNode(node)) {
 *   const status = getTaskStatus(node); // Type-safe access
 *   console.log(`Task is ${status}`);
 * }
 *
 * // Immutable update
 * const updated = setTaskStatus(node, 'in_progress');
 * ```
 */

import type { Node } from './node';

/**
 * Task status values (matches schema in init-schemas-via-proxy.ts)
 */
export type TaskStatus = 'open' | 'in_progress' | 'done' | 'cancelled';

/**
 * Task priority values
 */
export type TaskPriority = 'low' | 'medium' | 'high';

/**
 * Task-specific properties stored under properties.task
 */
export interface TaskProperties {
  status?: TaskStatus;
  priority?: TaskPriority;
  dueDate?: string;
  startedAt?: string;
  completedAt?: string;
  assignee?: string;
}

/**
 * Task node interface extending base Node
 *
 * Represents a task with status tracking and metadata.
 */
export interface TaskNode extends Node {
  nodeType: 'task';
  properties: {
    task?: TaskProperties;
    [key: string]: unknown;
  };
}

/**
 * Type guard to check if a node is a task node
 *
 * @param node - Node to check
 * @returns True if node is a task node
 *
 * @example
 * ```typescript
 * if (isTaskNode(node)) {
 *   // TypeScript knows node is TaskNode here
 *   const status = getTaskStatus(node);
 * }
 * ```
 */
export function isTaskNode(node: Node): node is TaskNode {
  return node.nodeType === 'task';
}

/**
 * Get the task status
 *
 * @param node - Task node
 * @returns Task status (defaults to "open")
 *
 * @example
 * ```typescript
 * const status = getTaskStatus(taskNode);
 * console.log(status); // "open", "in_progress", "done", "cancelled"
 * ```
 */
export function getTaskStatus(node: TaskNode): TaskStatus {
  return node.properties.task?.status ?? 'open';
}

/**
 * Set the task status (immutable)
 *
 * Returns a new node with the updated status property.
 * Original node is not modified.
 *
 * @param node - Task node
 * @param status - New status value
 * @returns New node with updated status
 *
 * @example
 * ```typescript
 * const updated = setTaskStatus(taskNode, 'in_progress');
 * // original node unchanged, updated has new status
 * ```
 */
export function setTaskStatus(node: TaskNode, status: TaskStatus): TaskNode {
  return {
    ...node,
    properties: {
      ...node.properties,
      task: {
        ...node.properties.task,
        status
      }
    }
  };
}

/**
 * Get the task priority
 *
 * @param node - Task node
 * @returns Task priority or undefined if not set
 */
export function getTaskPriority(node: TaskNode): TaskPriority | undefined {
  return node.properties.task?.priority;
}

/**
 * Set the task priority (immutable)
 *
 * @param node - Task node
 * @param priority - New priority value
 * @returns New node with updated priority
 */
export function setTaskPriority(node: TaskNode, priority: TaskPriority): TaskNode {
  return {
    ...node,
    properties: {
      ...node.properties,
      task: {
        ...node.properties.task,
        priority
      }
    }
  };
}

/**
 * Get the task due date
 *
 * @param node - Task node
 * @returns Due date string or undefined if not set
 */
export function getTaskDueDate(node: TaskNode): string | undefined {
  return node.properties.task?.dueDate;
}

/**
 * Set the task due date (immutable)
 *
 * @param node - Task node
 * @param dueDate - Due date string (ISO 8601)
 * @returns New node with updated due date
 */
export function setTaskDueDate(node: TaskNode, dueDate: string | undefined): TaskNode {
  return {
    ...node,
    properties: {
      ...node.properties,
      task: {
        ...node.properties.task,
        dueDate
      }
    }
  };
}

/**
 * Helper namespace for task node operations
 */
export const TaskNodeHelpers = {
  isTaskNode,
  getTaskStatus,
  setTaskStatus,
  getTaskPriority,
  setTaskPriority,
  getTaskDueDate,
  setTaskDueDate,

  /**
   * Check if task is completed (done or cancelled)
   */
  isCompleted(node: TaskNode): boolean {
    const status = getTaskStatus(node);
    return status === 'done' || status === 'cancelled';
  },

  /**
   * Check if task is active (in_progress)
   */
  isActive(node: TaskNode): boolean {
    return getTaskStatus(node) === 'in_progress';
  },

  /**
   * Check if task is pending (open)
   */
  isPending(node: TaskNode): boolean {
    return getTaskStatus(node) === 'open';
  },

  /**
   * Get display-friendly status name
   */
  getStatusDisplayName(status: TaskStatus): string {
    const displayNames: Record<TaskStatus, string> = {
      open: 'Open',
      in_progress: 'In Progress',
      done: 'Done',
      cancelled: 'Cancelled'
    };
    return displayNames[status];
  },

  /**
   * Get display-friendly priority name
   */
  getPriorityDisplayName(priority: TaskPriority): string {
    const displayNames: Record<TaskPriority, string> = {
      low: 'Low',
      medium: 'Medium',
      high: 'High'
    };
    return displayNames[priority];
  }
};
