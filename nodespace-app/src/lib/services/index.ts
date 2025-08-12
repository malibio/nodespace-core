/**
 * Services Module Export Index
 * 
 * Centralized exports for all NodeSpace services including pattern detection,
 * utilities, and mock data for development.
 */

// Core pattern detection service
export { MarkdownPatternDetector, markdownPatternDetector } from './markdownPatternDetector';

// Pattern utilities and integration helpers
export { 
  PatternIntegrationUtilities, 
  patternIntegrationUtils,
  PatternTestUtils,
  mockPatternData 
} from './markdownPatternUtils';

// Existing services
export { default as mockTextService } from './mockTextService';

// Re-export types for convenience
export type {
  MarkdownPattern,
  MarkdownPatternType,
  PatternDetectionResult,
  PatternDetectionOptions,
  PatternDetectionEvent,
  CursorPosition,
  DetectionMetrics,
  PatternValidation,
  PatternReplacement,
  IMarkdownPatternDetector,
  PatternIntegrationUtils,
  MockPatternData,
  HeaderLevel,
  BulletType
} from '$lib/types/markdownPatterns';