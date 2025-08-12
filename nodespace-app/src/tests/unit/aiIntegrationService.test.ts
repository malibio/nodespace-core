/**
 * AI Integration Service Tests
 * 
 * Comprehensive test suite for markdown import/export functionality
 * and AI integration workflows.
 */

import { describe, it, expect, beforeEach } from 'vitest';
import { 
  AIIntegrationService, 
  chatGPTIntegrationService, 
  claudeIntegrationService,
  AIIntegrationUtils
} from '$lib/services/aiIntegrationService';
import type { TreeNodeData } from '$lib/types/tree';
import type { AIIntegrationConfig } from '$lib/services/aiIntegrationService';

// Test data
const mockNodeHierarchy: TreeNodeData[] = [
  {
    id: 'node-1',
    title: 'Project Overview',
    content: 'Main project with multiple components and features',
    nodeType: 'text',
    depth: 0,
    parentId: null,
    expanded: true,
    hasChildren: true,
    children: [
      {
        id: 'node-1-1',
        title: 'Authentication System',
        content: 'User login and registration with JWT tokens',
        nodeType: 'task',
        depth: 1,
        parentId: 'node-1',
        expanded: true,
        hasChildren: false,
        children: []
      },
      {
        id: 'node-1-2',
        title: 'Data Management',
        content: 'CRUD operations with real-time synchronization',
        nodeType: 'text',
        depth: 1,
        parentId: 'node-1',
        expanded: true,
        hasChildren: true,
        children: [
          {
            id: 'node-1-2-1',
            title: 'Database Layer',
            content: 'PostgreSQL with connection pooling',
            nodeType: 'entity',
            depth: 2,
            parentId: 'node-1-2',
            expanded: true,
            hasChildren: false,
            children: []
          }
        ]
      }
    ]
  },
  {
    id: 'node-2',
    title: 'Performance Requirements',
    content: 'System must handle 1000+ concurrent users',
    nodeType: 'query',
    depth: 0,
    parentId: null,
    expanded: true,
    hasChildren: false,
    children: []
  }
];

const mockChatGPTResponse = `# Enhanced Project Architecture

Based on your project overview, here are key recommendations:

## Security Improvements
- **Multi-factor Authentication**: Implement TOTP-based 2FA
- **Rate Limiting**: Add API endpoint protection
- **Input Validation**: Comprehensive sanitization

## Performance Optimizations
- Database connection pooling
- Redis caching layer
- CDN for static assets

## Implementation Steps
1. Set up MFA system
2. Configure rate limiting middleware
3. Implement caching strategy

> **Note**: The 1000+ concurrent user target is achievable with proper scaling.

\`\`\`javascript
// Example rate limiting
const rateLimit = require('express-rate-limit');
const limiter = rateLimit({
  windowMs: 15 * 60 * 1000, // 15 minutes
  max: 100 // limit each IP to 100 requests per windowMs
});
\`\`\``;

const mockClaudeResponse = `# Technical Analysis & Recommendations

## Project Assessment
Your current architecture demonstrates solid foundations with authentication and data management components.

### Strengths
- Clear component separation
- Well-defined performance targets
- Comprehensive feature coverage

### Areas for Enhancement

**Security Framework**
- Implement zero-trust architecture
- Add comprehensive audit logging
- Consider security headers and HTTPS enforcement

**Scalability Considerations**
- Horizontal scaling with load balancers
- Database sharding strategies
- Microservice decomposition

**Performance Monitoring**
- APM integration (New Relic/DataDog)
- Custom metrics dashboard
- Real-time alerting system

### Implementation Priority Matrix
1. **Critical**: Security hardening
2. **High**: Performance monitoring
3. **Medium**: Scalability improvements

> The PostgreSQL with connection pooling approach is excellent for the initial scale.

\`\`\`sql
-- Example connection pool configuration
CREATE TABLE connection_metrics (
  pool_size INTEGER,
  active_connections INTEGER,
  idle_connections INTEGER,
  timestamp TIMESTAMP DEFAULT NOW()
);
\`\`\``;

