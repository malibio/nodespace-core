import { NodeData, NodeType } from './types';

let nodeIdCounter = 1;

export function createMockNode(overrides: Partial<NodeData> = {}): NodeData {
  const id = overrides.id || `node-${nodeIdCounter++}`;
  const now = new Date().toISOString();
  
  return {
    id,
    node_type: NodeType.Text,
    content: `Mock content for ${id}`,
    metadata: {},
    created_at: now,
    updated_at: now,
    ...overrides,
  };
}

export function createMockTextNode(content: string = 'Sample text content'): NodeData {
  return createMockNode({
    node_type: NodeType.Text,
    content,
  });
}

export function createMockTaskNode(content: string = 'Sample task', completed: boolean = false): NodeData {
  return createMockNode({
    node_type: NodeType.Task,
    content,
    metadata: {
      completed,
      dueDate: null,
      priority: 'medium',
    },
  });
}

export function createMockAIChatNode(content: string = 'AI conversation'): NodeData {
  return createMockNode({
    node_type: NodeType.AIChat,
    content,
    metadata: {
      model: 'gpt-4',
      tokens: 150,
      temperature: 0.7,
    },
  });
}

export function createMockNodes(): NodeData[] {
  return [
    createMockTextNode('Welcome to NodeSpace! This is your first text node.'),
    createMockTextNode('Meeting notes from the weekly standup'),
    createMockTaskNode('Complete the user authentication feature', false),
    createMockTaskNode('Review pull request #42', true),
    createMockTaskNode('Update documentation for the new API', false),
    createMockAIChatNode('Discuss the architecture of the new system'),
    createMockTextNode('Random thoughts about the project direction'),
    createMockTextNode('Notes from the client meeting yesterday'),
    createMockTaskNode('Fix the bug in the node rendering system', false),
    createMockAIChatNode('Help me brainstorm ideas for the next sprint'),
  ];
}

export function createTestScenarioData(scenarioName: string): NodeData[] {
  switch (scenarioName) {
    case 'empty':
      return [];
    
    case 'single-text':
      return [createMockTextNode('Single text node for testing')];
    
    case 'multiple-types':
      return [
        createMockTextNode('Text node'),
        createMockTaskNode('Task node'),
        createMockAIChatNode('Chat node'),
      ];
    
    case 'search-test':
      return [
        createMockTextNode('This contains the SEARCH term'),
        createMockTextNode('This does not contain the term'),
        createMockTaskNode('SEARCH this task as well'),
        createMockAIChatNode('No matching content here'),
      ];
    
    case 'large-dataset':
      return Array.from({ length: 100 }, (_, i) => 
        createMockNode({
          node_type: [NodeType.Text, NodeType.Task, NodeType.AIChat][i % 3],
          content: `Generated node ${i + 1} for performance testing`,
        })
      );
    
    default:
      return createMockNodes();
  }
}

export function createNodeWithMetadata(
  nodeType: NodeType,
  content: string,
  metadata: Record<string, any>
): NodeData {
  return createMockNode({
    node_type: nodeType,
    content,
    metadata,
  });
}

export function createTestNodesWithDates(dateRange: { start: Date; end: Date }): NodeData[] {
  const { start, end } = dateRange;
  const dayMs = 24 * 60 * 60 * 1000;
  const totalDays = Math.ceil((end.getTime() - start.getTime()) / dayMs);
  
  return Array.from({ length: totalDays }, (_, i) => {
    const date = new Date(start.getTime() + i * dayMs);
    return createMockNode({
      content: `Node created on ${date.toDateString()}`,
      created_at: date.toISOString(),
      updated_at: date.toISOString(),
    });
  });
}

// Utility functions for test data manipulation
export function getNodesByType(nodes: NodeData[], type: NodeType): NodeData[] {
  return nodes.filter(node => node.node_type === type);
}

export function searchNodesInTestData(nodes: NodeData[], query: string): NodeData[] {
  const lowerQuery = query.toLowerCase();
  return nodes.filter(node =>
    node.content.toLowerCase().includes(lowerQuery) ||
    JSON.stringify(node.metadata).toLowerCase().includes(lowerQuery)
  );
}

export function sortNodesByDate(nodes: NodeData[], ascending: boolean = true): NodeData[] {
  return [...nodes].sort((a, b) => {
    const dateA = new Date(a.created_at).getTime();
    const dateB = new Date(b.created_at).getTime();
    return ascending ? dateA - dateB : dateB - dateA;
  });
}

// Reset counter for predictable tests
export function resetMockNodeCounter(): void {
  nodeIdCounter = 1;
}