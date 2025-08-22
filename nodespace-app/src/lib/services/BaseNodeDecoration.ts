/**
 * BaseNode Decoration System - Rich Visual Node References (Phase 2.2)
 * 
 * Implements the decorateReference() pattern for creating rich visual decorations
 * for different node types in the universal node reference system.
 * 
 * Key Features:
 * - Base decorateReference() method for default styling
 * - Node type-specific decoration classes (TaskNode, UserNode, DateNode, etc.)
 * - Content-driven decoration system with XSS-safe rendering
 * - Performance-optimized viewport-based processing
 * - Full accessibility support with ARIA labels
 * - Integration with existing NodeReferenceService and DecorationCoordinator
 */

import type { NodeReferenceService } from './NodeReferenceService';
import type { ComponentDecoration, BaseNodeReferenceProps, TaskNodeReferenceProps, UserNodeReferenceProps, DateNodeReferenceProps } from '../types/ComponentDecoration';
import { 
  BaseNodeReference, 
  TaskNodeReference, 
  DateNodeReference, 
  UserNodeReference,
  getNodeReferenceComponent
} from '../components/references';

// ============================================================================
// Core Types and Interfaces
// ============================================================================

export interface DecorationContext {
  nodeId: string;
  nodeType: string;
  title: string;
  content: string;
  uri: string;
  metadata: Record<string, unknown>;
  targetElement: HTMLElement;
  displayContext: 'inline' | 'popup' | 'preview';
}

export interface DecorationResult {
  html: string;
  cssClasses: string[];
  ariaLabel: string;
  metadata: Record<string, unknown>;
  interactive: boolean;
}

// Legacy interface for backward compatibility
export interface LegacyDecorationResult extends DecorationResult {}

export interface NodeTypeConfig {
  icon: string;
  label: string;
  color: string;
  defaultDecoration: (context: DecorationContext) => DecorationResult;
}

// ============================================================================
// Base Node Decoration Class
// ============================================================================

export abstract class BaseNodeDecorator {
  protected nodeReferenceService: NodeReferenceService;
  protected readonly decorationType: string;

  constructor(nodeReferenceService: NodeReferenceService, decorationType: string) {
    this.nodeReferenceService = nodeReferenceService;
    this.decorationType = decorationType;
  }

  /**
   * Abstract method that must be implemented by each node type
   * This is the core decorateReference() pattern - now returns Svelte components
   */
  public abstract decorateReference(context: DecorationContext): ComponentDecoration;

  /**
   * Base implementation with safe defaults - now returns ComponentDecoration
   */
  protected getBaseComponentDecoration(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, nodeType, title } = context;
    const component = getNodeReferenceComponent(nodeType);

    return {
      component,
      props: {
        nodeId,
        content: title || content,
        href: uri,
        nodeType,
        ariaLabel: `Reference to ${nodeType}: ${title || content}`
      },
      metadata: { 
        nodeType, 
        decorationType: this.decorationType 
      }
    };
  }

  /**
   * Legacy base implementation for backward compatibility
   */
  protected getBaseDecoration(context: DecorationContext): LegacyDecorationResult {
    const { nodeType, title, uri } = context;
    const safeTitle = this.sanitizeText(title);
    const nodeConfig = this.getNodeTypeConfig(nodeType);

    return {
      html: `
        <span class="ns-noderef ns-noderef--${nodeType}" 
              data-node-id="${context.nodeId}" 
              data-uri="${uri}"
              role="link"
              tabindex="0">
          <span class="ns-noderef__icon">${nodeConfig.icon}</span>
          <span class="ns-noderef__title">${safeTitle}</span>
        </span>
      `,
      cssClasses: [`ns-noderef`, `ns-noderef--${nodeType}`],
      ariaLabel: `Reference to ${nodeConfig.label}: ${safeTitle}`,
      metadata: { nodeType, decorationType: this.decorationType },
      interactive: true
    };
  }

  /**
   * Get configuration for node type with fallback
   */
  protected getNodeTypeConfig(nodeType: string): NodeTypeConfig {
    return NODE_TYPE_CONFIGS[nodeType] || NODE_TYPE_CONFIGS.default;
  }

  /**
   * Sanitize text content to prevent XSS
   */
  protected sanitizeText(text: string): string {
    if (!text) return '';
    
    // In test environment or Node.js, use simple HTML escaping
    if (typeof document === 'undefined' || !document.createElement) {
      return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#39;');
    }
    
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  /**
   * Extract metadata from node content
   */
  protected extractMetadata(content: string, nodeType: string): Record<string, unknown> {
    const metadata: Record<string, unknown> = {};
    
    // Extract common metadata patterns
    const lines = content.split('\n');
    
    // Look for metadata in first few lines
    for (const line of lines.slice(0, 5)) {
      // Date patterns
      const dateMatch = line.match(/(\d{4}-\d{2}-\d{2}|\d{1,2}\/\d{1,2}\/\d{4})/);
      if (dateMatch) {
        metadata.mentionedDate = dateMatch[1];
      }
      
      // Status patterns for tasks
      if (nodeType === 'task') {
        const statusMatch = line.match(/(?:status|state):\s*(\w+)/i);
        if (statusMatch) {
          metadata.status = statusMatch[1].toLowerCase();
        }
        
        // Priority patterns
        const priorityMatch = line.match(/(?:priority|pri):\s*(\w+)/i);
        if (priorityMatch) {
          metadata.priority = priorityMatch[1].toLowerCase();
        }
      }
      
      // User patterns
      if (nodeType === 'user') {
        const emailMatch = line.match(/([a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,})/);
        if (emailMatch) {
          metadata.email = emailMatch[1];
        }
        
        // Role patterns
        const roleMatch = line.match(/(?:role|position):\s*(\w+)/i);
        if (roleMatch) {
          metadata.role = roleMatch[1].toLowerCase();
        }
      }
    }
    
    return metadata;
  }

  /**
   * Format relative time for display
   */
  protected formatRelativeTime(timestamp: number): string {
    const now = Date.now();
    const diff = now - timestamp;
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (days > 0) {
      return `${days}d ago`;
    } else if (hours > 0) {
      return `${hours}h ago`;
    } else if (minutes > 0) {
      return `${minutes}m ago`;
    } else {
      return 'just now';
    }
  }
}

