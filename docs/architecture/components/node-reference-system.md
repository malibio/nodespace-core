# Node Reference System

## Overview

The Node Reference System provides a universal way to reference any node type in NodeSpace using `@` triggers, autocomplete selection, and extensible visual decorations. The system leverages standard markdown links with `nodespace://` URIs while allowing each node type to control its own reference appearance.

## Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────┐
│                    User Experience Layer                        │
│  ┌─────────────────┐    ┌─────────────────┐    ┌──────────────┐ │
│  │ @ Keystroke     │    │ Autocomplete    │    │ Node         │ │
│  │ Detection       │───▶│ Modal           │───▶│ Selection    │ │
│  └─────────────────┘    └─────────────────┘    └──────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Storage Layer                                │
│             [display-text](nodespace://uuid)                   │
│                   Standard Markdown Links                      │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                   Processing Layer                              │
│  ┌─────────────────┐    ┌─────────────────┐    ┌──────────────┐ │
│  │ ContentProcessor│    │ Node Lookup     │    │ Decoration   │ │
│  │ Link Detection  │───▶│ Service         │───▶│ Rendering    │ │
│  └─────────────────┘    └─────────────────┘    └──────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Decoration Layer                             │
│  ┌─────────────────┐    ┌─────────────────┐    ┌──────────────┐ │
│  │ BaseNode        │    │ TaskNode        │    │ UserNode     │ │
│  │ Default         │    │ Checkbox +      │    │ Avatar +     │ │
│  │ Decoration      │    │ Status          │    │ Status       │ │
│  └─────────────────┘    └─────────────────┘    └──────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

## Core Components

### 1. BaseNode Decoration Interface

All nodes inherit from BaseNode and can override decoration behavior:

```typescript
/**
 * Base node class providing default reference decoration
 * All node types inherit from this foundation
 */
abstract class BaseNode {
  id: string;
  content: string;  // Raw content - interpretation up to derived types
  created_at: Date;
  modified_at: Date;
  metadata: Record<string, any>;

  /**
   * Default decoration renders content as simple link
   * Derived classes override this method for custom appearance
   */
  decorateReference(): string {
    return `<a href="nodespace://${this.id}" class="ns-noderef">${this.escapeHtml(this.content)}</a>`;
  }

  /**
   * Security: Escape HTML in content to prevent XSS
   */
  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }

  /**
   * Get display text for autocomplete and fallbacks
   */
  getDisplayText(): string {
    return this.content;
  }
}
```

### 2. Node Type Decorations

Each derived node type controls its own reference appearance:

#### TaskNode Example
```typescript
class TaskNode extends BaseNode {
  status: 'not-started' | 'in-progress' | 'completed' | 'blocked';
  priority: 'low' | 'medium' | 'high' | 'critical';
  due_date?: Date;
  assigned_to?: string;

  decorateReference(): string {
    const checkbox = this.status === 'completed' ? '✅' : '☐';
    const statusClass = `task-${this.status}`;
    const priorityClass = `priority-${this.priority}`;
    
    let decoration = `<a href="nodespace://${this.id}" class="ns-noderef ns-task ${statusClass} ${priorityClass}">`;
    decoration += `${checkbox} ${this.escapeHtml(this.content)}`;
    
    // Add due date if present and approaching
    if (this.due_date && this.isDueSoon()) {
      decoration += `<span class="due-indicator">📅 ${this.formatDueDate()}</span>`;
    }
    
    // Add priority indicator for high/critical tasks
    if (this.priority === 'high' || this.priority === 'critical') {
      decoration += `<span class="priority-indicator">⚡</span>`;
    }
    
    decoration += '</a>';
    return decoration;
  }

  private isDueSoon(): boolean {
    if (!this.due_date) return false;
    const now = new Date();
    const daysDiff = (this.due_date.getTime() - now.getTime()) / (1000 * 60 * 60 * 24);
    return daysDiff <= 3 && daysDiff >= 0; // Due within 3 days
  }

  private formatDueDate(): string {
    if (!this.due_date) return '';
    const today = new Date();
    const tomorrow = new Date(today);
    tomorrow.setDate(today.getDate() + 1);
    
    if (this.due_date.toDateString() === today.toDateString()) return 'Today';
    if (this.due_date.toDateString() === tomorrow.toDateString()) return 'Tomorrow';
    return this.due_date.toLocaleDateString();
  }
}
```

#### UserNode Example
```typescript
class UserNode extends BaseNode {
  email: string;
  role: string;
  avatar_url?: string;
  is_online: boolean;
  last_seen?: Date;

  decorateReference(): string {
    const statusIndicator = this.is_online ? '🟢' : '⚫';
    const roleClass = `role-${this.role.toLowerCase()}`;
    
    let decoration = `<a href="nodespace://${this.id}" class="ns-noderef ns-user ${roleClass}">`;
    
    // Add avatar if available
    if (this.avatar_url) {
      decoration += `<img src="${this.avatar_url}" class="user-avatar" alt="${this.content}">`;
    } else {
      decoration += `<span class="user-icon">👤</span>`;
    }
    
    decoration += `${this.escapeHtml(this.content)} ${statusIndicator}`;
    
    // Add role indicator for non-standard roles
    if (this.role !== 'member') {
      decoration += `<span class="role-badge">${this.role}</span>`;
    }
    
    decoration += '</a>';
    return decoration;
  }
}
```

#### DateNode Example
```typescript
class DateNode extends BaseNode {
  date: Date;
  date_type: 'date' | 'datetime' | 'time' | 'duration';
  timezone?: string;

  decorateReference(): string {
    const calendarIcon = this.getCalendarIcon();
    const formattedDate = this.formatDate();
    const dateClass = `date-${this.date_type}`;
    
    let decoration = `<a href="nodespace://${this.id}" class="ns-noderef ns-date ${dateClass}">`;
    decoration += `${calendarIcon} ${formattedDate}`;
    
    // Add relative time for recent/upcoming dates
    const relativeTime = this.getRelativeTime();
    if (relativeTime) {
      decoration += `<span class="relative-time">(${relativeTime})</span>`;
    }
    
    decoration += '</a>';
    return decoration;
  }

  private getCalendarIcon(): string {
    const now = new Date();
    if (this.date.toDateString() === now.toDateString()) return '📅'; // Today
    if (this.date < now) return '📆'; // Past
    return '🗓️'; // Future
  }

  private formatDate(): string {
    switch (this.date_type) {
      case 'time':
        return this.date.toLocaleTimeString();
      case 'datetime':
        return this.date.toLocaleString();
      default:
        return this.date.toLocaleDateString();
    }
  }

  private getRelativeTime(): string | null {
    const now = new Date();
    const diffMs = this.date.getTime() - now.getTime();
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    
    if (Math.abs(diffDays) > 7) return null; // Don't show relative for > 1 week
    
    if (diffDays === 0) return 'Today';
    if (diffDays === 1) return 'Tomorrow';
    if (diffDays === -1) return 'Yesterday';
    if (diffDays > 1) return `in ${diffDays} days`;
    if (diffDays < -1) return `${Math.abs(diffDays)} days ago`;
    
    return null;
  }
}
```

### 3. ContentProcessor Integration

The ContentProcessor detects and renders `nodespace://` links:

```typescript
/**
 * Enhanced ContentProcessor with nodespace:// link handling
 */
class ContentProcessor {
  private nodeRegistry: NodeLookupService;
  private NODESPACE_LINK_REGEX = /\[([^\]]+)\]\(nodespace:\/\/([^)]+)\)/g;

  constructor(nodeRegistry: NodeLookupService) {
    this.nodeRegistry = nodeRegistry;
  }

  /**
   * Process markdown content, handling nodespace:// links specially
   */
  processMarkdown(content: string): string {
    // Process regular markdown first
    let processed = this.processRegularMarkdown(content);
    
    // Then handle nodespace links with custom decoration
    processed = this.processNodespaceLinks(processed);
    
    return processed;
  }

  /**
   * Process nodespace:// links with node-specific decorations
   */
  private processNodespaceLinks(content: string): string {
    return content.replace(this.NODESPACE_LINK_REGEX, (match, linkText, nodeId) => {
      try {
        const node = this.nodeRegistry.getNode(nodeId);
        
        if (node && typeof node.decorateReference === 'function') {
          // Use node's custom decoration
          return node.decorateReference();
        } else {
          // Fallback for missing nodes or nodes without decoration
          return `<a href="nodespace://${nodeId}" class="ns-noderef ns-missing" title="Node not found">
            ${this.escapeHtml(linkText)}
          </a>`;
        }
      } catch (error) {
        console.warn(`Failed to render node reference ${nodeId}:`, error);
        return `<a href="nodespace://${nodeId}" class="ns-noderef ns-error">
          ${this.escapeHtml(linkText)}
        </a>`;
      }
    });
  }

  /**
   * Extract all node references from content
   */
  extractNodeReferences(content: string): NodeReference[] {
    const references: NodeReference[] = [];
    let match;

    // Reset regex state
    this.NODESPACE_LINK_REGEX.lastIndex = 0;

    while ((match = this.NODESPACE_LINK_REGEX.exec(content)) !== null) {
      references.push({
        linkText: match[1],
        nodeId: match[2],
        startPos: match.index,
        endPos: match.index + match[0].length,
        resolved: this.nodeRegistry.hasNode(match[2])
      });
    }

    return references;
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}

interface NodeReference {
  linkText: string;
  nodeId: string;
  startPos: number;
  endPos: number;
  resolved: boolean;
}
```

### 4. Node Lookup Service

Provides node resolution and caching:

```typescript
/**
 * Service for resolving node UUIDs to node instances
 */
interface NodeLookupService {
  getNode(nodeId: string): BaseNode | null;
  hasNode(nodeId: string): boolean;
  searchNodes(query: string, nodeType?: string): BaseNode[];
  createNode(nodeType: string, content: string): BaseNode;
}

class NodeLookupServiceImpl implements NodeLookupService {
  private nodeCache = new Map<string, BaseNode>();
  private nodeStorage: NodeStorage;

  constructor(nodeStorage: NodeStorage) {
    this.nodeStorage = nodeStorage;
  }

  getNode(nodeId: string): BaseNode | null {
    // Check cache first
    if (this.nodeCache.has(nodeId)) {
      return this.nodeCache.get(nodeId)!;
    }

    // Load from storage
    try {
      const node = this.nodeStorage.loadNode(nodeId);
      if (node) {
        this.nodeCache.set(nodeId, node);
        return node;
      }
    } catch (error) {
      console.warn(`Failed to load node ${nodeId}:`, error);
    }

    return null;
  }

  hasNode(nodeId: string): boolean {
    return this.getNode(nodeId) !== null;
  }

  searchNodes(query: string, nodeType?: string): BaseNode[] {
    const allNodes = this.nodeStorage.getAllNodes();
    
    return allNodes.filter(node => {
      // Filter by type if specified
      if (nodeType && node.constructor.name.toLowerCase() !== nodeType.toLowerCase() + 'node') {
        return false;
      }

      // Search in content (case-insensitive)
      return node.content.toLowerCase().includes(query.toLowerCase());
    }).slice(0, 10); // Limit results for performance
  }

  createNode(nodeType: string, content: string): BaseNode {
    const nodeId = this.generateUuid();
    
    switch (nodeType.toLowerCase()) {
      case 'task':
        return new TaskNode(nodeId, content);
      case 'user':
        return new UserNode(nodeId, content);
      case 'date':
        return new DateNode(nodeId, content);
      default:
        return new BaseNode(nodeId, content);
    }
  }

  private generateUuid(): string {
    return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
      const r = Math.random() * 16 | 0;
      const v = c == 'x' ? r : (r & 0x3 | 0x8);
      return v.toString(16);
    });
  }
}
```

### 5. Autocomplete System

Handles `@` keystroke detection and node selection:

```typescript
/**
 * Autocomplete service for @ triggers
 */
class AutocompleteService {
  private nodeRegistry: NodeLookupService;
  private modalElement: HTMLElement | null = null;
  private currentQuery = '';
  private selectedIndex = 0;
  private onSelection: ((node: BaseNode) => void) | null = null;

  constructor(nodeRegistry: NodeLookupService) {
    this.nodeRegistry = nodeRegistry;
  }

  /**
   * Show autocomplete modal at cursor position
   */
  showAutocomplete(
    triggerElement: HTMLElement, 
    cursorPosition: { x: number, y: number },
    onSelection: (node: BaseNode) => void
  ): void {
    this.onSelection = onSelection;
    this.currentQuery = '';
    this.selectedIndex = 0;

    // Create modal if it doesn't exist
    if (!this.modalElement) {
      this.createModal();
    }

    // Position modal near cursor
    this.positionModal(cursorPosition);
    
    // Show modal with all nodes initially
    this.updateResults('');
    this.showModal();
  }

  /**
   * Update search query and filter results
   */
  updateQuery(query: string): void {
    this.currentQuery = query;
    this.selectedIndex = 0;
    this.updateResults(query);
  }

  /**
   * Handle keyboard navigation
   */
  handleKeyDown(event: KeyboardEvent): boolean {
    if (!this.isVisible()) return false;

    switch (event.key) {
      case 'ArrowDown':
        event.preventDefault();
        this.moveSelection(1);
        return true;
      case 'ArrowUp':
        event.preventDefault();
        this.moveSelection(-1);
        return true;
      case 'Enter':
        event.preventDefault();
        this.selectCurrent();
        return true;
      case 'Escape':
        event.preventDefault();
        this.hideAutocomplete();
        return true;
      default:
        return false;
    }
  }

  /**
   * Hide autocomplete modal
   */
  hideAutocomplete(): void {
    if (this.modalElement) {
      this.modalElement.style.display = 'none';
    }
    this.onSelection = null;
  }

  private createModal(): void {
    this.modalElement = document.createElement('div');
    this.modalElement.className = 'ns-autocomplete-modal';
    this.modalElement.innerHTML = `
      <div class="autocomplete-content">
        <div class="autocomplete-results"></div>
        <div class="autocomplete-footer">
          <small>↑↓ navigate • Enter select • Esc cancel</small>
        </div>
      </div>
    `;
    document.body.appendChild(this.modalElement);
  }

  private updateResults(query: string): void {
    const results = this.nodeRegistry.searchNodes(query);
    const resultsContainer = this.modalElement?.querySelector('.autocomplete-results');
    
    if (!resultsContainer) return;

    if (results.length === 0) {
      resultsContainer.innerHTML = `
        <div class="autocomplete-item create-new">
          <span class="item-icon">✨</span>
          <span class="item-text">Create new node "${query}"</span>
        </div>
      `;
    } else {
      resultsContainer.innerHTML = results.map((node, index) => `
        <div class="autocomplete-item ${index === this.selectedIndex ? 'selected' : ''}" data-index="${index}">
          <span class="item-icon">${this.getNodeIcon(node)}</span>
          <span class="item-text">${this.escapeHtml(node.content)}</span>
          <span class="item-type">${this.getNodeTypeName(node)}</span>
        </div>
      `).join('') + `
        <div class="autocomplete-item create-new ${results.length === this.selectedIndex ? 'selected' : ''}" data-index="${results.length}">
          <span class="item-icon">✨</span>
          <span class="item-text">Create new node "${query}"</span>
        </div>
      `;
    }
  }

  private getNodeIcon(node: BaseNode): string {
    if (node instanceof TaskNode) return '☐';
    if (node instanceof UserNode) return '👤';
    if (node instanceof DateNode) return '📅';
    return '📄';
  }

  private getNodeTypeName(node: BaseNode): string {
    return node.constructor.name.replace('Node', '');
  }

  private moveSelection(direction: number): void {
    const items = this.modalElement?.querySelectorAll('.autocomplete-item');
    if (!items) return;

    this.selectedIndex = Math.max(0, Math.min(items.length - 1, this.selectedIndex + direction));
    
    // Update visual selection
    items.forEach((item, index) => {
      item.classList.toggle('selected', index === this.selectedIndex);
    });
  }

  private selectCurrent(): void {
    const selectedItem = this.modalElement?.querySelector(`.autocomplete-item[data-index="${this.selectedIndex}"]`);
    
    if (selectedItem?.classList.contains('create-new')) {
      // Create new node
      const newNode = this.nodeRegistry.createNode('text', this.currentQuery);
      this.onSelection?.(newNode);
    } else {
      // Select existing node
      const results = this.nodeRegistry.searchNodes(this.currentQuery);
      const selectedNode = results[this.selectedIndex];
      if (selectedNode) {
        this.onSelection?.(selectedNode);
      }
    }

    this.hideAutocomplete();
  }

  private positionModal(cursorPosition: { x: number, y: number }): void {
    if (!this.modalElement) return;

    // Position below cursor, but adjust if near screen edges
    const modal = this.modalElement;
    const rect = modal.getBoundingClientRect();
    
    let x = cursorPosition.x;
    let y = cursorPosition.y + 20; // 20px below cursor

    // Adjust if modal would go off-screen
    if (x + rect.width > window.innerWidth) {
      x = window.innerWidth - rect.width - 10;
    }
    if (y + rect.height > window.innerHeight) {
      y = cursorPosition.y - rect.height - 10; // Show above cursor
    }

    modal.style.left = `${x}px`;
    modal.style.top = `${y}px`;
  }

  private showModal(): void {
    if (this.modalElement) {
      this.modalElement.style.display = 'block';
    }
  }

  private isVisible(): boolean {
    return this.modalElement?.style.display === 'block';
  }

  private escapeHtml(text: string): string {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
  }
}
```

## CSS Framework

### Base Styling

```css
/* Base node reference styles */
.ns-noderef {
  display: inline-flex;
  align-items: center;
  gap: 0.25rem;
  padding: 0.125rem 0.25rem;
  border-radius: 0.25rem;
  text-decoration: none;
  font-weight: 500;
  transition: background-color 0.15s ease;
}

.ns-noderef:hover {
  background-color: hsl(var(--muted));
}

/* Missing/error states */
.ns-noderef.ns-missing {
  color: hsl(var(--muted-foreground));
  opacity: 0.6;
}

.ns-noderef.ns-error {
  color: hsl(var(--destructive));
  background-color: hsl(var(--destructive) / 0.1);
}
```

### Node Type Specific Styling

```css
/* Task node references */
.ns-noderef.ns-task {
  color: hsl(25 95% 53%); /* Orange for tasks */
  background-color: hsl(25 95% 53% / 0.1);
}

.ns-noderef.ns-task.task-completed {
  color: hsl(142 71% 45%); /* Green for completed */
  text-decoration: line-through;
}

.ns-noderef.ns-task.task-blocked {
  color: hsl(0 84% 60%); /* Red for blocked */
}

.ns-noderef.ns-task .due-indicator {
  font-size: 0.75rem;
  margin-left: 0.25rem;
  opacity: 0.8;
}

.ns-noderef.ns-task .priority-indicator {
  color: hsl(0 84% 60%);
  margin-left: 0.125rem;
}

/* User node references */
.ns-noderef.ns-user {
  color: hsl(221 83% 53%); /* Blue for users */
  background-color: hsl(221 83% 53% / 0.1);
}

.ns-noderef.ns-user .user-avatar {
  width: 1rem;
  height: 1rem;
  border-radius: 50%;
  object-fit: cover;
}

.ns-noderef.ns-user .role-badge {
  font-size: 0.625rem;
  background-color: hsl(var(--muted));
  padding: 0.125rem 0.25rem;
  border-radius: 0.125rem;
  margin-left: 0.25rem;
}

/* Date node references */
.ns-noderef.ns-date {
  color: hsl(271 81% 56%); /* Purple for dates */
  background-color: hsl(271 81% 56% / 0.1);
}

.ns-noderef.ns-date .relative-time {
  font-size: 0.75rem;
  opacity: 0.7;
  margin-left: 0.25rem;
}
```

### Autocomplete Modal Styling

```css
/* Autocomplete modal */
.ns-autocomplete-modal {
  position: fixed;
  z-index: 1000;
  background: hsl(var(--background));
  border: 1px solid hsl(var(--border));
  border-radius: 0.5rem;
  box-shadow: 0 10px 15px -3px rgb(0 0 0 / 0.1);
  min-width: 300px;
  max-width: 400px;
  max-height: 300px;
  overflow: hidden;
}

.autocomplete-content {
  display: flex;
  flex-direction: column;
  height: 100%;
}

.autocomplete-results {
  flex: 1;
  overflow-y: auto;
  padding: 0.5rem 0;
}

.autocomplete-item {
  display: flex;
  align-items: center;
  gap: 0.5rem;
  padding: 0.5rem 0.75rem;
  cursor: pointer;
  transition: background-color 0.15s ease;
}

.autocomplete-item:hover,
.autocomplete-item.selected {
  background-color: hsl(var(--muted));
}

.autocomplete-item.create-new {
  border-top: 1px solid hsl(var(--border));
  font-style: italic;
  color: hsl(var(--muted-foreground));
}

.item-icon {
  font-size: 1rem;
  width: 1rem;
  text-align: center;
}

.item-text {
  flex: 1;
  font-weight: 500;
}

.item-type {
  font-size: 0.75rem;
  color: hsl(var(--muted-foreground));
  background-color: hsl(var(--muted));
  padding: 0.125rem 0.375rem;
  border-radius: 0.25rem;
}

.autocomplete-footer {
  padding: 0.5rem 0.75rem;
  border-top: 1px solid hsl(var(--border));
  background-color: hsl(var(--muted) / 0.5);
  font-size: 0.75rem;
  color: hsl(var(--muted-foreground));
}
```

## Integration Example

### TextNode Integration

```typescript
/**
 * Enhanced TextNode with @ trigger support
 */
class TextNode extends BaseNode {
  private autocompleteService: AutocompleteService;
  private contentProcessor: ContentProcessor;

  constructor(
    nodeId: string, 
    content: string,
    autocompleteService: AutocompleteService,
    contentProcessor: ContentProcessor
  ) {
    super(nodeId, content);
    this.autocompleteService = autocompleteService;
    this.contentProcessor = contentProcessor;
  }

  /**
   * Handle keydown events, detecting @ triggers
   */
  handleKeyDown(event: KeyboardEvent, element: HTMLElement): void {
    // Check for @ trigger
    if (event.key === '@') {
      const cursorPosition = this.getCursorPosition(element);
      
      // Show autocomplete after @ character is inserted
      setTimeout(() => {
        this.showAutocomplete(element, cursorPosition);
      }, 0);
    }
    
    // Handle autocomplete navigation
    if (this.autocompleteService.handleKeyDown(event)) {
      return; // Autocomplete handled the event
    }
  }

  /**
   * Show autocomplete modal for node selection
   */
  private showAutocomplete(element: HTMLElement, position: { x: number, y: number }): void {
    this.autocompleteService.showAutocomplete(
      element,
      position,
      (selectedNode: BaseNode) => {
        this.insertNodeReference(element, selectedNode);
      }
    );
  }

  /**
   * Insert node reference as markdown link
   */
  private insertNodeReference(element: HTMLElement, node: BaseNode): void {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) return;

    const range = selection.getRangeAt(0);
    
    // Delete the @ character that triggered autocomplete
    range.setStart(range.startContainer, range.startOffset - 1);
    range.deleteContents();

    // Insert markdown link
    const linkText = node.getDisplayText();
    const markdownLink = `[${linkText}](nodespace://${node.id})`;
    
    const textNode = document.createTextNode(markdownLink);
    range.insertNode(textNode);
    
    // Move cursor after inserted link
    range.setStartAfter(textNode);
    range.collapse(true);
    selection.removeAllRanges();
    selection.addRange(range);

    // Trigger content change event for reprocessing
    this.onContentChange(element.textContent || '');
  }

  /**
   * Process content with node reference decoration
   */
  processContent(content: string): string {
    return this.contentProcessor.processMarkdown(content);
  }

  private getCursorPosition(element: HTMLElement): { x: number, y: number } {
    const selection = window.getSelection();
    if (!selection || selection.rangeCount === 0) {
      return { x: 0, y: 0 };
    }

    const range = selection.getRangeAt(0);
    const rect = range.getBoundingClientRect();
    
    return {
      x: rect.left,
      y: rect.top
    };
  }
}
```

## Performance Considerations

### 1. Node Lookup Caching

```typescript
class CachedNodeRegistry implements NodeLookupService {
  private cache = new LRUCache<string, BaseNode>(1000); // Cache 1000 nodes
  private searchCache = new Map<string, BaseNode[]>();

