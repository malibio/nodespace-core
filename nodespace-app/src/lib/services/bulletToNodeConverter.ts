/**
 * Smart Bullet-to-Node Conversion Service
 * 
 * Converts markdown bullet patterns into actual NodeSpace child node relationships.
 * Eliminates visual bullets in favor of physical hierarchy through node indentation.
 */

import type { 
  MarkdownPattern,
  BulletConversionConfig,
  BulletConversionResult
} from '$lib/types/markdownPatterns';
import type { TreeNodeData } from '$lib/types/tree';
import { patternIntegrationUtils } from './markdownPatternUtils';

/**
 * Individual bullet detection result
 */
interface BulletMatch {
  /** The bullet pattern detected */
  pattern: MarkdownPattern;
  /** Indentation level (0-based) */
  indentLevel: number;
  /** Content without bullet syntax */
  cleanContent: string;
  /** Line number in original content */
  lineNumber: number;
}

/**
 * Default configuration
 */
const DEFAULT_CONFIG: BulletConversionConfig = {
  createNewNodes: true,
  preserveBulletSyntax: false,
  maxDepth: 10,
  defaultNodeType: 'text',
  indentSize: 4
};

/**
 * Main bullet-to-node conversion service
 */
export class BulletToNodeConverter {
  
  private config: BulletConversionConfig;
  
  constructor(config: Partial<BulletConversionConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }
  
  /**
   * Convert bullet patterns in content to node hierarchy
   */
  convertBulletsToNodes(
    content: string, 
    patterns: MarkdownPattern[],
    cursorPosition: number,
    parentNodeId?: string
  ): BulletConversionResult {
    
    // Extract bullet patterns using utility from Issue #56
    const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(patterns);
    
    if (bulletPatterns.length === 0) {
      return {
        hasConversions: false,
        cleanedContent: content,
        newNodes: [],
        newCursorPosition: cursorPosition
      };
    }
    
    // Analyze bullet structure and indentation
    const bulletMatches = this.analyzeBulletStructure(content, bulletPatterns);
    
    // Convert bullets to node hierarchy
    const nodeHierarchy = this.buildNodeHierarchy(bulletMatches, parentNodeId);
    
    // Clean content by removing bullet syntax
    const cleanedContent = this.removeBulletSyntax(content, bulletPatterns);
    
    // Calculate new cursor position after bullet removal
    const newCursorPosition = this.calculateNewCursorPosition(
      content, 
      cursorPosition, 
      bulletPatterns
    );
    
    return {
      hasConversions: true,
      cleanedContent,
      newNodes: nodeHierarchy,
      newCursorPosition,
      parentNodeId
    };
  }
  
  /**
   * Detect bullet typing in real-time and trigger conversion
   */
  detectBulletTyping(
    content: string,
    patterns: MarkdownPattern[],
    cursorPosition: number
  ): boolean {
    // Look for bullet patterns near cursor position
    const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(patterns);
    
    // Check if cursor is immediately after a bullet pattern
    for (const pattern of bulletPatterns) {
      const bulletEndPosition = pattern.start + pattern.syntax.length;
      if (cursorPosition >= bulletEndPosition && cursorPosition <= bulletEndPosition + 2) {
        return true;
      }
    }
    
    // Check for fresh bullet typing (pattern just completed)
    const currentLine = this.getCurrentLine(content, cursorPosition);
    const bulletRegex = /^(\s*)([-*+])\s(.*)$/;
    const match = currentLine.trim().match(bulletRegex);
    
    return match !== null;
  }
  
  /**
   * Handle undo/correction scenarios
   */
  undoBulletConversion(
    originalContent: string,
    convertedNodes: TreeNodeData[]
  ): string {
    // Reconstruct bullet syntax from node hierarchy
    let restoredContent = originalContent;
    
    for (const node of convertedNodes) {
      const bulletSyntax = this.getBulletSyntaxForDepth(node.depth);
      const bulletLine = `${bulletSyntax}${node.content}`;
      
      // Find appropriate insertion point and restore bullet
      // This is a simplified restoration - in practice would need more sophisticated logic
      restoredContent += `\n${bulletLine}`;
    }
    
    return restoredContent;
  }
  
  /**
   * Private helper methods
   */
  
  private analyzeBulletStructure(content: string, patterns: MarkdownPattern[]): BulletMatch[] {
    const lines = content.split('\n');
    const matches: BulletMatch[] = [];
    
    for (const pattern of patterns) {
      const lineNumber = pattern.line;
      const lineContent = lines[lineNumber] || '';
      
      // Calculate indentation level - each 2 spaces = 1 level of nesting
      const leadingWhitespace = lineContent.match(/^(\s*)/)?.[1] || '';
      const indentLevel = Math.floor(leadingWhitespace.length / 2);
      
      // Extract clean content (without bullet syntax)
      const cleanContent = pattern.content.trim();
      
      matches.push({
        pattern,
        indentLevel: Math.min(indentLevel, this.config.maxDepth),
        cleanContent,
        lineNumber
      });
    }
    
    // Sort by line number to maintain order
    return matches.sort((a, b) => a.lineNumber - b.lineNumber);
  }
  
