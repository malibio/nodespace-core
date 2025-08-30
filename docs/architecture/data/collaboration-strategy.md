# NodeSpace Real-Time Collaboration Strategy

## Overview

NodeSpace supports **two complementary modes** of operation:
1. **Solo/Offline Mode**: Local-first editing with background sync
2. **Live Collaboration Mode**: Real-time, Google Docs-style collaborative editing

This hybrid approach provides the best of both worlds: instant local performance with optional real-time collaboration when users choose to work together on the same content.

## Collaboration Architecture

### Dual-Mode Design

```
┌─────────────────────────────────────────────────────────────┐
│                    NodeSpace Client                         │
├─────────────────┬───────────────────────────────────────────┤
│   Solo Mode     │           Collaborative Mode              │
│                 │                                           │
│  Local LanceDB  │  Local LanceDB + CRDT Layer + WebSocket  │
│       ↓         │         ↓              ↓          ↓       │
│  Svelte Store   │  Svelte Store    Yjs Engine   Realtime   │
│       ↓         │         ↓              ↓          ↓       │
│   UI Update     │    UI Update     Conflict      Others'    │
│                 │                  Resolution    Changes    │
└─────────────────┴───────────────────────────────────────────┘
```

### Mode Switching Logic

```typescript
class NodeEditor {
    private mode: 'solo' | 'collaborative' = 'solo';
    private collaborationSession?: CollaborationSession;
    
    async openNode(nodeId: string): Promise<void> {
        // Always load from local LanceDB first
        const node = await this.localStorage.getNode(nodeId);
        this.displayNode(node);
        
        // Check if others are actively editing
        const activeUsers = await this.presence.getActiveUsers(nodeId);
        
        if (activeUsers.length > 0 && navigator.onLine) {
            // Prompt user to join collaboration
            const shouldCollaborate = await this.promptCollaboration(activeUsers);
            
            if (shouldCollaborate) {
                await this.enterCollaborativeMode(nodeId);
            }
        }
    }
    
    private async promptCollaboration(users: User[]): Promise<boolean> {
        const userNames = users.map(u => u.name).join(', ');
        return confirm(`${userNames} ${users.length === 1 ? 'is' : 'are'} editing this node. Join collaborative session?`);
    }
}
```

## Real-Time Collaboration Technology Stack

### 1. CRDT Implementation: Yjs

[Yjs](https://github.com/yjs/yjs) provides conflict-free replicated data types for collaborative editing:

```typescript
import * as Y from 'yjs';
import { WebsocketProvider } from 'y-websocket';
import { IndexeddbPersistence } from 'y-indexeddb';

class CollaborativeTextEditor {
    private ydoc: Y.Doc;
    private ytext: Y.Text;
    private provider: WebsocketProvider;
    private indexeddbProvider: IndexeddbPersistence;
    
    async initialize(nodeId: string): Promise<void> {
        // Create Yjs document
        this.ydoc = new Y.Doc();
        this.ytext = this.ydoc.getText('content');
        
        // Local persistence for offline edits
        this.indexeddbProvider = new IndexeddbPersistence(nodeId, this.ydoc);
        
        // WebSocket connection for real-time sync
        this.provider = new WebsocketProvider(
            `wss://${COLLAB_SERVER_URL}`,
            `node-${nodeId}`,
            this.ydoc,
            {
                connect: true,
                maxBackoffTime: 10000
            }
        );
        
        // Bind to text editor
        this.bindToEditor();
    }
    
    private bindToEditor(): void {
        // Listen to remote changes
        this.ytext.observe((event) => {
            if (!event.transaction.local) {
                // Remote change - update editor
                this.updateEditor(this.ytext.toString());
            }
        });
        
        // Listen to local changes
        this.editor.on('change', (content: string) => {
            this.ydoc.transact(() => {
                // Clear and insert (simple approach)
                this.ytext.delete(0, this.ytext.length);
                this.ytext.insert(0, content);
            }, this.ydoc.clientID);
        });
    }
}
```

### 2. Presence Awareness

Track where other users are editing in real-time:

```typescript
class PresenceManager {
    private awareness: Awareness;
    private localUser: User;
    
    constructor(provider: WebsocketProvider, user: User) {
        this.awareness = provider.awareness;
        this.localUser = user;
        
        // Set local user info
        this.awareness.setLocalStateField('user', {
            name: user.name,
            avatar: user.avatar,
            color: this.generateUserColor(user.id)
        });
        
        // Listen for presence changes
        this.awareness.on('update', this.handlePresenceUpdate.bind(this));
    }
    