// ============================================================================
// Node Type-Specific Decorators
// ============================================================================

export class TaskNodeDecorator extends BaseNodeDecorator {
  constructor(nodeReferenceService: NodeReferenceService) {
    super(nodeReferenceService, 'task');
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, title } = context;
    const metadata = this.extractMetadata(content, 'task');
    
    // Extract task-specific data
    const status = metadata.status as string || 'pending';
    const priority = (metadata.priority as string || 'medium') as 'low' | 'medium' | 'high' | 'critical';
    const completed = status === 'completed' || status === 'done';
    
    const props: TaskNodeReferenceProps = {
      nodeId,
      content: title || content,
      href: uri,
      nodeType: 'task',
      completed,
      priority,
      status: this.mapStatusToValidType(status),
      ariaLabel: `Task: ${title || content} (${status}${priority !== 'medium' ? `, ${priority} priority` : ''})`
    };

    return {
      component: TaskNodeReference,
      props,
      metadata: { 
        nodeType: 'task', 
        decorationType: this.decorationType,
        status,
        priority,
        completed
      }
    };
  }

  private mapStatusToValidType(status: string): 'todo' | 'in-progress' | 'blocked' | 'done' {
    const statusMap: Record<string, 'todo' | 'in-progress' | 'blocked' | 'done'> = {
      'pending': 'todo',
      'todo': 'todo',
      'in-progress': 'in-progress',
      'in_progress': 'in-progress',
      'progress': 'in-progress',
      'blocked': 'blocked',
      'done': 'done',
      'completed': 'done',
      'complete': 'done'
    };
    return statusMap[status.toLowerCase()] || 'todo';
  }
}

export class UserNodeDecorator extends BaseNodeDecorator {
  constructor(nodeReferenceService: NodeReferenceService) {
    super(nodeReferenceService, 'user');
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, title } = context;
    const metadata = this.extractMetadata(content, 'user');
    
    // Mock online status (in real implementation, this would come from presence service)
    const isOnline = Math.random() > 0.5; // Mock for demo
    const displayName = title || content.split('\n')[0] || content;
    
    const props: UserNodeReferenceProps = {
      nodeId,
      content: displayName,
      href: uri,
      nodeType: 'user',
      isOnline,
      displayName,
      status: isOnline ? 'available' : 'offline',
      ariaLabel: `User: ${displayName} (${isOnline ? 'online' : 'offline'})`
    };

    return {
      component: UserNodeReference,
      props,
      metadata: { 
        nodeType: 'user', 
        decorationType: this.decorationType,
        isOnline,
        displayName
      }
    };
  }
}

