import { http, HttpResponse } from 'msw';
import { NodeData, NodeType } from '../fixtures/types';
import { createMockNode, createMockNodes } from '../fixtures/mockData';

// Mock API handlers for testing
export const handlers = [
  // Get all nodes
  http.get('/api/nodes', ({ request }) => {
    const url = new URL(request.url);
    const query = url.searchParams.get('q');
    const type = url.searchParams.get('type') as NodeType | null;
    
    let nodes = createMockNodes();
    
    // Filter by search query
    if (query) {
      nodes = nodes.filter(node => 
        node.content.toLowerCase().includes(query.toLowerCase()) ||
        JSON.stringify(node.metadata).toLowerCase().includes(query.toLowerCase())
      );
    }
    
    // Filter by type
    if (type) {
      nodes = nodes.filter(node => node.node_type === type);
    }
    
    return HttpResponse.json(nodes);
  }),

  // Get node by ID
  http.get('/api/nodes/:id', ({ params }) => {
    const { id } = params;
    const mockNodes = createMockNodes();
    const node = mockNodes.find(n => n.id === id);
    
    if (!node) {
      return HttpResponse.json({ error: 'Node not found' }, { status: 404 });
    }
    
    return HttpResponse.json(node);
  }),

  // Create new node
  http.post('/api/nodes', async ({ request }) => {
    const nodeData = await request.json() as Partial<NodeData>;
    const newNode = createMockNode({
      node_type: nodeData.node_type || NodeType.Text,
      content: nodeData.content || 'New node content',
      metadata: nodeData.metadata || {},
    });
    
    // Simulate creation delay
    await new Promise(resolve => setTimeout(resolve, 100));
    
    return HttpResponse.json(newNode, { status: 201 });
  }),

  // Update node
  http.put('/api/nodes/:id', async ({ params, request }) => {
    const { id } = params;
    const updates = await request.json() as Partial<NodeData>;
    const mockNodes = createMockNodes();
    const existingNode = mockNodes.find(n => n.id === id);
    
    if (!existingNode) {
      return HttpResponse.json({ error: 'Node not found' }, { status: 404 });
    }
    
    const updatedNode: NodeData = {
      ...existingNode,
      ...updates,
      id: existingNode.id, // Preserve ID
      updated_at: new Date().toISOString(),
    };
    
    // Simulate update delay
    await new Promise(resolve => setTimeout(resolve, 50));
    
    return HttpResponse.json(updatedNode);
  }),

  // Delete node
  http.delete('/api/nodes/:id', async ({ params }) => {
    const { id } = params;
    const mockNodes = createMockNodes();
    const nodeExists = mockNodes.some(n => n.id === id);
    
    if (!nodeExists) {
      return HttpResponse.json({ error: 'Node not found' }, { status: 404 });
    }
    
    // Simulate deletion delay
    await new Promise(resolve => setTimeout(resolve, 75));
    
    return HttpResponse.json({ success: true });
  }),

  // Search nodes
  http.get('/api/search', ({ request }) => {
    const url = new URL(request.url);
    const query = url.searchParams.get('q') || '';
    const limit = parseInt(url.searchParams.get('limit') || '10');
    
    if (!query.trim()) {
      return HttpResponse.json([]);
    }
    
    const mockNodes = createMockNodes();
    const results = mockNodes
      .filter(node => 
        node.content.toLowerCase().includes(query.toLowerCase()) ||
        JSON.stringify(node.metadata).toLowerCase().includes(query.toLowerCase())
      )
      .slice(0, limit);
    
    return HttpResponse.json(results);
  }),

  // Error simulation endpoints
  http.get('/api/nodes/error/500', () => {
    return HttpResponse.json({ error: 'Internal server error' }, { status: 500 });
  }),

  http.get('/api/nodes/error/timeout', async () => {
    // Simulate timeout
    await new Promise(resolve => setTimeout(resolve, 30000));
    return HttpResponse.json({ data: 'Should not reach here' });
  }),

  http.post('/api/nodes/error/validation', () => {
    return HttpResponse.json({
      error: 'Validation failed',
      details: {
        content: 'Content is required',
        type: 'Invalid node type'
      }
    }, { status: 400 });
  }),
];