describe('AIIntegrationService', () => {
  let service: AIIntegrationService;
  
  beforeEach(() => {
    service = new AIIntegrationService();
  });

  describe('Export Functionality', () => {
    it('should export node hierarchy to markdown', async () => {
      const result = await service.exportToMarkdown(mockNodeHierarchy);
      
      expect(result.markdown).toBeTruthy();
      expect(result.markdown).toContain('# Project Overview');
      expect(result.markdown).toContain('- [ ] User login and registration with JWT tokens');
      expect(result.stats.nodesProcessed).toBe(4);
      expect(result.stats.processingTime).toBeGreaterThan(0);
      expect(result.warnings).toBeInstanceOf(Array);
    });

    it('should handle empty node array', async () => {
      const result = await service.exportToMarkdown([]);
      
      expect(result.markdown).toBe('');
      expect(result.stats.nodesProcessed).toBe(0);
      expect(result.stats.charactersExported).toBe(0);
    });

    it('should apply different export styles', async () => {
      const configs = [
        { exportStyle: 'standard' as const },
        { exportStyle: 'ai-optimized' as const },
        { exportStyle: 'compact' as const }
      ];

      for (const config of configs) {
        const result = await service.exportToMarkdown(mockNodeHierarchy, config);
        expect(result.markdown).toBeTruthy();
        expect(result.metadata.config.exportStyle).toBe(config.exportStyle);
      }
    });

    it('should respect max depth configuration', async () => {
      const result = await service.exportToMarkdown(
        mockNodeHierarchy, 
        { maxDepth: 1 }
      );
      
      expect(result.markdown).toBeTruthy();
      expect(result.warnings.some(w => w.includes('maxDepth'))).toBe(true);
    });

    it('should clean AI patterns when requested', async () => {
      const nodesWithAIPatterns: TreeNodeData[] = [{
        id: 'ai-node',
        title: 'AI Response',
        content: '<thinking>This is internal reasoning</thinking>\n\nAI: Here is my response',
        nodeType: 'ai-chat',
        depth: 0,
        parentId: null,
        expanded: true,
        hasChildren: false,
        children: []
      }];

      const result = await service.exportToMarkdown(
        nodesWithAIPatterns,
        { cleanAIPatterns: true }
      );
      
      expect(result.markdown).not.toContain('<thinking>');
      expect(result.markdown).not.toContain('AI:');
      expect(result.stats.patternsConverted).toBeGreaterThan(0);
    });
  });

  describe('Import Functionality', () => {
    it('should import ChatGPT-style markdown to nodes', async () => {
      const result = await service.importFromMarkdown(mockChatGPTResponse);
      
      expect(result.nodes).toBeTruthy();
      expect(result.nodes.length).toBeGreaterThan(0);
      expect(result.stats.nodesCreated).toBeGreaterThan(0);
      expect(result.stats.patternsProcessed).toBeGreaterThan(0);
      expect(result.validation.isValid).toBe(true);
    });

    it('should import Claude-style markdown to nodes', async () => {
      const result = await service.importFromMarkdown(mockClaudeResponse);
      
      expect(result.nodes).toBeTruthy();
      expect(result.nodes.length).toBeGreaterThan(0);
      expect(result.stats.nodesCreated).toBeGreaterThan(0);
      expect(result.patterns.length).toBeGreaterThan(0);
    });

    it('should handle empty markdown input', async () => {
      const result = await service.importFromMarkdown('');
      
      expect(result.nodes).toEqual([]);
      expect(result.stats.nodesCreated).toBe(0);
      expect(result.validation.isValid).toBe(false);
    });

    it('should detect and convert bullet patterns', async () => {
      const bulletMarkdown = `# Task List
- First task item
  - Nested subtask
- Second task item
- [ ] Task checkbox item`;

      const result = await service.importFromMarkdown(bulletMarkdown);
      
      expect(result.nodes.some(node => node.nodeType === 'task')).toBe(true);
      expect(result.stats.bulletConversions).toBeGreaterThanOrEqual(0);
    });

    it('should apply WYSIWYG processing when enabled', async () => {
      const markdownWithFormatting = `# Header
This has **bold** and *italic* text with \`code\`.`;

      const result = await service.importFromMarkdown(
        markdownWithFormatting,
        { enableWYSIWYG: true }
      );
      
      expect(result.stats.wysiwygProcessed).toBe(true);
      expect(result.nodes.length).toBeGreaterThan(0);
    });

    it('should validate imported content at different levels', async () => {
      const configs: Partial<AIIntegrationConfig>[] = [
        { validationLevel: 'permissive' },
        { validationLevel: 'moderate' },
        { validationLevel: 'strict' }
      ];

      for (const config of configs) {
        const result = await service.importFromMarkdown(mockChatGPTResponse, config);
        expect(result.validation.isValid).toBeDefined();
        expect(result.validation.integrityScore).toBeGreaterThanOrEqual(0);
        expect(result.validation.integrityScore).toBeLessThanOrEqual(1);
      }
    });

    it('should handle malformed markdown gracefully', async () => {
      const malformedMarkdown = `# Incomplete header
**Unclosed bold formatting
\`\`\`javascript
// Unclosed code block without closing
- Bullet without content
`;

      const result = await service.importFromMarkdown(malformedMarkdown);
      
      expect(result.nodes).toBeInstanceOf(Array);
      expect(result.warnings.length).toBeGreaterThan(0);
    });
  });

  describe('Round-trip Functionality', () => {
    it('should maintain integrity through export-import cycle', async () => {
      const roundTripResult = await service.validateRoundTrip(mockNodeHierarchy);
      
      expect(roundTripResult.isValid).toBe(true);
      expect(roundTripResult.integrityScore).toBeGreaterThan(0.7);
      expect(roundTripResult.exportResult).toBeTruthy();
      expect(roundTripResult.importResult).toBeTruthy();
    });

    it('should preserve node structure in round-trip', async () => {
      const exportResult = await service.exportToMarkdown(mockNodeHierarchy);
      const importResult = await service.importFromMarkdown(exportResult.markdown);
      
      expect(importResult.nodes.length).toBeGreaterThan(0);
      expect(importResult.validation.integrityScore).toBeGreaterThan(0.5);
    });

    it('should handle complex nested structures', async () => {
      const deepHierarchy: TreeNodeData[] = [{
        id: 'root',
        title: 'Root',
        content: 'Root level',
        nodeType: 'text',
        depth: 0,
        parentId: null,
        expanded: true,
        hasChildren: true,
        children: [
          {
            id: 'level-1',
            title: 'Level 1',
            content: 'First level',
            nodeType: 'text',
            depth: 1,
            parentId: 'root',
            expanded: true,
            hasChildren: true,
            children: [
              {
                id: 'level-2',
                title: 'Level 2',
                content: 'Second level',
                nodeType: 'text',
                depth: 2,
                parentId: 'level-1',
                expanded: true,
                hasChildren: false,
                children: []
              }
            ]
          }
        ]
      }];

      const roundTripResult = await service.validateRoundTrip(deepHierarchy);
      expect(roundTripResult.integrityScore).toBeGreaterThan(0.6);
    });
  });

  describe('Configuration Management', () => {
    it('should update and apply configuration changes', () => {
      const newConfig: Partial<AIIntegrationConfig> = {
        exportStyle: 'compact',
        maxDepth: 5,
        cleanAIPatterns: false
      };

      service.updateConfig(newConfig);
      const currentConfig = service.getConfig();

      expect(currentConfig.exportStyle).toBe('compact');
      expect(currentConfig.maxDepth).toBe(5);
      expect(currentConfig.cleanAIPatterns).toBe(false);
    });

    it('should preserve other configuration when updating partially', () => {
      const originalConfig = service.getConfig();
      
      service.updateConfig({ exportStyle: 'compact' });
      const updatedConfig = service.getConfig();

      expect(updatedConfig.exportStyle).toBe('compact');
      expect(updatedConfig.validationLevel).toBe(originalConfig.validationLevel);
      expect(updatedConfig.maxDepth).toBe(originalConfig.maxDepth);
    });
  });

  describe('Error Handling', () => {
    it('should handle export errors gracefully', async () => {
      const invalidNodes = [
        {
          // Missing required fields
          id: 'invalid',
          content: null
        } as any
      ];

      const result = await service.exportToMarkdown(invalidNodes);
      expect(result.warnings.length).toBeGreaterThan(0);
    });

    it('should handle import errors gracefully', async () => {
      const result = await service.importFromMarkdown(null as any);
      
      expect(result.nodes).toEqual([]);
      expect(result.validation.isValid).toBe(false);
      expect(result.warnings.length).toBeGreaterThan(0);
    });
  });
});

