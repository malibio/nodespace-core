/**
 * AI Integration Service for NodeSpace
 * 
 * Comprehensive markdown import/export functionality that enables seamless AI integration,
 * allowing users to export NodeSpace content as markdown for AI processing (ChatGPT, Claude)
 * and import AI-generated markdown back into the NodeSpace hierarchy.
 * 
 * Integrates with:
 * - Pattern detection from Issue #56 for parsing
 * - Bullet-to-node conversion from Issue #58
 * - Soft newline intelligence from Issue #59
 * - WYSIWYG processing from Issue #57 for imported content
 * - ContentEditable foundation from Issue #55
 */

import type { TreeNodeData } from '$lib/types/tree';
import type { 
  MarkdownPattern, 
  MarkdownPatternType,
  PatternDetectionOptions 
} from '$lib/types/markdownPatterns';
import { markdownPatternDetector } from './markdownPatternDetector';
import { bulletToNodeConverter } from './bulletToNodeConverter';
import { softNewlineProcessor } from './softNewlineProcessor';
import { wysiwygProcessor } from './wysiwygProcessor';

/**
 * Configuration for AI integration operations
 */
export interface AIIntegrationConfig {
  /** Include node metadata in export */
  includeMetadata: boolean;
  /** Preserve node IDs during round-trip */
  preserveNodeIds: boolean;
  /** Maximum depth to export/import */
  maxDepth: number;
  /** Format style for markdown export */
  exportStyle: 'standard' | 'ai-optimized' | 'compact';
  /** Handle soft newlines during import */
  processSoftNewlines: boolean;
  /** Apply WYSIWYG processing to imported content */
  enableWYSIWYG: boolean;
  /** Validation level for imported content */
  validationLevel: 'strict' | 'moderate' | 'permissive';
  /** Clean up AI-specific markdown patterns */
  cleanAIPatterns: boolean;
}

/**
 * Result of export operation
 */
export interface ExportResult {
  /** Generated markdown content */
  markdown: string;
  /** Export metadata for round-trip validation */
  metadata: ExportMetadata;
  /** Export statistics */
  stats: ExportStats;
  /** Any warnings during export */
  warnings: string[];
}

/**
 * Result of import operation
 */
export interface ImportResult {
  /** Generated node hierarchy */
  nodes: TreeNodeData[];
  /** Import statistics */
  stats: ImportStats;
  /** Detected content patterns */
  patterns: MarkdownPattern[];
  /** Any warnings during import */
  warnings: string[];
  /** Validation results */
  validation: ValidationResult;
}

/**
 * Export metadata for validation
 */
export interface ExportMetadata {
  /** Original node count */
  nodeCount: number;
  /** Maximum depth exported */
  maxDepth: number;
  /** Export timestamp */
  timestamp: number;
  /** NodeSpace version */
  version: string;
  /** Configuration used */
  config: AIIntegrationConfig;
  /** Node ID mapping for round-trip */
  nodeIdMap: Map<string, string>;
}

/**
 * Export statistics
 */
export interface ExportStats {
  /** Total nodes processed */
  nodesProcessed: number;
  /** Total characters exported */
  charactersExported: number;
  /** Processing time in milliseconds */
  processingTime: number;
  /** Patterns converted to markdown */
  patternsConverted: number;
  /** Lines generated */
  linesGenerated: number;
}

/**
 * Import statistics
 */
export interface ImportStats {
  /** Total nodes created */
  nodesCreated: number;
  /** Total characters imported */
  charactersImported: number;
  /** Processing time in milliseconds */
  processingTime: number;
  /** Patterns detected and processed */
  patternsProcessed: number;
  /** Lines processed */
  linesProcessed: number;
  /** Bullet conversions performed */
  bulletConversions: number;
  /** WYSIWYG processing applied */
  wysiwygProcessed: boolean;
}

/**
 * Validation result for imported content
 */
