/**
 * Performance validation tests for NodeManager
 * Tests handling of 1000+ node documents efficiently
 */

import { describe, test, expect, beforeEach } from 'vitest';
import { NodeManager } from '../../lib/services/NodeManager';
import type { NodeManagerEvents } from '../../lib/services/NodeManager';

describe('NodeManager Performance Tests', () => {
  let nodeManager: NodeManager;
  let mockEvents: NodeManagerEvents;

  beforeEach(() => {
    mockEvents = {
      focusRequested: () => {},
      hierarchyChanged: () => {},
      nodeCreated: () => {},
      nodeDeleted: () => {}
    };
    nodeManager = new NodeManager(mockEvents);
  });

  // Generate large dataset for performance testing
  const generateLargeNodeDataset = (nodeCount: number) => {
    const nodes = [];
    for (let i = 0; i < nodeCount; i++) {
      nodes.push({
        id: `node-${i}`,
        content: `Content for node ${i}`,
        nodeType: 'text',
        depth: Math.floor(i / 100), // Create hierarchy
        parentId: i > 0 ? `node-${Math.floor((i - 1) / 10)}` : undefined,
        children: [],
        expanded: true,
        autoFocus: false,
        inheritHeaderLevel: 0,
        metadata: { created: Date.now() }
      });
    }
    return nodes;
  };

  test('initializes 1000 nodes efficiently (< 100ms)', () => {
    const largeDataset = generateLargeNodeDataset(1000);
    
    const startTime = performance.now();
    nodeManager.initializeFromLegacyData(largeDataset);
    const endTime = performance.now();
    
    const duration = endTime - startTime;
    console.log(`1000 node initialization: ${duration.toFixed(2)}ms`);
    
    expect(duration).toBeLessThan(100);
    expect(nodeManager.getVisibleNodes()).toHaveLength(1000);
  });

  test('node lookup performance with 1000+ nodes (< 1ms)', () => {
    const largeDataset = generateLargeNodeDataset(1500);
    nodeManager.initializeFromLegacyData(largeDataset);
    
    const startTime = performance.now();
    for (let i = 0; i < 100; i++) {
      const randomId = `node-${Math.floor(Math.random() * 1500)}`;
      nodeManager.findNode(randomId);
    }
    const endTime = performance.now();
    
    const duration = endTime - startTime;
    const avgDuration = duration / 100;
    console.log(`Average lookup time (1500 nodes): ${avgDuration.toFixed(4)}ms`);
    
    expect(avgDuration).toBeLessThan(1);
  });

  test('combineNodes performance with large document (< 10ms)', () => {
    const largeDataset = generateLargeNodeDataset(2000);
    nodeManager.initializeFromLegacyData(largeDataset);
    
    const startTime = performance.now();
    
    // Test multiple node combinations
    for (let i = 1; i < 11; i++) {
      const currentId = `node-${i}`;
      const previousId = `node-${i - 1}`;
      nodeManager.combineNodes(currentId, previousId);
    }
    
    const endTime = performance.now();
    const duration = endTime - startTime;
    console.log(`10 node combinations (2000 node document): ${duration.toFixed(2)}ms`);
    
    expect(duration).toBeLessThan(10);
  });

  test('hierarchy operations scale efficiently (< 50ms for 100 operations)', () => {
    const largeDataset = generateLargeNodeDataset(1000);
    nodeManager.initializeFromLegacyData(largeDataset);
    
    const startTime = performance.now();
    
    // Test multiple hierarchy operations
    for (let i = 100; i < 200; i++) {
      const nodeId = `node-${i}`;
      nodeManager.indentNode(nodeId);
      if (i % 2 === 0) {
        nodeManager.outdentNode(nodeId);
      }
    }
    
    const endTime = performance.now();
    const duration = endTime - startTime;
    console.log(`100 hierarchy operations: ${duration.toFixed(2)}ms`);
    
    expect(duration).toBeLessThan(50);
  });

  test('getVisibleNodes performance with large nested structure (< 25ms)', () => {
    // Create deeply nested structure
    const nestedDataset = [];
    for (let i = 0; i < 1000; i++) {
      nestedDataset.push({
        id: `node-${i}`,
        content: `Content ${i}`,
        nodeType: 'text',
        depth: Math.floor(i / 50), // Create 20 levels deep
        parentId: i > 0 ? `node-${i - 1}` : undefined,
        children: i < 999 ? [`node-${i + 1}`] : [],
        expanded: Math.random() > 0.3, // 70% expanded
        autoFocus: false,
        inheritHeaderLevel: 0,
        metadata: {}
      });
    }
    
    nodeManager.initializeFromLegacyData(nestedDataset);
    
    const startTime = performance.now();
    const visibleNodes = nodeManager.getVisibleNodes();
    const endTime = performance.now();
    
    const duration = endTime - startTime;
    console.log(`getVisibleNodes (1000 nested, 70% expanded): ${duration.toFixed(2)}ms`);
    console.log(`Visible nodes count: ${visibleNodes.length}`);
    
    expect(duration).toBeLessThan(25);
    expect(visibleNodes.length).toBeGreaterThan(500); // Should have many visible nodes
  });

  test('memory efficiency - no memory leaks during operations', () => {
    const initialMemory = (process as any).memoryUsage?.().heapUsed || 0;
    
    // Perform many operations
    for (let cycle = 0; cycle < 10; cycle++) {
      const dataset = generateLargeNodeDataset(500);
      nodeManager.initializeFromLegacyData(dataset);
      
      // Perform various operations
      for (let i = 1; i < 50; i++) {
        nodeManager.createNode(`node-${i}`, `New content ${i}`);
        if (i % 3 === 0) {
          nodeManager.deleteNode(`node-${i}`);
        }
      }
    }
    
    const finalMemory = (process as any).memoryUsage?.().heapUsed || 0;
    const memoryIncrease = finalMemory - initialMemory;
    
    console.log(`Memory increase after operations: ${(memoryIncrease / 1024 / 1024).toFixed(2)}MB`);
    
    // Memory increase should be reasonable (< 50MB for all operations)
    expect(memoryIncrease).toBeLessThan(50 * 1024 * 1024);
  });

  test('concurrent operations performance', () => {
    const largeDataset = generateLargeNodeDataset(1000);
    nodeManager.initializeFromLegacyData(largeDataset);
    
    const startTime = performance.now();
    
    // Simulate concurrent operations (batched for testing)
    const operations = [];
    for (let i = 0; i < 200; i++) {
      operations.push(() => {
        const nodeId = `node-${Math.floor(Math.random() * 1000)}`;
        nodeManager.updateNodeContent(nodeId, `Updated content ${i}`);
      });
    }
    
    // Execute all operations
    operations.forEach(op => op());
    
    const endTime = performance.now();
    const duration = endTime - startTime;
    console.log(`200 concurrent operations: ${duration.toFixed(2)}ms`);
    
    expect(duration).toBeLessThan(100);
  });
});