  getNode(nodeId: string): BaseNode | null {
    // Check cache first
    if (this.cache.has(nodeId)) {
      return this.cache.get(nodeId)!;
    }

    // Load from storage and cache
    const node = this.storage.loadNode(nodeId);
    if (node) {
      this.cache.set(nodeId, node);
    }
    
    return node;
  }

  searchNodes(query: string): BaseNode[] {
    // Cache search results for common queries
    const cacheKey = query.toLowerCase();
    if (this.searchCache.has(cacheKey)) {
      return this.searchCache.get(cacheKey)!;
    }

    const results = this.performSearch(query);
    this.searchCache.set(cacheKey, results);
    
    return results;
  }
}
```

### 2. Lazy Decoration Loading

```typescript
class LazyDecorationRenderer {
  private decorationCache = new Map<string, string>();
  private observer: IntersectionObserver;

  constructor() {
    // Only decorate references when they become visible
    this.observer = new IntersectionObserver((entries) => {
      entries.forEach(entry => {
        if (entry.isIntersecting) {
          this.decorateElement(entry.target as HTMLElement);
        }
      });
    });
  }

  observeNodeReference(element: HTMLElement): void {
    this.observer.observe(element);
  }

  private decorateElement(element: HTMLElement): void {
    const nodeId = this.extractNodeId(element);
    if (!nodeId) return;

    // Check cache first
    if (this.decorationCache.has(nodeId)) {
      element.innerHTML = this.decorationCache.get(nodeId)!;
      return;
    }

    // Load and decorate
    const node = this.nodeRegistry.getNode(nodeId);
    if (node) {
      const decoration = node.decorateReference();
      this.decorationCache.set(nodeId, decoration);
      element.innerHTML = decoration;
    }
  }
}
```

## Security Considerations

### 1. Content Sanitization

All node content must be escaped to prevent XSS attacks:

```typescript
private escapeHtml(text: string): string {
  const div = document.createElement('div');
  div.textContent = text;
  return div.innerHTML;
}