export interface ValidationResult {
  /** Whether content passed validation */
  isValid: boolean;
  /** Validation errors */
  errors: string[];
  /** Validation warnings */
  warnings: string[];
  /** Suggested fixes */
  suggestions: string[];
  /** Round-trip integrity score (0-1) */
  integrityScore: number;
}

/**
 * AI-specific markdown patterns that need special handling
 */
export interface AIMarkdownPatterns {
  /** Claude-style thinking blocks */
  thinkingBlocks: RegExp;
  /** ChatGPT code block variations */
  codeBlockVariations: RegExp;
  /** AI-generated bullet variations */
  aiBulletPatterns: RegExp;
  /** Numbered list patterns */
  numberedLists: RegExp;
  /** AI response formatting */
  aiResponseFormat: RegExp;
}

/**
 * Default configuration for AI integration
 */
const DEFAULT_CONFIG: AIIntegrationConfig = {
  includeMetadata: false,
  preserveNodeIds: true,
  maxDepth: 10,
  exportStyle: 'ai-optimized',
  processSoftNewlines: true,
  enableWYSIWYG: true,
  validationLevel: 'moderate',
  cleanAIPatterns: true
};

/**
 * AI-specific patterns for cleanup and processing
 */
const AI_PATTERNS: AIMarkdownPatterns = {
  thinkingBlocks: /<thinking>[\s\S]*?<\/thinking>/g,
  codeBlockVariations: /```(\w+)?\n?([\s\S]*?)```/g,
  aiBulletPatterns: /^(\s*)([-•▪▫▸▹‣⁃∘⦿⦾])\s+(.+)$/gm,
  numberedLists: /^(\s*)(\d+[.)]\s+)(.+)$/gm,
  aiResponseFormat: /^(AI|Assistant|Claude|GPT):\s*/gm
};

/**
 * Main AI Integration Service
 */
export class AIIntegrationService {
  private config: AIIntegrationConfig;
  private lastExportMetadata: ExportMetadata | null = null;

  constructor(config: Partial<AIIntegrationConfig> = {}) {
    this.config = { ...DEFAULT_CONFIG, ...config };
  }

  /**
   * Export NodeSpace hierarchy to AI-optimized markdown
   */
  async exportToMarkdown(
    nodes: TreeNodeData[], 
    config: Partial<AIIntegrationConfig> = {}
  ): Promise<ExportResult> {
    const startTime = performance.now();
    const mergedConfig = { ...this.config, ...config };
    const warnings: string[] = [];
    let markdown = '';
    let nodesProcessed = 0;
    let patternsConverted = 0;
    let linesGenerated = 0;
    const nodeIdMap = new Map<string, string>();

    try {
      // Build markdown content recursively
      const markdownLines: string[] = [];
      
      for (const node of nodes) {
        const nodeMarkdown = await this.exportNodeToMarkdown(
          node, 
          0, 
          mergedConfig, 
          nodeIdMap, 
          warnings
        );
        
        if (nodeMarkdown.trim()) {
          markdownLines.push(nodeMarkdown);
          nodesProcessed += 1 + this.countNodes(node); // Count the node itself plus children
        }
      }

      markdown = markdownLines.join('\n\n');
      linesGenerated = markdown.split('\n').length;

      // Apply style-specific formatting
      markdown = this.applyExportStyle(markdown, mergedConfig.exportStyle);

      // Clean up AI-specific patterns if requested
      if (mergedConfig.cleanAIPatterns) {
        const cleanupResult = this.cleanAIPatterns(markdown);
        markdown = cleanupResult.markdown;
        patternsConverted += cleanupResult.patternsRemoved;
      }

      // Create export metadata
      const metadata: ExportMetadata = {
        nodeCount: nodesProcessed,
        maxDepth: this.calculateMaxDepth(nodes),
        timestamp: Date.now(),
        version: '1.0.0', // NodeSpace version
        config: mergedConfig,
        nodeIdMap
      };

      this.lastExportMetadata = metadata;

      const processingTime = performance.now() - startTime;

      return {
        markdown,
        metadata,
        stats: {
          nodesProcessed,
          charactersExported: markdown.length,
          processingTime,
          patternsConverted,
          linesGenerated
        },
        warnings
      };

    } catch (error) {
      warnings.push(`Export error: ${error instanceof Error ? error.message : 'Unknown error'}`);
      
      return {
        markdown: '',
        metadata: this.createEmptyMetadata(mergedConfig),
        stats: {
          nodesProcessed: 0,
          charactersExported: 0,
          processingTime: performance.now() - startTime,
          patternsConverted: 0,
          linesGenerated: 0
        },
        warnings
      };
    }
  }

