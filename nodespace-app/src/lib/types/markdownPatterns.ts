/**
 * Markdown Pattern Detection Types
 * 
 * Type definitions for comprehensive markdown pattern detection system.
 * Supports 4 block types and 3 inline types with position information
 * for cursor management and real-time processing.
 */

/**
 * Core markdown pattern types
 */
export type MarkdownPatternType = 
  // Block types
  | 'header' 
  | 'bullet' 
  | 'blockquote' 
  | 'codeblock'
  // Inline types  
  | 'bold' 
  | 'italic' 
  | 'inlinecode';

/**
 * Block type specific subtypes
 */
export type HeaderLevel = 1 | 2 | 3 | 4 | 5 | 6;
export type BulletType = '-' | '*' | '+';

/**
 * Core pattern detection result
 */
export interface MarkdownPattern {
  /** Type of markdown pattern detected */
  type: MarkdownPatternType;
  
  /** Start position in the content string (inclusive) */
  start: number;
  
  /** End position in the content string (exclusive) */
  end: number;
  
  /** Original syntax characters (e.g., "##", "**", "`") */
  syntax: string;
  
  /** Content without syntax characters */
  content: string;
  
  /** Header level (1-6) for header patterns */
  level?: HeaderLevel;
  
  /** Bullet character type for bullet patterns */
  bulletType?: BulletType;
  
  /** Language specification for code blocks */
  language?: string;
  
  /** Line number where pattern starts (0-based) */
  line: number;
  
  /** Column position in line where pattern starts (0-based) */
  column: number;
}

/**
 * Pattern detection result with metadata
 */
export interface PatternDetectionResult {
  /** Array of detected patterns, sorted by position */
  patterns: MarkdownPattern[];
  
  /** Total time taken for detection (milliseconds) */
  detectionTime: number;
  
  /** Number of lines processed */
  linesProcessed: number;
  
  /** Content length processed */
  contentLength: number;
  
  /** Any parsing warnings or errors */
  warnings: string[];
}

/**
 * Pattern detection configuration options
 */
export interface PatternDetectionOptions {
  /** Enable header detection (default: true) */
  detectHeaders?: boolean;
  
  /** Enable bullet list detection (default: true) */
  detectBullets?: boolean;
  
  /** Enable blockquote detection (default: true) */
  detectBlockquotes?: boolean;
  
  /** Enable code block detection (default: true) */
  detectCodeBlocks?: boolean;
  
  /** Enable bold text detection (default: true) */
  detectBold?: boolean;
  
  /** Enable italic text detection (default: true) */
  detectItalic?: boolean;
  
  /** Enable inline code detection (default: true) */
  detectInlineCode?: boolean;
  
  /** Maximum header level to detect (1-6, default: 6) */
  maxHeaderLevel?: HeaderLevel;
  
  /** Include position metadata (default: true) */
  includePositions?: boolean;
  
  /** Performance mode - skip expensive validations (default: false) */
  performanceMode?: boolean;
}

/**
 * Pattern replacement/conversion interface
 */
export interface PatternReplacement {
  /** Original pattern to replace */
  pattern: MarkdownPattern;
  
  /** New content to replace with */
  replacement: string;
  
  /** Whether to preserve original pattern syntax */
  preserveSyntax?: boolean;
}

/**
 * Cursor position information for pattern operations
 */
export interface CursorPosition {
  /** Absolute position in content */
  position: number;
  
  /** Line number (0-based) */
  line: number;
  
  /** Column in line (0-based) */
  column: number;
  
  /** Whether cursor is at start of pattern */
  atPatternStart?: boolean;
  
  /** Whether cursor is at end of pattern */
  atPatternEnd?: boolean;
  
  /** Pattern cursor is currently within (if any) */
  currentPattern?: MarkdownPattern;
}

/**
 * Real-time detection event interface
 */
export interface PatternDetectionEvent {
  /** Type of event */
  type: 'patterns_detected' | 'patterns_changed' | 'pattern_added' | 'pattern_removed';
  
  /** Current patterns after the event */
  patterns: MarkdownPattern[];
  
  /** Patterns that were added (for change events) */
  addedPatterns?: MarkdownPattern[];
  
  /** Patterns that were removed (for change events) */
  removedPatterns?: MarkdownPattern[];
  
  /** Content that triggered the event */
  content: string;
  
  /** Cursor position during the event */
  cursorPosition?: CursorPosition;
  
  /** Timestamp of the event */
  timestamp: number;
}