// Additional sanitization for rich content
private sanitizeRichContent(html: string): string {
  // Use DOMPurify or similar library for complex HTML sanitization
  return DOMPurify.sanitize(html, {
    ALLOWED_TAGS: ['span', 'strong', 'em', 'code'],
    ALLOWED_ATTR: ['class']
  });
}
```

### 2. Node Access Control

Implement access control for node references:

```typescript
interface NodeAccessControl {
  canViewNode(nodeId: string, userId: string): boolean;
  canReferenceNode(nodeId: string, userId: string): boolean;
}

class SecureNodeRegistry implements NodeLookupService {
  constructor(
    private baseRegistry: NodeLookupService,
    private accessControl: NodeAccessControl,
    private currentUserId: string
  ) {}

  getNode(nodeId: string): BaseNode | null {
    if (!this.accessControl.canViewNode(nodeId, this.currentUserId)) {
      return null; // Hide inaccessible nodes
    }
    
    return this.baseRegistry.getNode(nodeId);
  }
}
```

## Future Extensions

### 1. Interactive Decorations

```typescript
class InteractiveTaskNode extends TaskNode {
  decorateReference(): string {
    return `<button 
      onclick="toggleTaskStatus('${this.id}')" 
      class="ns-noderef ns-task interactive"
      data-task-id="${this.id}"
    >
      ${this.getCheckbox()} ${this.escapeHtml(this.content)}
    </button>`;
  }
}