  /**
   * Import AI-generated markdown into NodeSpace hierarchy
   */
  async importFromMarkdown(
    markdown: string,
    config: Partial<AIIntegrationConfig> = {}
  ): Promise<ImportResult> {
    const startTime = performance.now();
    const mergedConfig = { ...this.config, ...config };
    const warnings: string[] = [];
    let nodes: TreeNodeData[] = [];
    let patterns: MarkdownPattern[] = [];
    let bulletConversions = 0;
    let wysiwygProcessed = false;

    try {
      // Pre-process markdown for AI-specific patterns
      let processedMarkdown = markdown;
      if (mergedConfig.cleanAIPatterns) {
        const cleanupResult = this.cleanAIPatterns(processedMarkdown);
        processedMarkdown = cleanupResult.markdown;
        warnings.push(...cleanupResult.warnings);
      }

      // Detect all markdown patterns using Issue #56 API
      const detectionResult = markdownPatternDetector.detectPatterns(
        processedMarkdown,
        this.getDetectionOptions(mergedConfig)
      );
      
      patterns = detectionResult.patterns;
      warnings.push(...detectionResult.warnings);

      // Convert structure to nodes using various services
      const conversionResult = await this.convertPatternsToNodes(
        processedMarkdown,
        patterns,
        mergedConfig,
        warnings
      );

      nodes = conversionResult.nodes;
      bulletConversions = conversionResult.bulletConversions;

      // Apply soft newline processing if requested
      if (mergedConfig.processSoftNewlines) {
        nodes = await this.applySoftNewlineProcessing(nodes, warnings);
      }

      // Apply WYSIWYG processing to content if requested
      if (mergedConfig.enableWYSIWYG) {
        nodes = await this.applyWYSIWYGProcessing(nodes, warnings);
        wysiwygProcessed = true;
      }

      // Validate imported content
      const validation = this.validateImportedContent(
        nodes,
        patterns,
        mergedConfig
      );

      if (!validation.isValid && mergedConfig.validationLevel === 'strict') {
        throw new Error(`Validation failed: ${validation.errors.join(', ')}`);
      }

      warnings.push(...validation.warnings);

      const processingTime = performance.now() - startTime;

      return {
        nodes,
        stats: {
          nodesCreated: this.countNodesInArray(nodes),
          charactersImported: markdown.length,
          processingTime,
          patternsProcessed: patterns.length,
          linesProcessed: markdown.split('\n').length,
          bulletConversions,
          wysiwygProcessed
        },
        patterns,
        warnings,
        validation
      };

    } catch (error) {
      warnings.push(`Import error: ${error instanceof Error ? error.message : 'Unknown error'}`);

      return {
        nodes: [],
        stats: {
          nodesCreated: 0,
          charactersImported: 0,
          processingTime: performance.now() - startTime,
          patternsProcessed: 0,
          linesProcessed: 0,
          bulletConversions: 0,
          wysiwygProcessed: false
        },
        patterns: [],
        warnings,
        validation: {
          isValid: false,
          errors: ['Import failed'],
          warnings: [],
          suggestions: [],
          integrityScore: 0
        }
      };
    }
  }