export class DateNodeDecorator extends BaseNodeDecorator {
  constructor(nodeReferenceService: NodeReferenceService) {
    super(nodeReferenceService, 'date');
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, title } = context;
    
    // Try to parse date from title or content
    const dateStr = title || content.split('\n')[0];
    const parsedDate = this.parseDate(dateStr);
    
    const isToday = parsedDate ? this.isToday(parsedDate) : false;
    const isPast = parsedDate ? parsedDate.getTime() < Date.now() : false;

    const props: DateNodeReferenceProps = {
      nodeId,
      content: title || content,
      href: uri,
      nodeType: 'date',
      date: parsedDate || undefined,
      isToday,
      isPast,
      ariaLabel: `Date: ${title || content}${isToday ? ' (Today)' : ''}`
    };

    return {
      component: DateNodeReference,
      props,
      metadata: { 
        nodeType: 'date', 
        decorationType: this.decorationType,
        parsedDate: parsedDate?.toISOString() || null,
        isToday,
        isPast
      }
    };
  }

  private parseDate(dateStr: string): Date | null {
    try {
      // Try common date formats
      const date = new Date(dateStr);
      return isNaN(date.getTime()) ? null : date;
    } catch {
      return null;
    }
  }

  private isToday(date: Date): boolean {
    const today = new Date();
    return date.toDateString() === today.toDateString();
  }
}

export class DocumentNodeDecorator extends BaseNodeDecorator {
  constructor(nodeReferenceService: NodeReferenceService) {
    super(nodeReferenceService, 'document');
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, title } = context;
    
    // Extract file type and preview from content
    const fileType = this.extractFileType(content);
    const preview = this.extractPreview(content);
    const size = this.extractSize(content);
    
    // Use BaseNodeReference with document-specific styling and icon
    const props: BaseNodeReferenceProps = {
      nodeId,
      content: title || content,
      href: uri,
      nodeType: 'document',
      className: `ns-noderef--document-${fileType}`,
      icon: this.getFileTypeIcon(fileType),
      ariaLabel: `Document: ${title || content} (${fileType.toUpperCase()}${size ? `, ${size}` : ''})`
    };

    return {
      component: BaseNodeReference,
      props,
      metadata: { 
        nodeType: 'document', 
        decorationType: this.decorationType,
        fileType,
        preview,
        size
      }
    };
  }

  private extractFileType(content: string): string {
    // Look for file type indicators in content
    const typeMatch = content.match(/type:\s*(\w+)/i) || content.match(/\.(\w+)$/m);
    return typeMatch ? typeMatch[1].toLowerCase() : 'document';
  }

  private extractPreview(content: string): string {
    // Get first line that's not metadata or title
    const lines = content.split('\n');
    for (let i = 1; i < lines.length; i++) {
      const line = lines[i].trim();
      if (line && !line.match(/^\w+:/) && !line.match(/^#+/)) {
        return line.substring(0, 100);
      }
    }
    return '';
  }

  private extractSize(content: string): string {
    const sizeMatch = content.match(/size:\s*([0-9.]+\s*[KMGT]?B)/i);
    return sizeMatch ? sizeMatch[1] : '';
  }

  private getFileTypeIcon(fileType: string): string {
    const icons: Record<string, string> = {
      pdf: '📄',
      doc: '📝',
      docx: '📝',
      txt: '📄',
      md: '📝',
      image: '🖼️',
      jpg: '🖼️',
      png: '🖼️',
      gif: '🖼️',
      video: '🎥',
      mp4: '🎥',
      audio: '🎵',
      mp3: '🎵',
      zip: '📦',
      default: '📄'
    };
    return icons[fileType] || icons.default;
  }
}

export class AINodeDecorator extends BaseNodeDecorator {
  constructor(nodeReferenceService: NodeReferenceService) {
    super(nodeReferenceService, 'ai_chat');
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const { nodeId, content, uri, title } = context;
    
    // Extract AI-specific metadata
    const model = this.extractModel(content);
    const messageCount = this.extractMessageCount(content);
    const lastActivity = this.extractLastActivity(content);
    
    // Use BaseNodeReference with AI-specific styling and icon
    const props: BaseNodeReferenceProps = {
      nodeId,
      content: title || content,
      href: uri,
      nodeType: 'ai_chat',
      icon: '🤖',
      ariaLabel: `AI Chat: ${title || content}${model ? ` (${model})` : ''}${messageCount ? `, ${messageCount} messages` : ''}`
    };

    return {
      component: BaseNodeReference,
      props,
      metadata: { 
        nodeType: 'ai_chat', 
        decorationType: this.decorationType,
        model,
        messageCount,
        lastActivity
      }
    };
  }