// Global handler for interactive elements
window.toggleTaskStatus = (taskId: string) => {
  const taskNode = nodeRegistry.getNode(taskId) as TaskNode;
  if (taskNode) {
    taskNode.toggleStatus();
    // Refresh all references to this task
    refreshNodeReferences(taskId);
  }
};
```

### 2. Real-time Updates

```typescript
class RealtimeNodeRegistry extends CachedNodeRegistry {
  constructor(private websocket: WebSocket) {
    super();
    
    // Listen for node updates from server
    this.websocket.addEventListener('message', (event) => {
      const update = JSON.parse(event.data);
      if (update.type === 'node_updated') {
        this.handleNodeUpdate(update.nodeId, update.node);
      }
    });
  }

  private handleNodeUpdate(nodeId: string, updatedNode: BaseNode): void {
    // Update cache
    this.cache.set(nodeId, updatedNode);
    
    // Refresh all DOM references to this node
    this.refreshNodeReferences(nodeId);
  }

  private refreshNodeReferences(nodeId: string): void {
    const references = document.querySelectorAll(`[href="nodespace://${nodeId}"]`);
    references.forEach(ref => {
      const node = this.getNode(nodeId);
      if (node) {
        ref.outerHTML = node.decorateReference();
      }
    });
  }
}
```

### 3. Context-Aware Decorations

```typescript
class ContextAwareNode extends BaseNode {
  decorateReference(context?: RenderContext): string {
    const baseDecoration = super.decorateReference();
    
    if (context?.compact) {
      return this.getCompactDecoration();
    }
    
    if (context?.preview) {
      return this.getPreviewDecoration();
    }
    
    return baseDecoration;
  }

  private getCompactDecoration(): string {
    return `<a href="nodespace://${this.id}" class="ns-noderef compact">
      ${this.getIcon()} ${this.getTruncatedContent(20)}
    </a>`;
  }
}

interface RenderContext {
  compact?: boolean;
  preview?: boolean;
  maxWidth?: number;
  theme?: 'light' | 'dark';
}
```

---

This component architecture provides a flexible, extensible foundation for the Node Reference System that scales from simple link decoration to rich, interactive node relationships while maintaining security and performance.