    updateCursor(position: number, selection?: [number, number]): void {
        this.awareness.setLocalStateField('cursor', {
            position,
            selection,
            timestamp: Date.now()
        });
    }
    
    private handlePresenceUpdate(): void {
        const states = Array.from(this.awareness.getStates().entries());
        const collaborators = states
            .filter(([clientId]) => clientId !== this.awareness.clientID)
            .map(([clientId, state]) => ({
                id: clientId,
                user: state.user,
                cursor: state.cursor
            }));
        
        this.renderCollaborators(collaborators);
    }
}
```

### 3. WebSocket Server Integration

Using Supabase Realtime for WebSocket infrastructure:

```typescript
// Supabase setup for collaboration
const supabase = createClient(url, key, {
    realtime: {
        params: {
            eventsPerSecond: 100, // High frequency for collaboration
        },
    },
});

class RealtimeCollaboration {
    private channel: RealtimeChannel;
    
    async joinNodeSession(nodeId: string): Promise<void> {
        this.channel = supabase.channel(`node:${nodeId}`, {
            config: {
                broadcast: { self: true },
                presence: { key: this.userId },
            },
        });
        
        // Track presence (who's in the document)
        this.channel
            .on('presence', { event: 'sync' }, () => {
                const presenceState = this.channel.presenceState();
                this.updateActiveUsers(presenceState);
            })
            .on('broadcast', { event: 'cursor' }, ({ payload }) => {
                this.renderRemoteCursor(payload);
            })
            .on('broadcast', { event: 'selection' }, ({ payload }) => {
                this.renderRemoteSelection(payload);
            });
        
        await this.channel.subscribe();
        
        // Announce presence
        await this.channel.track({
            user: this.currentUser,
            timestamp: Date.now(),
        });
    }
    
    broadcastCursor(position: CursorPosition): void {
        this.channel.send({
            type: 'broadcast',
            event: 'cursor',
            payload: {
                userId: this.userId,
                position,
                timestamp: Date.now(),
            },
        });
    }
}
```

## UI Components for Collaboration

### Collaborative Node Editor Component

```svelte
<!-- CollaborativeNodeEditor.svelte -->
<script lang="ts">
    import { onMount, onDestroy } from 'svelte';
    import { CollaborativeTextEditor } from './collaboration/CollaborativeTextEditor';
    import { PresenceManager } from './collaboration/PresenceManager';
    
    export let nodeId: string;
    export let initialContent: string;
    export let mode: 'solo' | 'collaborative' = 'solo';
    
    let editorElement: HTMLElement;
    let collaborativeEditor: CollaborativeTextEditor;
    let presenceManager: PresenceManager;
    let collaborators: Collaborator[] = [];
    let isConnected = false;
    
    onMount(async () => {
        if (mode === 'collaborative') {
            await initializeCollaboration();
        }
    });
    
    async function initializeCollaboration() {
        try {
            collaborativeEditor = new CollaborativeTextEditor(editorElement);
            await collaborativeEditor.initialize(nodeId);
            
            presenceManager = new PresenceManager(
                collaborativeEditor.provider,
                currentUser
            );
            
            // Listen for collaborator updates
            presenceManager.on('collaborators-changed', (updated) => {
                collaborators = updated;
            });
            
            // Listen for connection status
            collaborativeEditor.provider.on('status', ({ status }) => {
                isConnected = status === 'connected';
            });
            
        } catch (error) {
            console.error('Failed to initialize collaboration:', error);
            // Fallback to solo mode
            mode = 'solo';
        }
    }
    
    function handleCursorMove(event: CustomEvent) {
        if (mode === 'collaborative' && presenceManager) {
            presenceManager.updateCursor(event.detail.position);
        }
    }
    
    onDestroy(() => {
        collaborativeEditor?.destroy();
        presenceManager?.destroy();
    });
</script>

