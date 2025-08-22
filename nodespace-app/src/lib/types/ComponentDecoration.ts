/**
 * Component-Based Node Reference Decoration Types
 * 
 * Defines the interfaces for component-based node reference decorations
 * that replace HTML string generation with Svelte component rendering.
 */

import type { SvelteComponent } from 'svelte';

/**
 * Component decoration result containing Svelte component and props
 */
export interface ComponentDecoration {
  /** Svelte component class to render */
  component: any;
  
  /** Props to pass to the component (flexible JSON-like object) */
  props: Record<string, any>;
  
  /** Optional event handlers */
  events?: Record<string, Function>;
  
  /** Optional metadata */
  metadata?: Record<string, any>;
}

/**
 * Base props that all node reference components receive
 */
export interface BaseNodeReferenceProps {
  /** Unique node identifier */
  nodeId: string;
  
  /** Node content/text */
  content: string;
  
  /** Navigation URI for the node */
  href: string;
  
  /** Type of node (task, user, date, etc.) */
  nodeType: string;
  
  /** Additional CSS classes */
  className?: string;
  
  /** Inline styles */
  style?: string;
  
  /** ARIA label for accessibility */
  ariaLabel?: string;
}

/**
 * Task-specific reference props
 */
export interface TaskNodeReferenceProps extends BaseNodeReferenceProps {
  completed: boolean;
  priority: 'low' | 'medium' | 'high' | 'critical';
  status: 'todo' | 'in-progress' | 'blocked' | 'done';
  dueDate?: Date;
  createdAt?: Date;
  completedAt?: Date;
  assignee?: {
    id: string;
    name: string;
    avatar?: string;
  };
  tags?: string[];
  isOverdue?: boolean;
  timeRemaining?: string;
  progressPercentage?: number;
  estimatedHours?: number;
  actualHours?: number;
  onToggleComplete?: () => void;
  onPriorityChange?: (priority: string) => void;
  onAssigneeChange?: (userId: string) => void;
}

/**
 * User-specific reference props
 */
export interface UserNodeReferenceProps extends BaseNodeReferenceProps {
  firstName?: string;
  lastName?: string;
  email?: string;
  username?: string;
  isOnline: boolean;
  lastSeen?: Date;
  status: 'available' | 'busy' | 'away' | 'offline';
  avatar?: string;
  role?: string;
  department?: string;
  title?: string;
  timezone?: string;
  workingHours?: {
    start: string;
    end: string;
    timezone: string;
  };
  contactMethods?: Array<{
    type: 'email' | 'slack' | 'phone' | 'other';
    value: string;
    preferred?: boolean;
  }>;
  displayName?: string;
  initials?: string;
  statusColor?: string;
  canMessage?: boolean;
  canAssignTasks?: boolean;
  onMessageClick?: () => void;
  onProfileClick?: () => void;
  onAssignTask?: (taskId: string) => void;
}

/**
 * Date-specific reference props
 */
export interface DateNodeReferenceProps extends BaseNodeReferenceProps {
  date: Date;
  originalFormat?: string;
  isToday: boolean;
  isPast: boolean;
  isFuture: boolean;
  isWeekend?: boolean;
  isBusinessDay?: boolean;
  relativeTime?: string;
  humanizedDate?: string;
  dayOfWeek?: string;
  weekNumber?: number;
  quarter?: number;
  formats?: {
    short: string;
    medium: string;
    long: string;
    time?: string;
    iso: string;
  };
  hasEvents?: boolean;
  eventCount?: number;
  events?: Array<{
    id: string;
    title: string;
    time: string;
  }>;
  urgencyLevel?: 'low' | 'medium' | 'high';
  themeVariant?: string;
  onDateClick?: () => void;
  onScheduleEvent?: () => void;
  onViewEvents?: () => void;
}

/**
 * Document-specific reference props
 */
export interface DocumentNodeReferenceProps extends BaseNodeReferenceProps {
  fileType: string;
  fileName?: string;
  fileSize?: string;
  preview?: string;
  thumbnailUrl?: string;
  downloadUrl?: string;
  lastModified?: Date;
  author?: string;
  version?: string;
  isShared?: boolean;
  permissions?: {
    canEdit: boolean;
    canDelete: boolean;
    canShare: boolean;
  };
  onDownload?: () => void;
  onPreview?: () => void;
  onShare?: () => void;
  onEdit?: () => void;
}

/**
 * AI Chat-specific reference props
 */
export interface AINodeReferenceProps extends BaseNodeReferenceProps {
  model?: string;
  messageCount?: number;
  lastActivity?: Date;
  conversationSummary?: string;
  participants?: Array<{
    id: string;
    name: string;
    type: 'user' | 'ai';
  }>;
  status?: 'active' | 'completed' | 'archived';
  context?: string;
  onContinueChat?: () => void;
  onArchive?: () => void;
  onExport?: () => void;
}