describe('Specialized AI Services', () => {
  describe('ChatGPT Integration Service', () => {
    it('should be configured for ChatGPT workflows', () => {
      const config = chatGPTIntegrationService.getConfig();
      
      expect(config.exportStyle).toBe('ai-optimized');
      expect(config.cleanAIPatterns).toBe(true);
      expect(config.validationLevel).toBe('moderate');
    });

    it('should handle ChatGPT-specific patterns', async () => {
      const result = await chatGPTIntegrationService.importFromMarkdown(mockChatGPTResponse);
      
      expect(result.nodes.length).toBeGreaterThan(0);
      expect(result.validation.isValid).toBe(true);
    });
  });

  describe('Claude Integration Service', () => {
    it('should be configured for Claude workflows', () => {
      const config = claudeIntegrationService.getConfig();
      
      expect(config.exportStyle).toBe('standard');
      expect(config.validationLevel).toBe('strict');
      expect(config.includeMetadata).toBe(true);
    });

    it('should handle Claude-specific patterns', async () => {
      const result = await claudeIntegrationService.importFromMarkdown(mockClaudeResponse);
      
      expect(result.nodes.length).toBeGreaterThan(0);
      expect(result.validation.isValid).toBe(true);
    });
  });
});

describe('AIIntegrationUtils', () => {
  describe('Quick Export/Import', () => {
    it('should provide quick export functionality', async () => {
      const markdown = await AIIntegrationUtils.quickExport(mockNodeHierarchy, 'chatgpt');
      
      expect(markdown).toBeTruthy();
      expect(typeof markdown).toBe('string');
      expect(markdown.length).toBeGreaterThan(0);
    });

    it('should provide quick import functionality', async () => {
      const nodes = await AIIntegrationUtils.quickImport(mockChatGPTResponse, 'chatgpt');
      
      expect(nodes).toBeInstanceOf(Array);
      expect(nodes.length).toBeGreaterThan(0);
    });

    it('should support both ChatGPT and Claude types', async () => {
      const chatGPTMarkdown = await AIIntegrationUtils.quickExport(mockNodeHierarchy, 'chatgpt');
      const claudeMarkdown = await AIIntegrationUtils.quickExport(mockNodeHierarchy, 'claude');
      
      expect(chatGPTMarkdown).toBeTruthy();
      expect(claudeMarkdown).toBeTruthy();
      // Different services may produce different formatting
      expect(chatGPTMarkdown.length).toBeGreaterThan(0);
      expect(claudeMarkdown.length).toBeGreaterThan(0);
    });
  });

  describe('Markdown Validation', () => {
    it('should validate markdown content', async () => {
      const validation = await AIIntegrationUtils.validateMarkdown(mockChatGPTResponse);
      
      expect(validation.isValid).toBe(true);
      expect(validation.patterns).toBeInstanceOf(Array);
      expect(validation.patterns.length).toBeGreaterThan(0);
      expect(validation.warnings).toBeInstanceOf(Array);
    });

    it('should detect AI-specific patterns', async () => {
      const aiMarkdown = `<thinking>Internal reasoning</thinking>
      
AI: Here is my response with content.`;

      const validation = await AIIntegrationUtils.validateMarkdown(aiMarkdown);
      
      expect(validation.warnings.some(w => w.includes('thinking blocks'))).toBe(true);
      expect(validation.warnings.some(w => w.includes('response prefixes'))).toBe(true);
    });

    it('should handle empty markdown', async () => {
      const validation = await AIIntegrationUtils.validateMarkdown('');
      
      expect(validation.isValid).toBe(false);
      expect(validation.patterns).toEqual([]);
    });

    it('should handle whitespace-only markdown', async () => {
      const validation = await AIIntegrationUtils.validateMarkdown('   \n\n  \t  \n   ');
      
      expect(validation.isValid).toBe(false);
    });
  });

  describe('Workflow Configuration', () => {
    it('should provide chat workflow configuration', () => {
      const config = AIIntegrationUtils.getRecommendedConfig('chat');
      
      expect(config.exportStyle).toBe('ai-optimized');
      expect(config.cleanAIPatterns).toBe(true);
      expect(config.validationLevel).toBe('moderate');
      expect(config.maxDepth).toBe(5);
    });

    it('should provide content-generation workflow configuration', () => {
      const config = AIIntegrationUtils.getRecommendedConfig('content-generation');
      
      expect(config.exportStyle).toBe('standard');
      expect(config.cleanAIPatterns).toBe(false);
      expect(config.validationLevel).toBe('permissive');
      expect(config.enableWYSIWYG).toBe(true);
    });

    it('should provide analysis workflow configuration', () => {
      const config = AIIntegrationUtils.getRecommendedConfig('analysis');
      
      expect(config.exportStyle).toBe('compact');
      expect(config.includeMetadata).toBe(true);
      expect(config.validationLevel).toBe('strict');
      expect(config.preserveNodeIds).toBe(true);
    });
  });
});

