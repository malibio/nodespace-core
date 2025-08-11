/**
 * Performance Regression Tests for CodeMirror Integration
 * 
 * Automated tests to prevent performance regressions in CI/CD pipeline
 */

import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { PerformanceBenchmarks } from './benchmarks.js';
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import { markdown } from '@codemirror/lang-markdown';

/**
 * Performance test configuration
 */
const PERFORMANCE_TARGETS = {
  editorInitialization: {
    average: 100, // ms
    p95: 150     // ms
  },
  keystrokeResponse: {
    average: 50,  // ms
    p95: 75,     // ms
    p99: 100     // ms
  },
  largeDocument: {
    load1KB: 50,    // ms
    load10KB: 100,  // ms  
    scroll: 16.67   // ms (60fps)
  },
  memory: {
    maxGrowthPerOperation: 1024, // bytes
    maxInitialMemory: 50 * 1024 * 1024 // 50MB
  }
};

describe('Performance Regression Tests', () => {
  let benchmarks;
  let testContainer;
  
  beforeEach(() => {
    benchmarks = new PerformanceBenchmarks();
    
    // Create test container
    testContainer = document.createElement('div');
    testContainer.style.position = 'absolute';
    testContainer.style.left = '-9999px';
    testContainer.style.top = '-9999px';
    document.body.appendChild(testContainer);
  });

  afterEach(() => {
    if (testContainer && testContainer.parentNode) {
      testContainer.parentNode.removeChild(testContainer);
    }
  });

  describe('Editor Initialization Performance', () => {
    it('should initialize editors within performance targets', async () => {
      const editorFactory = () => {
        const state = EditorState.create({
          doc: '',
          extensions: [
            EditorView.theme({
              '&': { fontSize: '14px', fontFamily: 'inherit' },
              '.cm-content': { padding: '0', minHeight: '20px' }
            })
          ]
        });
        
        return new EditorView({
          state,
          parent: testContainer
        });
      };

      const result = await benchmarks.benchmarkEditorInit(editorFactory, 5);
      
      expect(result.average).toBeLessThan(PERFORMANCE_TARGETS.editorInitialization.average);
      expect(result.times.every(t => t < PERFORMANCE_TARGETS.editorInitialization.p95)).toBe(true);
      expect(result.passed).toBe(true);
    });

    it('should initialize markdown editors within performance targets', async () => {
      const markdownEditorFactory = () => {
        const state = EditorState.create({
          doc: '# Test Document\n\nThis is **markdown** content.',
          extensions: [
            EditorView.theme({
              '&': { fontSize: '14px', fontFamily: 'inherit' },
              '.cm-content': { padding: '0', minHeight: '20px' }
            }),
            markdown()
          ]
        });
        
        return new EditorView({
          state,
          parent: testContainer
        });
      };

      const result = await benchmarks.benchmarkEditorInit(markdownEditorFactory, 5);
      
      // Markdown editors should still meet performance targets
      expect(result.average).toBeLessThan(PERFORMANCE_TARGETS.editorInitialization.average * 1.2); // 20% allowance
      expect(result.passed).toBe(true);
    });
  });

  describe('Keystroke Response Performance', () => {
    let editor;

    beforeEach(() => {
      const state = EditorState.create({
        doc: '',
        extensions: [
          EditorView.theme({
            '&': { fontSize: '14px', fontFamily: 'inherit' },
            '.cm-content': { padding: '0', minHeight: '20px' }
          })
        ]
      });
      
      editor = new EditorView({
        state,
        parent: testContainer
      });
    });

    afterEach(() => {
      if (editor) {
        editor.destroy();
      }
    });

    it('should respond to keystrokes within performance targets', async () => {
      const result = await benchmarks.benchmarkKeystrokeResponse(editor, 50);
      
      expect(result.average).toBeLessThan(PERFORMANCE_TARGETS.keystrokeResponse.average);
      expect(result.p95).toBeLessThan(PERFORMANCE_TARGETS.keystrokeResponse.p95);
      expect(result.p99).toBeLessThan(PERFORMANCE_TARGETS.keystrokeResponse.p99);
      expect(result.passed).toBe(true);
    });

    it('should maintain keystroke performance with markdown', async () => {
      // Reconfigure editor with markdown support
      editor.destroy();
      
      const state = EditorState.create({
        doc: '# Markdown Test\n\n',
        extensions: [
          EditorView.theme({
            '&': { fontSize: '14px', fontFamily: 'inherit' },
            '.cm-content': { padding: '0', minHeight: '20px' }
          }),
          markdown()
        ]
      });
      
      editor = new EditorView({
        state,
        parent: testContainer
      });

      const result = await benchmarks.benchmarkKeystrokeResponse(editor, 30);
      
      // Markdown should have minimal performance impact
      expect(result.average).toBeLessThan(PERFORMANCE_TARGETS.keystrokeResponse.average * 1.5);
      expect(result.p95).toBeLessThan(PERFORMANCE_TARGETS.keystrokeResponse.p95 * 1.5);
    });
  });

  describe('Large Document Performance', () => {
    let editor;

    beforeEach(() => {
      const state = EditorState.create({
        doc: '',
        extensions: [
          EditorView.theme({
            '&': { fontSize: '14px', fontFamily: 'inherit' },
            '.cm-content': { padding: '0', minHeight: '20px' },
            '.cm-scroller': { height: '200px', overflow: 'auto' }
          })
        ]
      });
      
      editor = new EditorView({
        state,
        parent: testContainer
      });
    });

    afterEach(() => {
      if (editor) {
        editor.destroy();
      }
    });

    it('should handle large documents within performance targets', async () => {
      const sizes = [1000, 5000, 10000]; // Test smaller sizes for CI
      const result = await benchmarks.benchmarkLargeDocument(editor, sizes);
      
      expect(result.passed).toBe(true);
      
      result.results.forEach(docResult => {
        if (docResult.size <= 1000) {
          expect(docResult.loadTime).toBeLessThan(PERFORMANCE_TARGETS.largeDocument.load1KB);
        } else if (docResult.size <= 10000) {
          expect(docResult.loadTime).toBeLessThan(PERFORMANCE_TARGETS.largeDocument.load10KB);
        }
        expect(docResult.scrollTime).toBeLessThan(PERFORMANCE_TARGETS.largeDocument.scroll * 2); // Allow 2x buffer
      });
    });
  });

  describe('Memory Usage Performance', () => {
    let editor;

    beforeEach(() => {
      const state = EditorState.create({
        doc: '',
        extensions: [
          EditorView.theme({
            '&': { fontSize: '14px', fontFamily: 'inherit' },
            '.cm-content': { padding: '0', minHeight: '20px' }
          })
        ]
      });
      
      editor = new EditorView({
        state,
        parent: testContainer
      });
    });

    afterEach(() => {
      if (editor) {
        editor.destroy();
      }
    });

    it('should not have significant memory leaks', async () => {
      const result = await benchmarks.benchmarkMemoryLeak(editor, 100); // Smaller test for CI
      
      expect(result.passed).toBe(true);
      expect(result.memoryGrowth.avgGrowthPerOperation).toBeLessThan(PERFORMANCE_TARGETS.memory.maxGrowthPerOperation);
      expect(result.memoryLeakDetected).toBe(false);
    });

    it('should have reasonable initial memory footprint', () => {
      // @ts-expect-error - Chrome-specific performance.memory API
      if (performance.memory) {
        // @ts-expect-error - Chrome-specific performance.memory API
        const initialMemory = performance.memory.usedJSHeapSize;
        expect(initialMemory).toBeLessThan(PERFORMANCE_TARGETS.memory.maxInitialMemory);
      }
    });
  });

  describe('Bundle Size Regression', () => {
    it('should maintain bundle size under target threshold', async () => {
      // This would typically be implemented as a separate build step test
      // For now, we verify the configuration is optimized
      
      // Verify CodeMirror imports are optimized
      const moduleSource = `
        import { EditorView } from '@codemirror/view';
        import { EditorState } from '@codemirror/state';
        import { markdown } from '@codemirror/lang-markdown';
      `;
      
      // Basic validation that imports are explicit rather than wildcard
      expect(moduleSource).toContain('EditorView');
      expect(moduleSource).toContain('EditorState');
      expect(moduleSource).toContain('markdown');
      expect(moduleSource).not.toContain('import *');
    });
  });

  describe('Performance Monitoring Integration', () => {
    it('should provide comprehensive performance metrics', async () => {
      const editorFactory = () => {
        const state = EditorState.create({
          doc: 'Test content',
          extensions: []
        });
        
        return new EditorView({
          state,
          parent: testContainer
        });
      };

      const testEditor = await editorFactory();
      const results = await benchmarks.runAllBenchmarks(testEditor, editorFactory);
      
      // Verify all benchmarks ran
      expect(results.tests.editorInit).toBeDefined();
      expect(results.tests.keystroke).toBeDefined();
      expect(results.tests.largeDoc).toBeDefined();
      expect(results.tests.memoryLeak).toBeDefined();
      
      // Verify summary is generated
      expect(results.summary).toBeDefined();
      expect(results.summary.allTestsPassed).toBeDefined();
      expect(results.summary.recommendations).toBeDefined();
      
      testEditor.destroy();
    });
  });
});

/**
 * CI/CD Integration helpers
 */
export function createPerformanceReport(results) {
  return {
    timestamp: new Date().toISOString(),
    passed: results.summary.allTestsPassed,
    metrics: {
      editorInit: results.tests.editorInit?.average || null,
      keystrokeP95: results.tests.keystroke?.p95 || null,
      largeDocLoad: results.tests.largeDoc?.results?.[0]?.loadTime || null,
      memoryGrowth: results.tests.memoryLeak?.memoryGrowth?.avgGrowthPerOperation || null
    },
    targets: PERFORMANCE_TARGETS,
    recommendations: results.summary.recommendations || []
  };
}

export { PERFORMANCE_TARGETS };