/**
 * Mock Text Service
 * 
 * Provides simulated CRUD operations for TextNode components.
 * Includes auto-save functionality and realistic network delays.
 * 
 * This mock service enables independent TextNode development
 * and will be replaced with real data store integration later.
 */

export interface TextNodeData {
  id: string;
  content: string;
  title: string;
  createdAt: Date;
  updatedAt: Date;
  metadata: {
    wordCount: number;
    lastEditedBy: string;
    version: number;
  };
}

export interface TextSaveResult {
  success: boolean;
  id: string;
  timestamp: Date;
  error?: string;
}

export class MockTextService {
  private static instance: MockTextService;
  private textNodes = new Map<string, TextNodeData>();
  private autoSaveTimeouts = new Map<string, NodeJS.Timeout>();

  static getInstance(): MockTextService {
    if (!MockTextService.instance) {
      MockTextService.instance = new MockTextService();
    }
    return MockTextService.instance;
  }

  constructor() {
    // Initialize with some sample data
    this.initializeSampleData();
  }

  private initializeSampleData(): void {
    const sampleNode: TextNodeData = {
      id: 'text-sample-1',
      content: 'Welcome to NodeSpace! This is a sample text node with **markdown** support.\n\n## Features\n- Click to edit\n- Auto-save\n- Markdown rendering\n- Keyboard shortcuts',
      title: 'Welcome Text Node',
      createdAt: new Date(Date.now() - 86400000), // 1 day ago
      updatedAt: new Date(Date.now() - 3600000), // 1 hour ago
      metadata: {
        wordCount: 25,
        lastEditedBy: 'user',
        version: 3
      }
    };
    
    this.textNodes.set(sampleNode.id, sampleNode);
  }

  /**
   * Save text node content with auto-save simulation
   */
  async saveTextNode(
    id: string, 
    content: string, 
    title?: string
  ): Promise<TextSaveResult> {
    // Simulate network delay
    await new Promise(resolve => setTimeout(resolve, 200 + Math.random() * 300));

    try {
      const now = new Date();
      const existingNode = this.textNodes.get(id);
      
      const nodeData: TextNodeData = {
        id,
        content,
        title: title || existingNode?.title || 'Untitled',
        createdAt: existingNode?.createdAt || now,
        updatedAt: now,
        metadata: {
          wordCount: this.calculateWordCount(content),
          lastEditedBy: 'user',
          version: (existingNode?.metadata.version || 0) + 1
        }
      };

      this.textNodes.set(id, nodeData);

      return {
        success: true,
        id,
        timestamp: now
      };

    } catch (error) {
      return {
        success: false,
        id,
        timestamp: new Date(),
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }

  /**
   * Load text node by ID
   */
  async loadTextNode(id: string): Promise<TextNodeData | null> {
    // Simulate network delay
    await new Promise(resolve => setTimeout(resolve, 100 + Math.random() * 200));

    return this.textNodes.get(id) || null;
  }

  /**
   * Create new text node
   */
  async createTextNode(
    content: string = '', 
    title: string = 'New Text Node'
  ): Promise<TextNodeData> {
    await new Promise(resolve => setTimeout(resolve, 150));

    const id = this.generateId();
    const now = new Date();
    
    const nodeData: TextNodeData = {
      id,
      content,
      title,
      createdAt: now,
      updatedAt: now,
      metadata: {
        wordCount: this.calculateWordCount(content),
        lastEditedBy: 'user',
        version: 1
      }
    };

    this.textNodes.set(id, nodeData);
    return nodeData;
  }

  /**
   * Delete text node
   */
  async deleteTextNode(id: string): Promise<boolean> {
    await new Promise(resolve => setTimeout(resolve, 100));
    
    // Clear any pending auto-save
    const timeout = this.autoSaveTimeouts.get(id);
    if (timeout) {
      clearTimeout(timeout);
      this.autoSaveTimeouts.delete(id);
    }
    
    return this.textNodes.delete(id);
  }

  /**
   * Auto-save with debouncing
   */
  scheduleAutoSave(
    id: string, 
    content: string, 
    title?: string,
    delay: number = 2000
  ): Promise<TextSaveResult> {
    return new Promise((resolve) => {
      // Clear existing timeout for this node
      const existingTimeout = this.autoSaveTimeouts.get(id);
      if (existingTimeout) {
        clearTimeout(existingTimeout);
      }

      // Set new timeout
      const timeout = setTimeout(async () => {
        const result = await this.saveTextNode(id, content, title);
        this.autoSaveTimeouts.delete(id);
        resolve(result);
      }, delay);

      this.autoSaveTimeouts.set(id, timeout);
    });
  }

  /**
   * Cancel pending auto-save for a node
   */
  cancelAutoSave(id: string): void {
    const timeout = this.autoSaveTimeouts.get(id);
    if (timeout) {
      clearTimeout(timeout);
      this.autoSaveTimeouts.delete(id);
    }
  }

  /**
   * Get all text nodes (for listing/search)
   */
  async getAllTextNodes(): Promise<TextNodeData[]> {
    await new Promise(resolve => setTimeout(resolve, 150));
    
    return Array.from(this.textNodes.values())
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
  }

  /**
   * Search text nodes by content or title
   */
  async searchTextNodes(query: string): Promise<TextNodeData[]> {
    await new Promise(resolve => setTimeout(resolve, 200));

    if (!query.trim()) {
      return this.getAllTextNodes();
    }

    const searchLower = query.toLowerCase();
    
    return Array.from(this.textNodes.values())
      .filter(node => 
        node.title.toLowerCase().includes(searchLower) ||
        node.content.toLowerCase().includes(searchLower)
      )
      .sort((a, b) => b.updatedAt.getTime() - a.updatedAt.getTime());
  }

  /**
   * Generate unique node ID
   */
  private generateId(): string {
    return `text-${Date.now()}-${Math.random().toString(36).substring(2, 9)}`;
  }

  /**
   * Calculate word count for content
   */
  private calculateWordCount(content: string): number {
    return content
      .trim()
      .split(/\s+/)
      .filter(word => word.length > 0)
      .length;
  }

  /**
   * Get service statistics (for debugging/monitoring)
   */
  getStats() {
    return {
      totalNodes: this.textNodes.size,
      pendingAutoSaves: this.autoSaveTimeouts.size,
      lastActivity: new Date()
    };
  }
}

// Export singleton instance
export const mockTextService = MockTextService.getInstance();

// Export types for use by components
export type { TextNodeData, TextSaveResult };