/**
 * Pattern validation result
 */
export interface PatternValidation {
  /** Whether pattern is valid */
  isValid: boolean;
  
  /** Validation errors */
  errors: string[];
  
  /** Validation warnings */
  warnings: string[];
  
  /** Suggested fixes */
  suggestions: string[];
}

/**
 * Performance metrics for pattern detection
 */
export interface DetectionMetrics {
  /** Time for block pattern detection (ms) */
  blockDetectionTime: number;
  
  /** Time for inline pattern detection (ms) */
  inlineDetectionTime: number;
  
  /** Total detection time (ms) */
  totalTime: number;
  
  /** Number of regex operations performed */
  regexOperations: number;
  
  /** Content length processed */
  contentLength: number;
  
  /** Patterns detected per millisecond */
  patternsPerMs: number;
}

/**
 * Main pattern detector interface
 */
export interface IMarkdownPatternDetector {
  /** Detect all patterns in content */
  detectPatterns(content: string, options?: PatternDetectionOptions): PatternDetectionResult;
  
  /** Detect patterns in real-time with cursor information */
  detectPatternsRealtime(content: string, cursorPosition: number, options?: PatternDetectionOptions): PatternDetectionResult;
  
  /** Get pattern at specific position */
  getPatternAt(content: string, position: number): MarkdownPattern | null;
  
  /** Get all patterns of specific type */
  getPatternsByType(content: string, type: MarkdownPatternType): MarkdownPattern[];
  
  /** Extract content from patterns */
  extractPatternContent(patterns: MarkdownPattern[]): string[];
  
  /** Replace patterns in content */
  replacePatterns(content: string, replacements: PatternReplacement[]): string;
  
  /** Validate pattern syntax */
  validatePattern(pattern: MarkdownPattern): PatternValidation;
  
  /** Get performance metrics for last detection */
  getMetrics(): DetectionMetrics;
  
  /** Subscribe to real-time pattern detection events */
  subscribe(callback: (event: PatternDetectionEvent) => void): () => void;
}

/**
 * Mock data structures for parallel development
 */
export interface MockPatternData {
  /** Sample content with various patterns */
  sampleContent: string;
  
  /** Expected patterns for the sample content */
  expectedPatterns: MarkdownPattern[];
  
  /** Sample cursor positions and expected results */
  cursorScenarios: Array<{
    position: number;
    expectedPattern: MarkdownPattern | null;
    description: string;
  }>;
  
  /** Performance test scenarios */
  performanceScenarios: Array<{
    content: string;
    description: string;
    expectedMaxTime: number;
  }>;
}

/**
 * Integration utilities for other components
 */
export interface PatternIntegrationUtils {
  /** Convert patterns to CSS classes for WYSIWYG */
  toCSSClasses(patterns: MarkdownPattern[]): Record<number, string[]>;
  
  /** Convert patterns to HTML structure for rendering */
  toHTMLStructure(content: string, patterns: MarkdownPattern[]): HTMLElement;
  
  /** Handle cursor positioning around patterns */
  adjustCursorForPatterns(content: string, position: number, patterns: MarkdownPattern[]): number;
  
  /** Extract bullet patterns for node conversion */
  extractBulletPatterns(patterns: MarkdownPattern[]): MarkdownPattern[];
  
  /** Detect soft newline contexts */
  detectSoftNewlineContext(content: string, position: number, patterns: MarkdownPattern[]): boolean;
}

/**
 * Configuration for bullet conversion behavior
 */
export interface BulletConversionConfig {
  /** Create new nodes or update existing content */
  createNewNodes: boolean;
  /** Preserve original bullet syntax in content */
  preserveBulletSyntax: boolean;
  /** Maximum nesting depth to allow */
  maxDepth: number;
  /** Default node type for converted bullets */
  defaultNodeType: 'text' | 'task' | 'ai-chat' | 'entity' | 'query';
  /** Auto-indent pixels per level (matching NodeTree) */
  indentSize: number;
}

/**
 * Result of bullet-to-node conversion operation
 */
export interface BulletConversionResult {
  /** Whether any conversions were performed */
  hasConversions: boolean;
  /** Original content with bullet syntax removed */
  cleanedContent: string;
  /** New node hierarchy created from bullets */
  newNodes: import('$lib/types/tree').TreeNodeData[];
  /** Cursor position after conversion */
  newCursorPosition: number;
  /** Parent node ID if creating child relationships */
  parentNodeId?: string;
}