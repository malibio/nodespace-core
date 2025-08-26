# Supabase Integration Strategy

## Overview

Supabase serves as the **sync coordination hub** for NodeSpace's multi-user capabilities. This integration enables database synchronization, user authentication, and real-time coordination while preserving NodeSpace's local-first architecture.

**Key Principle**: Supabase coordinates between local LanceDB instances rather than replacing them.

## Supabase's Role in NodeSpace

### What Supabase DOES Handle

#### 1. **Authentication & User Management**
```sql
-- Supabase handles user accounts and sessions
CREATE TABLE auth.users (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    email TEXT UNIQUE NOT NULL,
    encrypted_password TEXT,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    -- Supabase built-in fields
);

-- NodeSpace user profiles
CREATE TABLE public.user_profiles (
    id UUID PRIMARY KEY REFERENCES auth.users(id),
    display_name TEXT,
    avatar_url TEXT,
    workspace_id UUID,
    settings JSONB DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

#### 2. **Sync Coordination Storage**
```sql
-- Central log of all node operations
CREATE TABLE sync_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID REFERENCES auth.users(id) NOT NULL,
    workspace_id UUID NOT NULL,
    node_id TEXT NOT NULL,
    operation_type TEXT NOT NULL, -- 'create', 'update', 'delete'
    operation_data JSONB NOT NULL,
    node_version INTEGER NOT NULL,
    parent_version INTEGER,
    timestamp TIMESTAMPTZ DEFAULT NOW(),
    device_id TEXT,
    
    -- Indexes for efficient querying
    INDEX idx_workspace_timestamp (workspace_id, timestamp),
    INDEX idx_node_versions (node_id, node_version),
    INDEX idx_user_recent (user_id, timestamp DESC)
);

-- Vector embeddings storage (using pgvector)
CREATE TABLE node_embeddings (
    node_id TEXT PRIMARY KEY,
    workspace_id UUID NOT NULL,
    embedding vector(384),
    model_version TEXT DEFAULT 'bge-small-en-v1.5',
    created_at TIMESTAMPTZ DEFAULT NOW(),
    
    -- Vector similarity index
    INDEX embedding_cosine_idx USING ivfflat (embedding vector_cosine_ops)
    WITH (lists = 100)
);

-- Conflict resolution tracking
CREATE TABLE conflict_resolutions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    node_id TEXT NOT NULL,
    workspace_id UUID NOT NULL,
    conflicting_versions INTEGER[],
    resolution_strategy TEXT, -- 'last_write_wins', 'manual', 'merge'
    resolved_version INTEGER,
    resolved_by UUID REFERENCES auth.users(id),
    resolution_data JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);
```

#### 3. **Real-Time Coordination**
```typescript
// Presence tracking for collaborative editing
const presenceChannel = supabase.channel(`workspace:${workspaceId}`)
  .on('presence', { event: 'sync' }, () => {
    const presenceState = presenceChannel.presenceState();
    updateActiveUsers(presenceState);
  })
  .on('broadcast', { event: 'cursor_position' }, ({ payload }) => {
    renderRemoteCursor(payload);
  })
  .subscribe();

// Node-level collaboration events
const nodeChannel = supabase.channel(`node:${nodeId}`)
  .on('postgres_changes', 
    { event: 'INSERT', schema: 'public', table: 'sync_log' },
    (payload) => handleRemoteChange(payload.new)
  )
  .subscribe();
```

#### 4. **File Storage for Large Attachments**
```typescript
// Large files stored in Supabase Storage
const uploadFile = async (file: File, nodeId: string) => {
  const filePath = `nodes/${nodeId}/attachments/${file.name}`;
  
  const { data, error } = await supabase.storage
    .from('node-attachments')
    .upload(filePath, file);
    
  if (error) throw error;
  
  // Store reference in node metadata
  await updateNodeMetadata(nodeId, {
    attachments: [{
      name: file.name,
      path: filePath,
      size: file.size,
      type: file.type,
      uploaded_at: new Date().toISOString()
    }]
  });
};
```

### What Supabase Does NOT Handle

#### ❌ **Primary Node Storage**
```typescript
// WRONG: Don't store nodes directly in Supabase
const createNode = async (node: Node) => {
  await supabase.from('nodes').insert(node); // ❌ DON'T DO THIS
};

// CORRECT: Store in local LanceDB, sync via operations
const createNode = async (node: Node) => {
  // 1. Store locally first (instant)
  await lanceDB.insert(node);
  
  // 2. Log operation for sync
  await syncManager.logOperation({
    type: 'create',
    node_id: node.id,
    data: node
  });
};
```

#### ❌ **Vector Search Operations**
```typescript
// WRONG: Primary vector search via Supabase
const search = async (query: string) => {
  const embedding = await generateEmbedding(query);
  return await supabase.rpc('match_nodes', { embedding }); // ❌ TOO SLOW
};