describe('Performance Tests', () => {
  let perfService: AIIntegrationService;
  
  beforeEach(() => {
    perfService = new AIIntegrationService();
  });

  it('should handle large node hierarchies efficiently', async () => {
    // Create a large hierarchy
    const largeHierarchy: TreeNodeData[] = [];
    for (let i = 0; i < 100; i++) {
      largeHierarchy.push({
        id: `node-${i}`,
        title: `Node ${i}`,
        content: `Content for node ${i} with some substantial text content that simulates real usage patterns and provides meaningful content for processing.`,
        nodeType: 'text',
        depth: 0,
        parentId: null,
        expanded: true,
        hasChildren: false,
        children: []
      });
    }

    const startTime = performance.now();
    const result = await perfService.exportToMarkdown(largeHierarchy);
    const endTime = performance.now();

    expect(result.stats.nodesProcessed).toBe(100);
    expect(endTime - startTime).toBeLessThan(1000); // Should complete in under 1 second
    expect(result.markdown.length).toBeGreaterThan(1000);
  });

  it('should handle large markdown imports efficiently', async () => {
    // Create large markdown content
    const largeMarkdown = Array.from({ length: 50 }, (_, i) => 
      `# Section ${i}\n\nThis is content for section ${i} with **bold** and *italic* formatting.\n\n- Bullet point 1\n- Bullet point 2\n  - Nested bullet\n\n> Blockquote in section ${i}\n\n\`\`\`javascript\n// Code block ${i}\nfunction example${i}() {\n  return "example";\n}\n\`\`\`\n`
    ).join('\n\n');

    const startTime = performance.now();
    const result = await perfService.importFromMarkdown(largeMarkdown);
    const endTime = performance.now();

    expect(result.nodes.length).toBeGreaterThan(0);
    expect(endTime - startTime).toBeLessThan(2000); // Should complete in under 2 seconds
    expect(result.stats.patternsProcessed).toBeGreaterThan(100);
  });
});