  /**
   * Round-trip validation: export → import → compare
   */
  async validateRoundTrip(
    originalNodes: TreeNodeData[],
    config: Partial<AIIntegrationConfig> = {}
  ): Promise<{
    isValid: boolean;
    exportResult: ExportResult;
    importResult: ImportResult;
    integrityScore: number;
    differences: string[];
  }> {
    // Export nodes to markdown
    const exportResult = await this.exportToMarkdown(originalNodes, config);
    
    // Import markdown back to nodes
    const importResult = await this.importFromMarkdown(
      exportResult.markdown, 
      config
    );

    // Compare structures
    const comparison = this.compareNodeStructures(
      originalNodes, 
      importResult.nodes
    );

    return {
      isValid: comparison.integrityScore > 0.8,
      exportResult,
      importResult,
      integrityScore: comparison.integrityScore,
      differences: comparison.differences
    };
  }

  /**
   * Get export metadata from last operation
   */
  getLastExportMetadata(): ExportMetadata | null {
    return this.lastExportMetadata;
  }

  /**
   * Update service configuration
   */
  updateConfig(config: Partial<AIIntegrationConfig>): void {
    this.config = { ...this.config, ...config };
  }

  /**
   * Get current configuration
   */
  getConfig(): AIIntegrationConfig {
    return { ...this.config };
  }

  /**
   * Private helper methods
   */

  private async exportNodeToMarkdown(
    node: TreeNodeData,
    depth: number,
    config: AIIntegrationConfig,
    nodeIdMap: Map<string, string>,
    warnings: string[]
  ): Promise<string> {
    if (depth > config.maxDepth) {
      warnings.push(`Node at depth ${depth} exceeds maxDepth ${config.maxDepth}`);
      return '';
    }

    const lines: string[] = [];
    const indent = '  '.repeat(depth); // 2 spaces per level

    // Map node ID if needed
    if (config.preserveNodeIds) {
      nodeIdMap.set(node.id, node.id);
    }

    // Handle different node types with appropriate markdown
    switch (node.nodeType) {
      case 'text':
        if (node.title && node.title !== node.content) {
          // Title as header, content as body
          const headerLevel = Math.min(depth + 1, 6);
          const headerPrefix = '#'.repeat(headerLevel);
          lines.push(`${headerPrefix} ${node.title}`);
          if (node.content.trim()) {
            lines.push('', node.content);
          }
        } else {
          // Just content, potentially as bullet if nested
          if (depth > 0) {
            lines.push(`${indent}- ${node.content}`);
          } else {
            lines.push(node.content);
          }
        }
        break;

      case 'task':
        const taskPrefix = depth > 0 ? `${indent}- [ ] ` : '- [ ] ';
        lines.push(`${taskPrefix}${node.content}`);
        break;

      case 'ai-chat':
        lines.push(`**AI Chat**: ${node.content}`);
        break;

      case 'entity':
        lines.push(`**${node.title}**: ${node.content}`);
        break;

      case 'query':
        lines.push(`> **Query**: ${node.content}`);
        break;

      default:
        // Fallback to bullet format for unknown types
        if (depth > 0) {
          lines.push(`${indent}- ${node.content}`);
        } else {
          lines.push(node.content);
        }
    }

    // Process children recursively
    if (node.children && node.children.length > 0) {
      for (const child of node.children) {
        const childMarkdown = await this.exportNodeToMarkdown(
          child,
          depth + 1,
          config,
          nodeIdMap,
          warnings
        );
        
        if (childMarkdown.trim()) {
          lines.push('', childMarkdown);
        }
      }
    }

    return lines.join('\n');
  }