// CORRECT: Search locally, optionally sync results
const search = async (query: string) => {
  // Local search (fast)
  const results = await lanceDB.vectorSearch(query);
  
  // Optional: Cache results in cloud for other devices
  if (shouldCacheResults(query)) {
    await syncManager.cacheSearchResults(query, results);
  }
  
  return results;
};
```

#### ❌ **Real-Time Text Collaboration**
```typescript
// WRONG: Character-level edits via Supabase
const onTextChange = async (char: string, position: number) => {
  await supabase.from('text_operations').insert({ char, position }); // ❌ TOO SLOW
};

// CORRECT: Use Yjs/CRDT with Supabase as WebSocket provider
const collaborativeEditor = new Y.Doc();
const provider = new SupabaseProvider(
  supabase,
  `node-${nodeId}`,
  collaborativeEditor
);
```

## Implementation Architecture

### Sync Service Implementation

```typescript
export class SupabaseSyncService {
  private supabase: SupabaseClient;
  private localDB: LanceDBAdapter;
  private syncQueue: OperationQueue;
  
  constructor(supabaseUrl: string, apiKey: string) {
    this.supabase = createClient(supabaseUrl, apiKey);
    this.syncQueue = new OperationQueue();
  }
  
  async startSync(workspaceId: string): Promise<void> {
    // 1. Pull recent changes from server
    await this.pullRemoteChanges(workspaceId);
    
    // 2. Push local changes
    await this.pushLocalChanges(workspaceId);
    
    // 3. Start real-time subscription
    this.subscribeToChanges(workspaceId);
    
    // 4. Schedule periodic sync
    setInterval(() => this.periodicSync(), 30_000);
  }
  
  private async pullRemoteChanges(workspaceId: string): Promise<void> {
    const lastSyncTime = await this.getLastSyncTime();
    
    const { data: operations, error } = await this.supabase
      .from('sync_log')
      .select('*')
      .eq('workspace_id', workspaceId)
      .gt('timestamp', lastSyncTime)
      .order('timestamp', { ascending: true });
    
    if (error) throw error;
    
    for (const operation of operations) {
      await this.applyRemoteOperation(operation);
    }
    
    await this.updateLastSyncTime();
  }
  
  private async pushLocalChanges(workspaceId: string): Promise<void> {
    const pendingOperations = await this.syncQueue.getPending();
    
    if (pendingOperations.length === 0) return;
    
    const { data, error } = await this.supabase
      .from('sync_log')
      .insert(pendingOperations.map(op => ({
        workspace_id: workspaceId,
        user_id: this.currentUser.id,
        node_id: op.nodeId,
        operation_type: op.type,
        operation_data: op.data,
        node_version: op.version,
        parent_version: op.parentVersion
      })));
    
    if (error) throw error;
    
    // Mark as synced
    await this.syncQueue.markSynced(pendingOperations);
  }
  
  private subscribeToChanges(workspaceId: string): void {
    const channel = this.supabase
      .channel(`workspace-${workspaceId}`)
      .on('postgres_changes', {
        event: 'INSERT',
        schema: 'public',
        table: 'sync_log',
        filter: `workspace_id=eq.${workspaceId}`
      }, (payload) => {
        // Don't process our own changes
        if (payload.new.user_id !== this.currentUser.id) {
          this.handleRemoteOperation(payload.new);
        }
      })
      .subscribe();
  }
}
```

### Authentication Integration

```typescript
export class NodeSpaceAuth {
  private supabase: SupabaseClient;
  
  async signIn(email: string, password: string): Promise<AuthResult> {
    const { data, error } = await this.supabase.auth.signInWithPassword({
      email,
      password
    });
    
    if (error) throw error;
    
    // Create or update user profile
    await this.ensureUserProfile(data.user);
    
    return {
      user: data.user,
      session: data.session,
      profile: await this.getUserProfile(data.user.id)
    };
  }
  
  async signUp(email: string, password: string, displayName: string): Promise<AuthResult> {
    const { data, error } = await this.supabase.auth.signUp({
      email,
      password
    });
    
    if (error) throw error;
    
    // Create user profile
    await this.createUserProfile(data.user!.id, displayName);
    
    return {
      user: data.user!,
      session: data.session,
      profile: { id: data.user!.id, display_name: displayName }
    };
  }
  