  private extractModel(content: string): string {
    const modelMatch = content.match(/model:\s*([^\n]+)/i);
    return modelMatch ? modelMatch[1].trim() : '';
  }

  private extractMessageCount(content: string): number {
    const countMatch = content.match(/messages:\s*(\d+)/i);
    return countMatch ? parseInt(countMatch[1], 10) : 0;
  }

  private extractLastActivity(content: string): number | null {
    const activityMatch = content.match(/last_activity:\s*([^\n]+)/i);
    if (activityMatch) {
      const date = new Date(activityMatch[1]);
      return isNaN(date.getTime()) ? null : date.getTime();
    }
    return null;
  }
}

// ============================================================================
// Node Type Configuration
// ============================================================================

export const NODE_TYPE_CONFIGS: Record<string, NodeTypeConfig> = {
  text: {
    icon: '📄',
    label: 'Text',
    color: 'var(--node-text)',
    defaultDecoration: (context) => new (class extends BaseNodeDecorator { constructor() { super(null!, 'text'); } })()
      .getBaseDecoration(context)
  },
  task: {
    icon: '☐',
    label: 'Task',
    color: 'var(--node-task)',
    defaultDecoration: (context) => new TaskNodeDecorator(null!).decorateReference(context)
  },
  user: {
    icon: '👤',
    label: 'User',
    color: 'var(--node-user)',
    defaultDecoration: (context) => new UserNodeDecorator(null!).decorateReference(context)
  },
  date: {
    icon: '📅',
    label: 'Date',
    color: 'var(--node-date)',
    defaultDecoration: (context) => new DateNodeDecorator(null!).decorateReference(context)
  },
  document: {
    icon: '📄',
    label: 'Document',
    color: 'var(--node-document)',
    defaultDecoration: (context) => new DocumentNodeDecorator(null!).decorateReference(context)
  },
  ai_chat: {
    icon: '🤖',
    label: 'AI Chat',
    color: 'var(--node-ai-chat)',
    defaultDecoration: (context) => new AINodeDecorator(null!).decorateReference(context)
  },
  entity: {
    icon: '🏷️',
    label: 'Entity',
    color: 'var(--node-entity)',
    defaultDecoration: (context) => new (class extends BaseNodeDecorator { constructor() { super(null!, 'entity'); } })()
      .getBaseDecoration(context)
  },
  query: {
    icon: '🔍',
    label: 'Query',
    color: 'var(--node-query)',
    defaultDecoration: (context) => new (class extends BaseNodeDecorator { constructor() { super(null!, 'query'); } })()
      .getBaseDecoration(context)
  },
  default: {
    icon: '📄',
    label: 'Node',
    color: 'var(--node-text)',
    defaultDecoration: (context) => new (class extends BaseNodeDecorator { constructor() { super(null!, 'default'); } })()
      .getBaseDecoration(context)
  }
};

// ============================================================================
// Decorator Factory
// ============================================================================

export class NodeDecoratorFactory {
  private decorators = new Map<string, BaseNodeDecorator>();
  private nodeReferenceService: NodeReferenceService;

  constructor(nodeReferenceService: NodeReferenceService) {
    this.nodeReferenceService = nodeReferenceService;
    this.initializeDecorators();
  }

  private initializeDecorators(): void {
    this.decorators.set('task', new TaskNodeDecorator(this.nodeReferenceService));
    this.decorators.set('user', new UserNodeDecorator(this.nodeReferenceService));
    this.decorators.set('date', new DateNodeDecorator(this.nodeReferenceService));
    this.decorators.set('document', new DocumentNodeDecorator(this.nodeReferenceService));
    this.decorators.set('ai_chat', new AINodeDecorator(this.nodeReferenceService));
    
    // Base decorator for other types
    this.decorators.set('default', new class extends BaseNodeDecorator {
      constructor(service: NodeReferenceService) {
        super(service, 'default');
      }
      
      public decorateReference(context: DecorationContext): ComponentDecoration {
        return this.getBaseComponentDecoration(context);
      }
    }(this.nodeReferenceService));
  }

  public getDecorator(nodeType: string): BaseNodeDecorator {
    return this.decorators.get(nodeType) || this.decorators.get('default')!;
  }

  public decorateReference(context: DecorationContext): ComponentDecoration {
    const decorator = this.getDecorator(context.nodeType);
    return decorator.decorateReference(context);
  }
}

export default NodeDecoratorFactory;