  private applyExportStyle(markdown: string, style: string): string {
    switch (style) {
      case 'ai-optimized':
        // Format for AI consumption: clear headers, consistent bullets
        return markdown
          .replace(/^(#{1,6})\s+/gm, (match, hashes) => `${hashes} `)
          .replace(/^\s*[-*+]\s+/gm, '- ')
          .replace(/\n{3,}/g, '\n\n'); // Normalize spacing

      case 'compact':
        // Minimal formatting for space efficiency
        return markdown
          .replace(/\n{2,}/g, '\n')
          .replace(/^#+\s+/gm, '**')
          .replace(/\*\*([^*]+)$/gm, '**$1**');

      case 'standard':
      default:
        return markdown;
    }
  }

  private cleanAIPatterns(markdown: string): { 
    markdown: string; 
    patternsRemoved: number; 
    warnings: string[] 
  } {
    let cleanMarkdown = markdown;
    let patternsRemoved = 0;
    const warnings: string[] = [];

    // Remove AI thinking blocks
    const thinkingMatches = cleanMarkdown.match(AI_PATTERNS.thinkingBlocks);
    if (thinkingMatches) {
      cleanMarkdown = cleanMarkdown.replace(AI_PATTERNS.thinkingBlocks, '');
      patternsRemoved += thinkingMatches.length;
      warnings.push(`Removed ${thinkingMatches.length} AI thinking blocks`);
    }

    // Normalize AI response prefixes
    const responseMatches = cleanMarkdown.match(AI_PATTERNS.aiResponseFormat);
    if (responseMatches) {
      cleanMarkdown = cleanMarkdown.replace(AI_PATTERNS.aiResponseFormat, '');
      patternsRemoved += responseMatches.length;
    }

    // Normalize bullet patterns
    cleanMarkdown = cleanMarkdown.replace(AI_PATTERNS.aiBulletPatterns, '$1- $3');

    // Convert numbered lists to bullets (optional, based on config)
    cleanMarkdown = cleanMarkdown.replace(AI_PATTERNS.numberedLists, '$1- $3');

    return {
      markdown: cleanMarkdown.trim(),
      patternsRemoved,
      warnings
    };
  }

  private async convertPatternsToNodes(
    markdown: string,
    patterns: MarkdownPattern[],
    config: AIIntegrationConfig,
    warnings: string[]
  ): Promise<{ nodes: TreeNodeData[]; bulletConversions: number }> {
    let bulletConversions = 0;
    
    // First, try to convert using bullet-to-node converter for bullet patterns
    const bulletPatterns = patterns.filter(p => p.type === 'bullet');
    if (bulletPatterns.length > 0) {
      const conversionResult = bulletToNodeConverter.convertBulletsToNodes(
        markdown,
        bulletPatterns,
        0 // cursor position not relevant for import
      );
      
      if (conversionResult.hasConversions) {
        bulletConversions = conversionResult.newNodes.length;
        return {
          nodes: conversionResult.newNodes,
          bulletConversions
        };
      }
    }

    // Fallback: manual parsing for complex structures
    const nodes = await this.parseMarkdownToNodes(markdown, patterns, config, warnings);
    
    return {
      nodes,
      bulletConversions: 0
    };
  }

  private async parseMarkdownToNodes(
    markdown: string,
    patterns: MarkdownPattern[],
    config: AIIntegrationConfig,
    warnings: string[]
  ): Promise<TreeNodeData[]> {
    const lines = markdown.split('\n');
    const nodes: TreeNodeData[] = [];
    const nodeStack: Array<{ node: TreeNodeData; level: number }> = [];

    let currentNodeId = 0;
    const generateId = () => `imported-node-${++currentNodeId}`;

    for (let i = 0; i < lines.length; i++) {
      const line = lines[i];
      const trimmedLine = line.trim();
      
      if (!trimmedLine) continue;

      // Find patterns on this line
      const linePatterns = patterns.filter(p => p.line === i);
      
      // Determine node properties based on patterns and content
      const nodeData = this.parseLineToNode(
        line,
        linePatterns,
        generateId(),
        config
      );

      if (nodeData) {
        // Determine hierarchy level
        const indentLevel = this.calculateIndentLevel(line);
        const targetLevel = Math.min(indentLevel, config.maxDepth);

        // Adjust node stack to current level
        while (nodeStack.length > targetLevel) {
          nodeStack.pop();
        }

        // Add to parent or root
        if (nodeStack.length > 0) {
          const parent = nodeStack[nodeStack.length - 1];
          parent.node.children.push(nodeData);
          parent.node.hasChildren = true;
          nodeData.parentId = parent.node.id;
          nodeData.depth = targetLevel;
        } else {
          nodes.push(nodeData);
          nodeData.depth = 0;
        }

        // Add to stack if it could have children
        nodeStack.push({ node: nodeData, level: targetLevel });
      }
    }

    return nodes;
  }

  private parseLineToNode(
    line: string,
    patterns: MarkdownPattern[],
    nodeId: string,
    config: AIIntegrationConfig
  ): TreeNodeData | null {
    const trimmedLine = line.trim();
    
    if (!trimmedLine) return null;

    // Determine node type and content based on patterns
    let nodeType: TreeNodeData['nodeType'] = 'text';
    let content = trimmedLine;
    let title = '';

    // Process patterns to extract content and determine type
    for (const pattern of patterns) {
      switch (pattern.type) {
        case 'header':
          nodeType = 'text';
          title = pattern.content;
          content = pattern.content;
          break;

        case 'bullet':
          nodeType = 'text';
          content = pattern.content;
          break;

        case 'blockquote':
          nodeType = 'query'; // Treat blockquotes as queries
          content = pattern.content;
          break;

        case 'codeblock':
          nodeType = 'text';
          content = `\`\`\`${pattern.language || ''}\n${pattern.content}\n\`\`\``;
          break;
      }
    }

    // Check for task pattern (including checkbox patterns)
    const taskMatch = trimmedLine.match(/^[-*+]\s*\[\s*[x ]?\s*\]\s*(.+)$/i);
    if (taskMatch) {
      nodeType = 'task';
      content = taskMatch[1];
    }

    // Clean up content from bullet syntax if no patterns found
    if (patterns.length === 0) {
      const bulletMatch = trimmedLine.match(/^[-*+]\s+(.+)$/);
      if (bulletMatch) {
        content = bulletMatch[1];
      }
      
      // Check for header patterns even without detected patterns
      const headerMatch = trimmedLine.match(/^(#{1,6})\s+(.+)$/);
      if (headerMatch) {
        title = headerMatch[2];
        content = headerMatch[2];
      }
    }

    // Always create a node for non-empty content
    return {
      id: nodeId,
      title: title || content.substring(0, 50),
      content,
      nodeType,
      depth: 0, // Will be set by caller
      parentId: null,
      children: [],
      expanded: true,
      hasChildren: false
    };
  }

  private calculateIndentLevel(line: string): number {
    const leadingWhitespace = line.match(/^(\s*)/)?.[1] || '';
    return Math.floor(leadingWhitespace.length / 2); // 2 spaces per level
  }

  private async applySoftNewlineProcessing(
    nodes: TreeNodeData[],
    warnings: string[]
  ): Promise<TreeNodeData[]> {
    const processedNodes: TreeNodeData[] = [];

    for (const node of nodes) {
      const processedContent = softNewlineProcessor.processSoftNewlines(
        node.content,
        0 // cursor position not relevant
      );

      const processedNode: TreeNodeData = {
        ...node,
        content: processedContent.cleanedContent,
        children: node.children.length > 0 
          ? await this.applySoftNewlineProcessing(node.children, warnings)
          : []
      };

      processedNodes.push(processedNode);
    }

    return processedNodes;
  }

  private async applyWYSIWYGProcessing(
    nodes: TreeNodeData[],
    warnings: string[]
  ): Promise<TreeNodeData[]> {
    const processedNodes: TreeNodeData[] = [];

    for (const node of nodes) {
      // Process content through WYSIWYG processor
      const processingResult = wysiwygProcessor.processContent(
        node.content,
        0, // cursor position not relevant for import
        { enableRealTime: false } // Process once, not real-time
      );

      const processedNode: TreeNodeData = {
        ...node,
        content: node.content, // Keep original content, WYSIWYG is for display
        children: node.children.length > 0 
          ? await this.applyWYSIWYGProcessing(node.children, warnings)
          : []
      };

      processedNodes.push(processedNode);
    }

    return processedNodes;
  }

  private validateImportedContent(
    nodes: TreeNodeData[],
    patterns: MarkdownPattern[],
    config: AIIntegrationConfig
  ): ValidationResult {
    const errors: string[] = [];
    const warnings: string[] = [];
    const suggestions: string[] = [];

    // Validate node structure
    const nodeCount = this.countNodesInArray(nodes);
    if (nodeCount === 0) {
      errors.push('No nodes were created from imported content');
    }

    // Check for valid node types
    const invalidNodes = this.findInvalidNodes(nodes);
    if (invalidNodes.length > 0) {
      warnings.push(`Found ${invalidNodes.length} nodes with invalid types`);
    }

    // Validate against original metadata if available
    let integrityScore = 1.0;
    if (this.lastExportMetadata) {
      const expectedCount = this.lastExportMetadata.nodeCount;
      const actualCount = nodeCount;
      integrityScore = actualCount / Math.max(expectedCount, 1);
      
      if (integrityScore < 0.9) {
        warnings.push(
          `Node count mismatch: expected ${expectedCount}, got ${actualCount}`
        );
      }
    }

    // Check maximum depth
    const maxDepth = this.calculateMaxDepth(nodes);
    if (maxDepth > config.maxDepth) {
      warnings.push(`Content depth ${maxDepth} exceeds configured maximum ${config.maxDepth}`);
      suggestions.push('Consider increasing maxDepth in configuration');
    }

    const isValid = errors.length === 0;

    return {
      isValid,
      errors,
      warnings,
      suggestions,
      integrityScore: Math.max(0, Math.min(1, integrityScore))
    };
  }

  private compareNodeStructures(
    original: TreeNodeData[],
    imported: TreeNodeData[]
  ): { integrityScore: number; differences: string[] } {
    const differences: string[] = [];
    
    const originalCount = this.countNodesInArray(original);
    const importedCount = this.countNodesInArray(imported);
    
    let integrityScore = importedCount / Math.max(originalCount, 1);

    if (originalCount !== importedCount) {
      differences.push(`Node count: original ${originalCount}, imported ${importedCount}`);
    }

    const originalDepth = this.calculateMaxDepth(original);
    const importedDepth = this.calculateMaxDepth(imported);
    
    if (originalDepth !== importedDepth) {
      differences.push(`Max depth: original ${originalDepth}, imported ${importedDepth}`);
      integrityScore *= 0.9;
    }

    return {
      integrityScore: Math.max(0, Math.min(1, integrityScore)),
      differences
    };
  }

  private getDetectionOptions(config: AIIntegrationConfig): PatternDetectionOptions {
    return {
      detectHeaders: true,
      detectBullets: true,
      detectBlockquotes: true,
      detectCodeBlocks: true,
      detectBold: true,
      detectItalic: true,
      detectInlineCode: true,
      maxHeaderLevel: 6,
      includePositions: true,
      performanceMode: config.validationLevel === 'permissive'
    };
  }

  private countNodes(node: { children: TreeNodeData[] }): number {
    if (!node.children || node.children.length === 0) {
      return 0;
    }
    
    let count = node.children.length;
    for (const child of node.children) {
      count += this.countNodes(child);
    }
    return count;
  }
  
  private countNodesInArray(nodes: TreeNodeData[]): number {
    let count = nodes.length;
    for (const node of nodes) {
      count += this.countNodes(node);
    }
    return count;
  }

  private calculateMaxDepth(nodes: TreeNodeData[]): number {
    let maxDepth = 0;
    
    for (const node of nodes) {
      const nodeDepth = node.depth || 0;
      maxDepth = Math.max(maxDepth, nodeDepth);
      
      if (node.children && node.children.length > 0) {
        maxDepth = Math.max(maxDepth, this.calculateMaxDepth(node.children));
      }
    }
    
    return maxDepth;
  }

  private findInvalidNodes(nodes: TreeNodeData[]): TreeNodeData[] {
    const invalid: TreeNodeData[] = [];
    const validTypes = ['text', 'task', 'ai-chat', 'entity', 'query'];
    
    for (const node of nodes) {
      if (!validTypes.includes(node.nodeType)) {
        invalid.push(node);
      }
      
      if (node.children && node.children.length > 0) {
        invalid.push(...this.findInvalidNodes(node.children));
      }
    }
    
    return invalid;
  }

  private createEmptyMetadata(config: AIIntegrationConfig): ExportMetadata {
    return {
      nodeCount: 0,
      maxDepth: 0,
      timestamp: Date.now(),
      version: '1.0.0',
      config,
      nodeIdMap: new Map()
    };
  }
}

/**
 * Default AI integration service instance
 */
export const aiIntegrationService = new AIIntegrationService();

/**
 * Specialized service for ChatGPT integration
 */
export const chatGPTIntegrationService = new AIIntegrationService({
  exportStyle: 'ai-optimized',
  cleanAIPatterns: true,
  validationLevel: 'moderate',
  processSoftNewlines: true,
  enableWYSIWYG: true
});

/**
 * Specialized service for Claude integration
 */
export const claudeIntegrationService = new AIIntegrationService({
  exportStyle: 'standard',
  cleanAIPatterns: true,
  validationLevel: 'strict',
  processSoftNewlines: true,
  enableWYSIWYG: true,
  includeMetadata: true
});

/**
 * Utility functions for AI integration workflows
 */
export class AIIntegrationUtils {
  
  /**
   * Quick export for AI chat workflows
   */
  static async quickExport(nodes: TreeNodeData[], aiType: 'chatgpt' | 'claude' = 'chatgpt'): Promise<string> {
    const service = aiType === 'claude' ? claudeIntegrationService : chatGPTIntegrationService;
    const result = await service.exportToMarkdown(nodes);
    return result.markdown;
  }

  /**
   * Quick import for AI response processing
   */
  static async quickImport(markdown: string, aiType: 'chatgpt' | 'claude' = 'chatgpt'): Promise<TreeNodeData[]> {
    const service = aiType === 'claude' ? claudeIntegrationService : chatGPTIntegrationService;
    const result = await service.importFromMarkdown(markdown);
    return result.nodes;
  }

  /**
   * Validate markdown before import
   */
  static async validateMarkdown(markdown: string): Promise<{
    isValid: boolean;
    patterns: MarkdownPattern[];
    warnings: string[];
  }> {
    const detectionResult = markdownPatternDetector.detectPatterns(markdown);
    
    const warnings: string[] = [];
    if (detectionResult.warnings.length > 0) {
      warnings.push(...detectionResult.warnings);
    }

    // Check for common AI-specific issues
    if (markdown.includes('<thinking>')) {
      warnings.push('Contains AI thinking blocks that will be removed');
    }

    if (markdown.match(/^(AI|Assistant|Claude|GPT):/m)) {
      warnings.push('Contains AI response prefixes that will be cleaned');
    }

    return {
      isValid: detectionResult.patterns.length > 0 || markdown.trim().length > 0,
      patterns: detectionResult.patterns,
      warnings
    };
  }

  /**
   * Get recommended configuration for specific AI workflows
   */
  static getRecommendedConfig(workflow: 'chat' | 'content-generation' | 'analysis'): Partial<AIIntegrationConfig> {
    switch (workflow) {
      case 'chat':
        return {
          exportStyle: 'ai-optimized',
          cleanAIPatterns: true,
          validationLevel: 'moderate',
          maxDepth: 5
        };

      case 'content-generation':
        return {
          exportStyle: 'standard',
          cleanAIPatterns: false,
          validationLevel: 'permissive',
          enableWYSIWYG: true,
          maxDepth: 10
        };

      case 'analysis':
        return {
          exportStyle: 'compact',
          includeMetadata: true,
          validationLevel: 'strict',
          preserveNodeIds: true
        };

      default:
        return {};
    }
  }
}