  private async createUserProfile(userId: string, displayName: string): Promise<void> {
    const { error } = await this.supabase
      .from('user_profiles')
      .insert({
        id: userId,
        display_name: displayName,
        workspace_id: await this.createDefaultWorkspace(userId)
      });
    
    if (error) throw error;
  }
}
```

### Conflict Resolution

```typescript
export class ConflictResolver {
  private supabase: SupabaseClient;
  private localDB: LanceDBAdapter;
  
  async resolveConflict(
    nodeId: string,
    localVersion: Node,
    remoteVersions: Node[]
  ): Promise<Node> {
    // Simple strategy: Last Write Wins
    const latestRemote = remoteVersions.reduce((latest, current) => 
      current.updated_at > latest.updated_at ? current : latest
    );
    
    if (localVersion.updated_at > latestRemote.updated_at) {
      // Local wins - push to server
      await this.pushConflictResolution(nodeId, localVersion, 'local_wins');
      return localVersion;
    } else {
      // Remote wins - update local
      await this.localDB.update(nodeId, latestRemote);
      await this.logConflictResolution(nodeId, latestRemote, 'remote_wins');
      return latestRemote;
    }
  }
  
  async promptUserResolution(
    nodeId: string,
    localVersion: Node,
    remoteVersions: Node[]
  ): Promise<Node> {
    // Show conflict UI to user
    const resolution = await this.showConflictDialog({
      local: localVersion,
      remote: remoteVersions,
      nodeId
    });
    
    // Apply user's choice
    const resolvedNode = await this.applyResolution(resolution);
    
    // Log resolution
    await this.logConflictResolution(nodeId, resolvedNode, 'user_resolved');
    
    return resolvedNode;
  }
}
```

## Performance Considerations

### Efficient Sync Patterns

```typescript
// Batch operations for performance
export class BatchSyncOptimizer {
  private readonly BATCH_SIZE = 50;
  private readonly BATCH_TIMEOUT = 5000;
  private pendingOperations: SyncOperation[] = [];
  private batchTimer?: NodeJS.Timeout;
  
  queueOperation(operation: SyncOperation): void {
    this.pendingOperations.push(operation);
    
    if (this.pendingOperations.length >= this.BATCH_SIZE) {
      this.flushBatch();
    } else if (!this.batchTimer) {
      this.batchTimer = setTimeout(() => this.flushBatch(), this.BATCH_TIMEOUT);
    }
  }
  
  private async flushBatch(): Promise<void> {
    if (this.pendingOperations.length === 0) return;
    
    const batch = this.pendingOperations.splice(0);
    
    // Clear timer
    if (this.batchTimer) {
      clearTimeout(this.batchTimer);
      this.batchTimer = undefined;
    }
    
    // Send batch to Supabase
    await this.syncService.pushBatch(batch);
  }
}
```

### Optimized Vector Sync

```typescript
// Only sync vectors when content changes significantly
export class VectorSyncOptimizer {
  private readonly SIMILARITY_THRESHOLD = 0.95;
  
  async shouldSyncEmbedding(
    nodeId: string,
    newContent: string
  ): Promise<boolean> {
    const oldEmbedding = await this.getStoredEmbedding(nodeId);
    if (!oldEmbedding) return true;
    
    const newEmbedding = await this.generateEmbedding(newContent);
    const similarity = this.cosineSimilarity(oldEmbedding, newEmbedding);
    
    return similarity < this.SIMILARITY_THRESHOLD;
  }
  
  async syncEmbeddingIfNeeded(nodeId: string, content: string): Promise<void> {
    if (await this.shouldSyncEmbedding(nodeId, content)) {
      const embedding = await this.generateEmbedding(content);
      
      await this.supabase
        .from('node_embeddings')
        .upsert({
          node_id: nodeId,
          workspace_id: this.workspaceId,
          embedding: embedding,
          model_version: this.modelVersion
        });
    }
  }
}
```

## Security & Privacy

### Row Level Security (RLS)

```sql
-- Enable RLS on all tables
ALTER TABLE sync_log ENABLE ROW LEVEL SECURITY;
ALTER TABLE node_embeddings ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_profiles ENABLE ROW LEVEL SECURITY;

-- Users can only access their workspace data
CREATE POLICY "Users can access their workspace sync_log" ON sync_log
  FOR ALL USING (
    workspace_id IN (
      SELECT workspace_id FROM user_profiles 
      WHERE id = auth.uid()
    )
  );

CREATE POLICY "Users can access their workspace embeddings" ON node_embeddings
  FOR ALL USING (
    workspace_id IN (
      SELECT workspace_id FROM user_profiles 
      WHERE id = auth.uid()
    )
  );
