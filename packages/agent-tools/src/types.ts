export interface NodeResult {
  id: string;
  nodeType: string;
  content: string;
  parentId?: string;
}

export interface SearchResult {
  nodes: NodeResult[];
  query: string;
}

export class ToolError extends Error {
  constructor(
    public readonly code: string,
    message: string
  ) {
    super(message);
    this.name = 'ToolError';
  }
}