  private buildNodeHierarchy(matches: BulletMatch[], parentNodeId?: string): TreeNodeData[] {
    const nodes: TreeNodeData[] = [];
    const nodeStack: TreeNodeData[] = []; // Track parent nodes at each level
    
    for (const match of matches) {
      const nodeId = this.generateNodeId();
      
      // Determine parent based on indentation level
      let actualParentId = parentNodeId;
      let targetParent: TreeNodeData | undefined;
      
      if (match.indentLevel > 0 && nodeStack.length > 0) {
        // Find parent at the correct level (one level up)
        const parentLevel = match.indentLevel - 1;
        targetParent = nodeStack[parentLevel];
        
        if (targetParent) {
          actualParentId = targetParent.id;
        }
      }
      
      const newNode = this.createNodeData(nodeId, match, actualParentId);
      
      // Add to parent's children or root level
      if (targetParent) {
        targetParent.hasChildren = true;
        targetParent.children.push(newNode);
      } else {
        // Root level node
        nodes.push(newNode);
      }
      
      // Update node stack for this indentation level
      nodeStack[match.indentLevel] = newNode;
      
      // Clear deeper levels from stack
      nodeStack.splice(match.indentLevel + 1);
    }
    
    return nodes;
  }
  
  private createNodeData(
    nodeId: string, 
    match: BulletMatch, 
    parentId: string | undefined
  ): TreeNodeData {
    return {
      id: nodeId,
      title: match.cleanContent.substring(0, 50), // First 50 chars as title
      content: match.cleanContent,
      nodeType: this.config.defaultNodeType,
      depth: match.indentLevel,
      parentId: parentId || null,
      children: [],
      expanded: true,
      hasChildren: false
    };
  }
  
  private removeBulletSyntax(content: string, patterns: MarkdownPattern[]): string {
    let cleanContent = content;
    
    // Sort patterns by start position in reverse order to maintain positions during removal
    const sortedPatterns = [...patterns].sort((a, b) => b.start - a.start);
    
    for (const pattern of sortedPatterns) {
      // Find the line containing this bullet
      const lines = cleanContent.split('\n');
      const line = lines[pattern.line];
      
      if (line) {
        // Remove just the bullet syntax (e.g., '- ', '* ', '+ ') from the line
        const bulletRegex = new RegExp(`^(\\s*)([-*+])\\s`);
        const cleanedLine = line.replace(bulletRegex, '$1'); // Keep indentation, remove bullet and space
        
        lines[pattern.line] = cleanedLine;
        cleanContent = lines.join('\n');
      }
    }
    
    return cleanContent;
  }
  
  private calculateNewCursorPosition(
    originalContent: string,
    originalPosition: number,
    patterns: MarkdownPattern[]
  ): number {
    let newPosition = originalPosition;
    
    // Adjust position based on removed bullet syntax
    for (const pattern of patterns) {
      if (pattern.start < originalPosition) {
        // Subtract the length of removed syntax
        newPosition -= pattern.syntax.length;
      }
    }
    
    return Math.max(0, newPosition);
  }
  
  private getCurrentLine(content: string, position: number): string {
    const lines = content.split('\n');
    let currentPos = 0;
    
    for (const line of lines) {
      if (position <= currentPos + line.length) {
        return line;
      }
      currentPos += line.length + 1; // +1 for newline character
    }
    
    return lines[lines.length - 1] || '';
  }
  
  private getBulletSyntaxForDepth(depth: number): string {
    const bulletTypes = ['- ', '* ', '+ '];
    const bulletType = bulletTypes[depth % bulletTypes.length];
    const indent = ' '.repeat(depth * this.config.indentSize);
    return indent + bulletType;
  }
  
  private generateNodeId(): string {
    // Generate unique node ID - in production would use proper UUID library
    return `node-${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
  }
}

/**
 * Default converter instance with standard configuration
 */
export const bulletToNodeConverter = new BulletToNodeConverter();

/**
 * Specialized converter for task-oriented bullet points
 */
export const taskBulletConverter = new BulletToNodeConverter({
  defaultNodeType: 'task',
  createNewNodes: true,
  preserveBulletSyntax: false
});

/**
 * Utility functions for bullet detection and processing
 */
export class BulletProcessingUtils {
  
  /**
   * Check if current typing context suggests bullet conversion
   */
  static shouldTriggerConversion(
    content: string,
    cursorPosition: number,
    patterns: MarkdownPattern[]
  ): boolean {
    const converter = new BulletToNodeConverter();
    return converter.detectBulletTyping(content, patterns, cursorPosition);
  }
  
  /**
   * Get bullet patterns that would be affected by cursor position
   */
  static getBulletPatternsNearCursor(
    patterns: MarkdownPattern[],
    cursorPosition: number,
    tolerance: number = 10
  ): MarkdownPattern[] {
    const bulletPatterns = patternIntegrationUtils.extractBulletPatterns(patterns);
    
    return bulletPatterns.filter(pattern => {
      const distance = Math.min(
        Math.abs(cursorPosition - pattern.start),
        Math.abs(cursorPosition - pattern.end)
      );
      return distance <= tolerance;
    });
  }
  
  /**
   * Preview what nodes would be created from current bullets
   */
  static previewConversion(
    content: string,
    patterns: MarkdownPattern[],
    parentNodeId?: string
  ): TreeNodeData[] {
    const converter = new BulletToNodeConverter({ createNewNodes: false });
    const result = converter.convertBulletsToNodes(content, patterns, 0, parentNodeId);
    return result.newNodes;
  }
}