```

### Data Encryption

```typescript
// Encrypt sensitive metadata before storing in Supabase
export class MetadataEncryptor {
  private encryptionKey: string;
  
  async encryptSensitiveFields(metadata: any): Promise<any> {
    const sensitiveFields = ['credentials', 'api_keys', 'personal_info'];
    const encrypted = { ...metadata };
    
    for (const field of sensitiveFields) {
      if (encrypted[field]) {
        encrypted[field] = await this.encrypt(JSON.stringify(encrypted[field]));
      }
    }
    
    return encrypted;
  }
  
  async decryptSensitiveFields(metadata: any): Promise<any> {
    const sensitiveFields = ['credentials', 'api_keys', 'personal_info'];
    const decrypted = { ...metadata };
    
    for (const field of sensitiveFields) {
      if (decrypted[field] && typeof decrypted[field] === 'string') {
        decrypted[field] = JSON.parse(await this.decrypt(decrypted[field]));
      }
    }
    
    return decrypted;
  }
}
```

## Development & Testing

### Local Development Setup

```typescript
// Mock Supabase for local development
export class MockSupabaseClient {
  private localSyncLog: SyncOperation[] = [];
  
  async from(table: string) {
    switch (table) {
      case 'sync_log':
        return new MockSyncLogTable(this.localSyncLog);
      case 'user_profiles':
        return new MockUserProfilesTable();
      default:
        throw new Error(`Mock table ${table} not implemented`);
    }
  }
  
  channel(name: string) {
    return new MockRealtimeChannel(name);
  }
}

// Use in development
const supabase = process.env.NODE_ENV === 'development'
  ? new MockSupabaseClient()
  : createClient(supabaseUrl, supabaseKey);
```

### Testing Strategies

```typescript
// Integration tests with real Supabase instance
describe('Supabase Sync Integration', () => {
  let testSupabase: SupabaseClient;
  let testWorkspace: string;
  
  beforeEach(async () => {
    testSupabase = createClient(testUrl, testKey);
    testWorkspace = await createTestWorkspace();
  });
  
  test('should sync node operations', async () => {
    const syncService = new SupabaseSyncService(testSupabase);
    
    // Create node locally
    const node = await createTestNode();
    
    // Sync to Supabase
    await syncService.pushOperation({
      type: 'create',
      nodeId: node.id,
      data: node
    });
    
    // Verify in database
    const { data } = await testSupabase
      .from('sync_log')
      .select('*')
      .eq('node_id', node.id);
    
    expect(data).toHaveLength(1);
    expect(data[0].operation_type).toBe('create');
  });
});
```

## Migration Strategy

### Enabling Supabase Sync

```rust
// Rust implementation for enabling sync on existing installation
impl NodeSpaceMigration {
    async fn enable_supabase_sync(&self, config: SupabaseConfig) -> Result<(), Error> {
        // 1. Backup current data
        self.create_migration_backup().await?;
        
        // 2. Export all operations from local LanceDB
        let operations = self.export_all_operations().await?;
        
        // 3. Initialize Supabase connection
        let supabase = SupabaseClient::new(config).await?;
        
        // 4. Push historical data
        supabase.push_historical_operations(operations).await?;
        
        // 5. Enable real-time sync
        self.enable_sync_service(supabase).await?;
        
        // 6. Update configuration
        self.update_config_with_sync(config).await?;
        
        Ok(())
    }
}
```

## Monitoring & Observability

### Sync Health Monitoring

```typescript
export class SyncHealthMonitor {
  private metrics = {
    operationsPerSecond: 0,
    syncLatency: [] as number[],
    conflictRate: 0,
    errorRate: 0
  };
  
  startMonitoring(): void {
    setInterval(() => {
      this.reportMetrics();
    }, 60_000); // Every minute
  }
  
  recordSyncOperation(latency: number, success: boolean): void {
    this.metrics.syncLatency.push(latency);
    
    if (!success) {
      this.metrics.errorRate++;
    }
    
    // Keep only last 100 measurements
    if (this.metrics.syncLatency.length > 100) {
      this.metrics.syncLatency.shift();
    }
  }
  
  private reportMetrics(): void {
    const avgLatency = this.metrics.syncLatency.reduce((a, b) => a + b, 0) / this.metrics.syncLatency.length;
    
    console.log('Sync Health:', {
      avgLatencyMs: avgLatency,
      p95LatencyMs: this.percentile(this.metrics.syncLatency, 0.95),
      errorRate: this.metrics.errorRate,
      conflictRate: this.metrics.conflictRate
    });
  }
}
```

This Supabase integration strategy provides a robust foundation for NodeSpace's multi-user capabilities while preserving the local-first architecture and performance characteristics that make NodeSpace unique.