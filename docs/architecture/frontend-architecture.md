# Frontend Architecture

NodeSpace frontend architecture built with Svelte 4.x, TypeScript, and Tauri 2.0.

## Core Technologies

- **Framework**: Svelte 4.x with reactive state management
- **Language**: TypeScript for type safety
- **Desktop**: Tauri 2.0 for native integration  
- **Styling**: CSS with design system variables
- **Build**: Vite with SvelteKit

## Component Architecture

### Base Components

**BaseNode** (`src/lib/design/components/base-node.svelte`)
- Core node rendering component
- Handles content editing with ContentEditableController
- Manages node positioning and hierarchy
- Integrates with icon system for visual indicators

**TextNode** (`src/lib/components/text-node.svelte`) 
- Extends BaseNode for text content
- Handles markdown processing via ContentProcessor
- Supports autoFocus and content inheritance

### Design System

**Theme System**
- CSS custom properties for colors and spacing
- Light/dark theme support
- Design tokens in `src/lib/design/tokens.ts`

**Layout Patterns**
- Node hierarchy with visual indicators
- Responsive positioning system
- CSS Grid and Flexbox layouts

## Icon System Architecture

### Component Registry

The icon system uses a component-based registry approach:

```typescript
// Registry Structure (src/lib/design/icons/registry.ts)
iconRegistry: {
  text: { component: CircleIcon, semanticClass: 'node-icon' },
  task: { component: TaskIcon, semanticClass: 'task-icon' },
  'ai-chat': { component: AIIcon, semanticClass: 'ai-icon' }
}
```

### Icon Components

**Smart Icon Wrapper** (`src/lib/design/icons/icon.svelte`)
- Semantic API: `<Icon nodeType="text" hasChildren={true} />`
- Legacy compatibility: `<Icon name="circle" size={20} />`
- Automatic semantic class assignment
- Direct component rendering without wrapper divs

**Specialized Icon Components** (`src/lib/design/icons/components/`)
- **CircleIcon.svelte**: Text and document nodes with optional ring effects
- **TaskIcon.svelte**: State-aware task indicators (pending, inProgress, completed)
- **AIIcon.svelte**: Square design with "AI" text for AI chat nodes

### CSS Class System

**Semantic Classes:**
- `.node-icon`: General-purpose nodes (text, document)
- `.task-icon`: Task nodes with state indicators  
- `.ai-icon`: AI chat nodes with square design

**Integration:**
- Classes automatically applied based on node type
- Consistent 16x16px sizing within 20x20px containers
- Ring effects for parent nodes using opacity layers

### Type Safety

```typescript
// Type Definitions (src/lib/design/icons/types.ts)
type NodeType = 'text' | 'task' | 'ai-chat' | 'entity' | 'query' | 'user' | 'document';
type NodeState = 'pending' | 'inProgress' | 'completed';

interface NodeIconProps {
  nodeType: NodeType;
  state?: NodeState;
  hasChildren?: boolean;
  size?: number;
  className?: string;
}
```

## State Management

### Node State
- Reactive stores for node data
- Event-driven updates via EventBus
- Hierarchical node relationships

### UI State  
- Theme preference and system detection
- Focus management across nodes
- Autocomplete and dropdown states

## Data Flow

```
User Interaction → BaseNode → ContentEditableController → NodeManager → EventBus → UI Updates
                             ↓
                    Icon System determines visual representation
```

## Service Layer

**Core Services:**
- **NodeManager**: CRUD operations and node lifecycle
- **ContentProcessor**: Markdown parsing and content transformation  
- **EventBus**: Decoupled communication between components
- **NodeReferenceService**: @ mention and node linking functionality

**UI Services:**
- **ComponentHydrationSystem**: Dynamic component mounting
- **PerformanceTracker**: Performance monitoring and metrics
- **CacheCoordinator**: Intelligent caching strategies

## Build Architecture

**Module Structure:**
```
src/
├── lib/
│   ├── components/          # Feature components
│   ├── design/             # Design system & base components
│   │   ├── components/     # BaseNode, theme system
│   │   └── icons/          # Icon system
│   ├── services/           # Business logic services
│   └── utils/              # Utility functions
└── routes/                 # SvelteKit routes
```

**Build Process:**
- Vite for fast development and optimized builds
- TypeScript compilation with strict type checking
- CSS processing with custom property support
- Tree-shaking for optimal bundle size

## Performance Characteristics

**Icon System:**
- Component-based rendering (no HTML injection)
- SVG-based icons for optimal scaling
- Automatic tree-shaking of unused icons
- < 20px positioning accuracy

**Component Hydration:**
- Lazy loading of complex components
- Performance tracking and metrics
- Memory leak prevention with proper cleanup

## Integration Points

**Tauri Bridge:**
- Native desktop integration
- File system access
- System theme detection

**Design System:**
- CSS custom properties for theming
- Consistent semantic class naming
- Responsive layout patterns