// Test types matching the Rust backend types
export enum NodeType {
  Text = 'Text',
  Task = 'Task',
  AIChat = 'AIChat',
}

export interface NodeData {
  id: string;
  node_type: NodeType;
  content: string;
  metadata: Record<string, any>;
  created_at: string;
  updated_at: string;
}

export interface NodeCreateRequest {
  node_type: NodeType;
  content: string;
  metadata?: Record<string, any>;
}

export interface NodeUpdateRequest {
  id: string;
  content?: string;
  metadata?: Record<string, any>;
}

export interface TestScenario {
  id: string;
  name: string;
  description: string;
  setup: () => Promise<void>;
  cleanup: () => Promise<void>;
  data: NodeData[];
}

export interface TestContext {
  nodes: NodeData[];
  mockApi: {
    createNode: (data: NodeCreateRequest) => Promise<NodeData>;
    updateNode: (data: NodeUpdateRequest) => Promise<NodeData>;
    deleteNode: (id: string) => Promise<void>;
    getNode: (id: string) => Promise<NodeData | null>;
    searchNodes: (query: string) => Promise<NodeData[]>;
  };
}