<div class="node-editor">
    <!-- Collaboration Status Bar -->
    {#if mode === 'collaborative'}
        <div class="collaboration-bar">
            <div class="connection-status" class:connected={isConnected}>
                {#if isConnected}
                    <span class="indicator connected"></span>
                    Live collaboration
                {:else}
                    <span class="indicator disconnected"></span>
                    Reconnecting...
                {/if}
            </div>
            
            <div class="collaborators">
                {#each collaborators as collaborator (collaborator.id)}
                    <img 
                        src={collaborator.user.avatar} 
                        alt={collaborator.user.name}
                        title={collaborator.user.name}
                        class="collaborator-avatar"
                        style="border-color: {collaborator.user.color}"
                    />
                {/each}
            </div>
            
            <button on:click={() => mode = 'solo'} class="exit-collab">
                Exit Collaboration
            </button>
        </div>
    {/if}
    
    <!-- Text Editor -->
    <div 
        bind:this={editorElement}
        class="editor-content"
        on:cursorMove={handleCursorMove}
    >
        {initialContent}
        
        <!-- Remote Cursors -->
        {#if mode === 'collaborative'}
            {#each collaborators as collaborator}
                {#if collaborator.cursor}
                    <div 
                        class="remote-cursor"
                        style="left: {collaborator.cursor.x}px; 
                               top: {collaborator.cursor.y}px;
                               border-color: {collaborator.user.color}"
                    >
                        <span class="cursor-label" style="background: {collaborator.user.color}">
                            {collaborator.user.name}
                        </span>
                    </div>
                {/if}
            {/each}
        {/if}
    </div>
</div>

<style>
    .collaboration-bar {
        display: flex;
        align-items: center;
        gap: 1rem;
        padding: 0.5rem;
        background: var(--surface);
        border-bottom: 1px solid var(--border);
    }
    
    .connection-status {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 0.9rem;
    }
    
    .indicator {
        width: 8px;
        height: 8px;
        border-radius: 50%;
    }
    
    .indicator.connected {
        background: var(--success);
    }
    
    .indicator.disconnected {
        background: var(--warning);
        animation: pulse 1s infinite;
    }
    
    .collaborators {
        display: flex;
        gap: 0.25rem;
    }
    
    .collaborator-avatar {
        width: 24px;
        height: 24px;
        border-radius: 50%;
        border: 2px solid;
    }
    
    .remote-cursor {
        position: absolute;
        pointer-events: none;
        z-index: 100;
    }
    
    .remote-cursor::before {
        content: '';
        display: block;
        width: 2px;
        height: 20px;
        background: currentColor;
    }
    
    .cursor-label {
        position: absolute;
        top: -24px;
        left: 0;
        font-size: 0.8rem;
        color: white;
        padding: 2px 6px;
        border-radius: 4px;
        white-space: nowrap;
    }
    
    @keyframes pulse {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.5; }
    }
</style>
```

## Conflict Resolution Strategies

### 1. CRDT-Based Resolution (Automatic)
Yjs handles most conflicts automatically through operation transformation:

```typescript
// Example: Two users typing simultaneously
// User A types "Hello" at position 0
// User B types "World" at position 0
// Result: "WorldHello" or "HelloWorld" (deterministic based on client IDs)

class ConflictResolution {
    // Yjs automatically resolves these types of conflicts:
    // - Concurrent insertions
    // - Overlapping deletions
    // - Mixed operations (insert + delete)
    
    // Manual resolution only needed for semantic conflicts
    async resolveSemanticConflict(local: Node, remote: Node): Promise<Node> {
        if (this.isStructuralConflict(local, remote)) {
            return await this.promptUserResolution(local, remote);
        }
        
        // Use operational transform for text content
        return this.mergeWithOT(local, remote);
    }
}
```

### 2. Semantic Conflict Detection
For higher-level conflicts (metadata, node type changes):

```typescript
interface ConflictContext {
    localNode: Node;
    remoteNode: Node;
    conflictType: 'metadata' | 'type' | 'structure' | 'content';
    resolution: 'manual' | 'auto' | 'pending';
}

class SemanticConflictResolver {
    detectConflicts(local: Node, remote: Node): ConflictContext[] {
        const conflicts: ConflictContext[] = [];
        
        // Type conflicts
        if (local.type !== remote.type) {
            conflicts.push({
                localNode: local,
                remoteNode: remote,
                conflictType: 'type',
                resolution: 'manual' // Requires user decision
            });
        }
        
        // Metadata conflicts
        const metadataConflicts = this.compareMetadata(
            local.metadata, 
            remote.metadata
        );
        conflicts.push(...metadataConflicts);
        
        return conflicts;
    }
}
```

## Graceful Degradation

### Offline → Online Transition
```typescript
class CollaborationManager {
    private pendingOperations: Operation[] = [];
    
    async handleConnectivityChange(isOnline: boolean): Promise<void> {
        if (isOnline && this.pendingOperations.length > 0) {
            // Reconnected - sync pending operations
            await this.syncPendingOperations();
        } else if (!isOnline) {
            // Going offline - store operations locally
            this.enableOfflineMode();
        }
    }
    
    private async syncPendingOperations(): Promise<void> {
        for (const op of this.pendingOperations) {
            try {
                await this.applyRemoteOperation(op);
            } catch (error) {
                // Conflict - queue for manual resolution
                await this.queueConflictResolution(op, error);
            }
        }
        this.pendingOperations = [];
    }
}
```

### Collaboration Session Management
```typescript
class SessionManager {
    private activeSessions = new Map<string, CollaborationSession>();
    
    async createSession(nodeId: string): Promise<CollaborationSession> {
        const session = new CollaborationSession(nodeId);
        
        // Set timeout for inactive sessions
        setTimeout(() => {
            if (session.inactiveTime > 30_000) {
                this.cleanupSession(nodeId);
            }
        }, 60_000);
        
        this.activeSessions.set(nodeId, session);
        return session;
    }
    
    async leaveSession(nodeId: string): Promise<void> {
        const session = this.activeSessions.get(nodeId);
        if (session) {
            // Save final state to local LanceDB
            await this.persistSessionState(session);
            
            // Cleanup collaborative state
            session.destroy();
            this.activeSessions.delete(nodeId);
        }
    }
}
```

## Performance Considerations

### Memory Management
```typescript
class PerformanceOptimizer {
    // Limit active collaborative sessions
    private readonly MAX_CONCURRENT_SESSIONS = 5;
    
    // Cleanup old presence states
    cleanupStalePresence(): void {
        const now = Date.now();
        const staleThreshold = 60_000; // 1 minute
        
        for (const [clientId, state] of this.awareness.getStates()) {
            if (now - state.timestamp > staleThreshold) {
                this.awareness.states.delete(clientId);
            }
        }
    }
    
    // Throttle cursor position broadcasts
    private broadcastCursor = throttle((position: CursorPosition) => {
        this.realtimeChannel.broadcast('cursor', position);
    }, 100); // Max 10 updates per second
}
```

### Network Optimization
```typescript
class NetworkOptimizer {
    // Batch operations for efficiency
    private operationBuffer: YjsOperation[] = [];
    private readonly BATCH_SIZE = 10;
    private readonly BATCH_TIMEOUT = 100; // ms
    
    queueOperation(operation: YjsOperation): void {
        this.operationBuffer.push(operation);
        
        if (this.operationBuffer.length >= this.BATCH_SIZE) {
            this.flushOperations();
        } else {
            // Set timeout for partial batch
            setTimeout(() => this.flushOperations(), this.BATCH_TIMEOUT);
        }
    }
    
    private flushOperations(): void {
        if (this.operationBuffer.length > 0) {
            this.websocket.send(this.encodeOperationBatch(this.operationBuffer));
            this.operationBuffer = [];
        }
    }
}
```

## Security and Privacy

### Collaboration Security
```typescript
class CollaborationSecurity {
    // Validate all incoming operations
    validateOperation(operation: YjsOperation, userId: string): boolean {
        // Check user permissions
        if (!this.hasEditPermission(userId, operation.nodeId)) {
            return false;
        }
        
        // Validate operation structure
        if (!this.isValidYjsOperation(operation)) {
            return false;
        }
        
        // Rate limiting
        if (this.isRateLimited(userId)) {
            return false;
        }
        
        return true;
    }
    
    // Sanitize content before broadcasting
    sanitizeContent(content: string): string {
        // Remove potentially harmful content
        return DOMPurify.sanitize(content);
    }
}
```

## Future Enhancements

### Advanced Collaboration Features
1. **Comment System**: Threaded discussions on specific text ranges
2. **Suggestion Mode**: Track changes like Google Docs suggestions  
3. **Version History**: Visual timeline of collaborative edits
4. **Role-Based Permissions**: Owner, editor, commenter, viewer roles
5. **Smart Notifications**: AI-powered relevance filtering

### Integration with NodeSpace Features
1. **Collaborative EntityNodes**: Real-time spreadsheet-like editing
2. **Shared QueryNodes**: Live-updating shared views
3. **AI Chat Collaboration**: Shared AI conversations with context
4. **Collaborative Mind Maps**: Real-time node graph editing

This collaboration strategy provides a clear path from single-user local-first editing to rich, real-time collaborative experiences while maintaining the core benefits of NodeSpace's architecture.