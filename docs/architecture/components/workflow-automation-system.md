# Workflow Automation System

> **Status**: Planned - Core infrastructure for automated workflows
> **Priority**: High - Enables AI coding methodology support and process automation
> **Dependencies**: Entity Management, Schema System, Collections

## Overview

The Workflow Automation System provides event-driven automation within NodeSpace, enabling users to define triggers, actions, and phase progressions without writing code. This system is central to NodeSpace's vision as a **local-first replacement** for tools like Jira, Linear, Confluence, and AI coding methodologies (Spec Kit, BMAD, OpenSpec).

## Vision Statement

NodeSpace becomes the **universal substrate** for development workflows:

- **Documentation System** - Replaces Confluence, Notion, wikis
- **Issue Tracking** - Replaces Jira, Linear, GitHub Issues
- **Project Management** - Replaces Asana, Monday, Trello
- **AI Coding Methodology** - Replaces Spec Kit, BMAD, OpenSpec

All running **locally**, with **user-defined schemas**, **automated workflows**, and **semantic search** across everything.

## Core Architecture

### Workflow-as-Data

Workflows are stored as nodes, not code. This means:

- Workflows are queryable, versionable, and shareable
- Users can create, modify, and fork workflows through the UI
- AI agents can read and understand workflow definitions
- No vendor lock-in - export your workflows anytime

```
Workflow Node Structure:
├── workflow (root)
│   ├── name: "Spec-Driven Development"
│   ├── description: "..."
│   ├── phases: [phase references]
│   ├── triggers: [trigger definitions]
│   └── version: 1
│
├── workflow-phase (children)
│   ├── phase_id: "specify"
│   ├── output_schema: "spec"
│   ├── completion_criteria: {...}
│   ├── agent_context: {...}  // AI instructions if delegated
│   └── next_phase: → "plan"
```

### Event-Driven Execution

The workflow engine watches for node changes and evaluates triggers:

```
┌──────────────────────────────────────────────────────────┐
│   Node Changed (property updated, node created, etc.)    │
│                          │                               │
│                          ▼                               │
│   ┌──────────────────────────────────────────────────┐   │
│   │  Workflow Engine: Evaluate Triggers              │   │
│   │  - Check all active workflows                    │   │
│   │  - Match trigger conditions                      │   │
│   └──────────────────────┬───────────────────────────┘   │
│                          │                               │
│              ┌───────────┴───────────┐                   │
│              ▼                       ▼                   │
│        No match                 Trigger fires            │
│        (wait)                        │                   │
│                                      ▼                   │
│                          ┌───────────────────────┐       │
│                          │  Execute Actions      │       │
│                          └───────────┬───────────┘       │
│                                      │                   │
│                                      ▼                   │
│                            Node Changed (loop)           │
└──────────────────────────────────────────────────────────┘
```

### Three-Actor Model

| Actor | Role |
|-------|------|
| **System** | Executes workflow, checks triggers, advances phases |
| **Human** | Completes tasks, reviews output, approves gates |
| **AI Agent** | Drafts content when delegated, follows `agent_context` instructions |

The system orchestrates; humans and AI agents do the work.

## Schema Definitions

### Workflow Schema

```javascript
{
  node_type: "workflow",
  description: "Automated workflow definition",
  fields: [
    {
      name: "name",
      type: "text",
      required: true,
      description: "Workflow display name"
    },
    {
      name: "description",
      type: "text",
      description: "What this workflow accomplishes"
    },
    {
      name: "is_active",
      type: "boolean",
      default: true,
      description: "Whether workflow is currently executing"
    },
    {
      name: "entry_phase",
      type: "text",
      description: "ID of the first phase"
    },
    {
      name: "version",
      type: "number",
      default: 1,
      description: "Workflow version for history tracking"
    }
  ],
  relationships: [
    { name: "phases", target: "workflow-phase", type: "has_many" },
    { name: "triggers", target: "workflow-trigger", type: "has_many" }
  ]
}
```

### Workflow Phase Schema

```javascript
{
  node_type: "workflow-phase",
  description: "A phase/stage within a workflow",
  fields: [
    {
      name: "phase_id",
      type: "text",
      required: true,
      indexed: true,
      description: "Unique identifier within workflow (e.g., 'specify', 'plan')"
    },
    {
      name: "display_name",
      type: "text",
      description: "Human-readable phase name"
    },
    {
      name: "output_schema",
      type: "text",
      description: "Node type created by this phase (e.g., 'spec', 'plan', 'task')"
    },
    {
      name: "completion_criteria",
      type: "object",
      description: "Conditions that must be met to advance",
      // Example: { required_fields: ["content"], status: "approved" }
    },
    {
      name: "assignable_to",
      type: "array",
      itemType: "text",
      default: ["human", "ai-agent"],
      description: "Who can complete this phase"
    },
    {
      name: "agent_context",
      type: "object",
      description: "Instructions for AI agent if delegated",
      // Example: {
      //   instruction: "Create a technical plan based on the spec",
      //   input_refs: ["$spec"],
      //   review_required: true
      // }
    },
    {
      name: "ui_prompt",
      type: "text",
      description: "Prompt shown to user in this phase"
    }
  ],
  relationships: [
    { name: "next_phase", target: "workflow-phase", type: "has_one" },
    { name: "required_inputs", target: "schema", type: "has_many" }
  ]
}
```

### Workflow Trigger Schema

```javascript
{
  node_type: "workflow-trigger",
  description: "Event-based trigger for workflow actions",
  fields: [
    {
      name: "name",
      type: "text",
      required: true,
      description: "Trigger identifier"
    },
    {
      name: "trigger_type",
      type: "enum",
      coreValues: [
        { value: "property_change", label: "Property Changed" },
        { value: "node_created", label: "Node Created" },
        { value: "node_deleted", label: "Node Deleted" },
        { value: "condition_met", label: "Condition Met" },
        { value: "schedule", label: "Scheduled" }
      ],
      required: true
    },
    {
      name: "watch_node_type",
      type: "text",
      description: "Node type to watch (e.g., 'spec', 'task')"
    },
    {
      name: "condition",
      type: "object",
      description: "Condition expression to evaluate",
      // Example: { field: "status", operator: "equals", value: "approved" }
    },
    {
      name: "actions",
      type: "array",
      itemType: "object",
      description: "Actions to execute when trigger fires"
      // Example: [
      //   { type: "create_node", schema: "plan", link_to: "$trigger_node" },
      //   { type: "set_property", target: "$created_node", field: "status", value: "draft" },
      //   { type: "assign", target: "$created_node", to: "$trigger_node.author" }
      // ]
    }
  ]
}
```

### Workflow Action Types

```javascript
// Available action types within triggers
{
  actions: [
    // Create a new node
    {
      type: "create_node",
      schema: "plan",           // Node type to create
      link_to: "$trigger_node", // Create relationship to triggering node
      properties: {             // Initial properties
        status: "draft"
      }
    },

    // Update a property
    {
      type: "set_property",
      target: "$trigger_node",  // or "$created_node", specific node ID
      field: "status",
      value: "in_progress"
    },

    // Assign to user or agent
    {
      type: "assign",
      target: "$created_node",
      to: "$trigger_node.author"  // or "ai-agent", specific user
    },

    // Advance workflow phase
    {
      type: "set_phase",
      phase: "implement",
      context: "$trigger_node"
    },

    // Create notification (future)
    {
      type: "notify",
      channel: "in-app",
      message: "Spec approved, ready for planning"
    }
  ]
}

// Variable references:
// $trigger_node - The node that triggered this action
// $created_node - Node created by a create_node action in this trigger
// $workflow - The workflow definition
// $phase - Current phase
```

## Generic Workflow System (No Methodology-Specific Code)

### Design Principle

**NodeSpace has ZERO code specific to Spec Kit, BMAD, OpenSpec, or any other methodology.**

The workflow system is generic and flexible. These methodologies serve as **litmus tests** to validate that the workflow engine is robust enough to support any user-defined process. If a user wants a Spec Kit-style workflow, they create it themselves using the same primitives available to everyone.

### What Methodologies Need (Our Validation Criteria)

These tools typically provide:
1. **Prompts/Templates** - How to write specs, plans, tasks
2. **Folder Conventions** - Where artifacts live
3. **Workflow Phases** - Specify → Plan → Tasks → Implement
4. **CLI Commands** - Trigger phase transitions

NodeSpace's generic workflow system provides all of this as **data**:

| Tool Feature | NodeSpace Equivalent |
|--------------|---------------------|
| Prompts/templates | `agent_context.instruction` in workflow-phase |
| Folder structure | Collections + node hierarchy |
| Workflow phases | workflow-phase nodes with transitions |
| CLI commands | Triggers fire on property changes |
| Markdown files | Nodes with semantic search |

### Example: User-Created Spec-Driven Workflow

The following example shows how a **user** (not NodeSpace) would define a spec-driven workflow. This is purely user data, not built-in functionality:

```javascript
// Workflow definition
{
  node_type: "workflow",
  name: "Spec-Driven Development",
  description: "Based on GitHub Spec Kit methodology",
  entry_phase: "specify",

  // Stored as child nodes
  phases: ["specify", "plan", "tasks", "implement"]
}

// Phase: Specify
{
  node_type: "workflow-phase",
  phase_id: "specify",
  display_name: "Specify",
  output_schema: "spec",
  ui_prompt: "Describe what you want to build. What problem does it solve?",
  agent_context: {
    instruction: "Create a specification document with user stories and acceptance criteria",
    review_required: true
  },
  completion_criteria: {
    required_fields: ["user_stories", "acceptance_criteria"],
    status: "approved"
  },
  next_phase: → "plan"
}

// Phase: Plan
{
  node_type: "workflow-phase",
  phase_id: "plan",
  display_name: "Plan",
  output_schema: "plan",
  ui_prompt: "Review the spec. Ready to create a technical plan?",
  agent_context: {
    instruction: "Create a technical plan covering architecture, dependencies, and approach",
    input_refs: ["$spec"],
    review_required: true
  },
  completion_criteria: {
    status: "approved"
  },
  next_phase: → "tasks"
}

// Triggers
{
  node_type: "workflow-trigger",
  name: "spec-approved-create-plan",
  trigger_type: "property_change",
  watch_node_type: "spec",
  condition: { field: "status", operator: "equals", value: "approved" },
  actions: [
    { type: "create_node", schema: "plan", link_to: "$trigger_node" },
    { type: "assign", target: "$created_node", to: "$trigger_node.author" },
    { type: "set_phase", phase: "plan" }
  ]
}
```

### Starter Templates (Optional, User-Deletable)

NodeSpace may ship with a few **example** workflow templates to help users get started. These are purely educational - users can delete, modify, or ignore them. They demonstrate what's possible, not what's required:

```
Example Templates (user data, not core functionality):
├── simple-kanban
│   └── Todo → In Progress → Done
├── spec-driven-example
│   └── Specify → Plan → Tasks → Implement
├── review-workflow
│   └── Draft → Review → Approved
├── content-lifecycle
│   └── Active → Needs Review → Archived
└── (users create their own)
```

**These templates are validated by ensuring the workflow engine is generic enough to support them without special-case code.**

---

## Use Case Examples

The following examples demonstrate how the generic workflow system supports diverse use cases. These are **not built-in features** - they're user-configurable templates that validate the workflow engine's flexibility.

### Use Case 1: Spec-Driven Development (AI Coding Methodologies)

This template supports methodologies like Spec Kit, BMAD, and OpenSpec - phased development with AI agent delegation.

**Schema Requirements:**
```javascript
// User creates these schemas
{ node_type: "spec", fields: ["user_stories", "acceptance_criteria", "status"] }
{ node_type: "plan", fields: ["architecture", "dependencies", "status"] }
{ node_type: "task", fields: ["description", "files_to_modify", "status", "assignee"] }
```

**Workflow Definition:**
```javascript
{
  node_type: "workflow",
  name: "Spec-Driven Development",
  entry_phase: "specify",
  phases: ["specify", "plan", "tasks", "implement"]
}
```

**Triggers:**
```javascript
// Spec approved → Create plan
{
  trigger_type: "property_change",
  watch_node_type: "spec",
  condition: { field: "status", equals: "approved" },
  actions: [
    { type: "create_node", schema: "plan", link_to: "$trigger_node" },
    { type: "set_phase", phase: "plan" }
  ]
}

// Plan approved → Generate tasks
{
  trigger_type: "property_change",
  watch_node_type: "plan",
  condition: { field: "status", equals: "approved" },
  actions: [
    { type: "set_phase", phase: "tasks" }
    // AI agent can be delegated to break plan into tasks
  ]
}

// All tasks completed → Mark workflow done
{
  trigger_type: "condition_met",
  condition: {
    all_children: { node_type: "task", field: "status", equals: "done" }
  },
  actions: [
    { type: "set_property", target: "$workflow", field: "status", value: "completed" }
  ]
}
```

**Why This Works:**
- No Spec Kit/BMAD-specific code in NodeSpace
- Users define their own schemas (spec, plan, task)
- Triggers are user-configurable
- AI agents receive context via graph traversal

---

### Use Case 2: Content Lifecycle Management (Knowledge Governance)

This template addresses the "stale Confluence page" problem - automated detection and archival of outdated content.

**The Problem:**
> "Poor data quality costs organizations an average of $12.9 million annually" - Gartner
>
> Without lifecycle management, knowledge bases become graveyards of outdated, conflicting information that users stop trusting.

**Core Property (Built-in):**

`lifecycle_status` is a **core property on all nodes** (like `node_type`, `created_at`):

```javascript
// Every node has this - no schema extension needed
{
  lifecycle_status: 'active' | 'archived'  // Core property, not user-defined
}
```

- **Semantic search** excludes `archived` nodes by default
- Use `search_semantic({ ..., include_archived: true })` to include archived content
- See [SurrealDB Schema](../data/surrealdb-schema-design.md#lifecycle-status-core-hub-property) for details

**Optional Schema Extensions (User-Defined):**

For advanced lifecycle workflows, users can add these properties via schema:

```javascript
// Optional user-defined properties for review workflows
{
  fields: [
    { name: "content_owner", type: "reference", description: "Who's responsible for this content" },
    { name: "review_interval", type: "duration", description: "How often to review (e.g., '90 days', '6 months')" },
    { name: "last_reviewed_at", type: "datetime" },
    { name: "next_review_at", type: "datetime", calculated: true }  // Based on last_reviewed + interval
  ]
}
```

**Workflow Definition:**
```javascript
{
  node_type: "workflow",
  name: "Content Lifecycle Management",
  description: "Automated detection and archival of stale content",
  is_active: true
}
```

**Triggers:**

```javascript
// 1. Flag content needing review (runs on schedule)
{
  name: "flag-stale-content",
  trigger_type: "schedule",
  schedule: "daily",
  watch_node_type: "*",  // All node types with lifecycle properties
  condition: {
    and: [
      { field: "lifecycle_status", equals: "active" },
      { field: "next_review_at", operator: "past_due" }
    ]
  },
  actions: [
    { type: "set_property", field: "lifecycle_status", value: "needs_review" }
  ]
}

// 2. Notify content owner when review needed
{
  name: "notify-review-needed",
  trigger_type: "property_change",
  condition: { field: "lifecycle_status", changed_to: "needs_review" },
  actions: [
    { type: "notify",
      to: "$trigger_node.content_owner",
      message: "Content review needed: $trigger_node.content",
      link: "$trigger_node" }
  ]
}

// 3. Auto-archive after extended inactivity
{
  name: "auto-archive-abandoned",
  trigger_type: "schedule",
  schedule: "weekly",
  condition: {
    and: [
      { field: "lifecycle_status", equals: "needs_review" },
      { field: "modified_at", older_than: "365 days" },
      { field: "last_viewed_at", older_than: "180 days" }
    ]
  },
  actions: [
    { type: "set_property", field: "lifecycle_status", value: "archived" },
    { type: "add_to_collection", collection: "Archive/$year" },
    { type: "notify",
      to: "$trigger_node.content_owner",
      message: "Auto-archived due to inactivity: $trigger_node.content" }
  ]
}

// 4. Mark reviewed when content is updated
{
  name: "mark-reviewed-on-update",
  trigger_type: "property_change",
  watch_field: "content",  // When content changes
  condition: { field: "lifecycle_status", in: ["active", "needs_review"] },
  actions: [
    { type: "set_property", field: "lifecycle_status", value: "active" },
    { type: "set_property", field: "last_reviewed_at", value: "$now" }
  ]
}

// 5. Restore archived content when accessed
{
  name: "restore-on-access",
  trigger_type: "node_viewed",
  condition: { field: "lifecycle_status", equals: "archived" },
  actions: [
    { type: "set_property", field: "lifecycle_status", value: "needs_review" },
    { type: "notify",
      to: "$trigger_node.content_owner",
      message: "Archived content accessed - please review: $trigger_node.content" }
  ]
}
```

**Different Lifecycles for Different Content:**
```javascript
// Meeting notes - never expire
{ node_type: "meeting-notes", review_interval: null }

// Product documentation - quarterly review
{ node_type: "product-doc", review_interval: "90 days" }

// Onboarding guides - annual review
{ node_type: "onboarding", review_interval: "365 days" }

// Project specs - archive 30 days after project completion
{ node_type: "spec", archive_after: "project.status == 'completed' + 30 days" }
```

**UI Treatment (User-Configurable):**
```
Stale content indicators:
├── Banner: "⚠️ Last updated 18 months ago - may be outdated"
├── Badge: Review status indicator
├── Muted styling for archived content
└── Dashboard: "12 nodes need review this week"
```

**Why This Works:**
- No "content lifecycle" code in NodeSpace core
- Uses standard schema properties + calculated fields
- Triggers are user-configurable (intervals, actions, notifications)
- Integrates with Collections for organization
- Users can modify thresholds, disable auto-archive, etc.

---

### Use Case 3: Simple Kanban

A minimal workflow for task tracking.

**Workflow Definition:**
```javascript
{
  node_type: "workflow",
  name: "Simple Kanban",
  phases: ["todo", "in_progress", "done"]
}

// Single trigger: track time in progress
{
  trigger_type: "property_change",
  watch_node_type: "task",
  condition: { field: "status", changed_to: "in_progress" },
  actions: [
    { type: "set_property", field: "started_at", value: "$now" }
  ]
}
```

---

### Template Validation Criteria

The workflow engine must support ALL of these use cases without special-case code:

| Use Case | Key Requirements |
|----------|-----------------|
| Spec-Driven Development | Phased progression, AI delegation, context assembly |
| Content Lifecycle | Scheduled triggers, date comparisons, auto-archival |
| Simple Kanban | Minimal config, property tracking |
| Review Workflow | Approval gates, notifications |
| Custom (User-Defined) | Any combination of triggers and actions |

If a use case requires NodeSpace code changes, the workflow engine is insufficiently generic.

### AI Agent Context Assembly

When an AI agent is delegated a task, the system automatically gathers context:

```javascript
// Task node being implemented
task:xyz
  └── linked_to: plan:abc
        └── linked_to: spec:123

// System provides to AI agent:
{
  task: {
    content: "Create login API endpoint",
    status: "in_progress",
    files_to_modify: ["src/api/auth/login.ts"]
  },
  plan: {
    architecture: "REST + JWT",
    dependencies: ["bcrypt", "jose"]
  },
  spec: {
    user_stories: ["As a user, I want to log in..."],
    acceptance_criteria: ["Return 401 on invalid credentials"]
  },
  similar_tasks: [
    // Semantic search finds related past work
    { content: "Create signup API endpoint", files_modified: [...] }
  ]
}
```

## Comparison: Spec Kit CLI vs NodeSpace

| Aspect | Spec Kit (CLI) | NodeSpace |
|--------|---------------|-----------|
| **Trigger** | Human runs command | Property change |
| **State** | File existence | Node properties |
| **Orchestration** | Human remembers | System enforces |
| **Context** | Manual file paths | Graph traversal |
| **Search** | grep / text search | Semantic search |
| **History** | Git blame | Node versions |
| **Sharing** | Clone repo | Share workflow node |
| **Location** | Cloud / external | **Local, private, yours** |

## The Local-First Advantage

```
Cloud workflow (Spec Kit + Copilot):
  Code → GitHub → Copilot API → OpenAI servers → back
  Your proprietary code touches external servers

NodeSpace workflow:
  Code → Local NodeSpace → Local LLM (optional) → Done
  Nothing leaves your machine
```

**Value proposition:**
- Your specs on YOUR machine
- Your workflows are YOUR data
- No subscription fees
- Works offline
- They can't train on your data

## Integration with Entity Management

Workflows build on the Entity Management system:

- **Schemas** define the structure of specs, plans, tasks
- **Calculated fields** can compute workflow state (e.g., "all tasks done?")
- **Validation rules** enforce completion criteria
- **AI extraction** helps populate fields from natural language

See [Entity Management](./entity-management.md) for schema and field details.

## Integration with Collections

Workflows can organize output into Collections:

```javascript
// Trigger action to add to collection
{
  type: "add_to_collection",
  target: "$created_node",
  collection: "active-projects/$project_name/specs"
}
```

See [Collections System](./collections-system.md) for organization details.

## Future Enhancements

### Version History

Every node change creates a version entry:

```javascript
node:abc
├── version: 5
├── content: "current content"
├── modified_at: "2025-01-15T10:00:00Z"
└── history: [
      { version: 4, content: "...", modified_at: "...", modified_by: "..." },
      { version: 3, ... }
    ]
```

Enables:
- Restore to previous version
- Diff between versions
- Audit trail for compliance

### Conditional Branching

```javascript
{
  type: "branch",
  condition: "$spec.complexity == 'high'",
  true_actions: [
    { type: "create_node", schema: "architecture-review" }
  ],
  false_actions: [
    { type: "set_phase", phase: "tasks" }  // Skip to tasks
  ]
}
```

### Parallel Phases

```javascript
{
  phase_id: "review",
  parallel_with: ["security-review", "ux-review"],
  completion: "all",  // or "any"
  next_phase: "implement"
}
```

### External Integrations (Optional)

For teams that need it:

```javascript
{
  type: "sync_external",
  adapter: "github-issues",
  action: "create",
  mapping: {
    title: "$node.content",
    labels: ["from-nodespace"]
  }
}
```

But this is optional - NodeSpace IS the system, not a bridge.

## Implementation Notes

### Workflow Engine Service

```rust
pub struct WorkflowEngine {
    trigger_evaluator: TriggerEvaluator,
    action_executor: ActionExecutor,
    phase_manager: PhaseManager,
    context_assembler: ContextAssembler,
}

impl WorkflowEngine {
    pub async fn on_node_changed(&self, event: NodeChangeEvent) -> Result<()> {
        // 1. Find matching triggers
        let triggers = self.trigger_evaluator
            .find_matching_triggers(&event).await?;

        // 2. Execute actions for each trigger
        for trigger in triggers {
            self.action_executor
                .execute_actions(&trigger.actions, &event).await?;
        }

        Ok(())
    }
}
```

### Query Enhancements Needed

To support workflows and browsing, `query_nodes` needs:

1. **`is_root: true`** - Filter for nodes without parents
2. **Property filters** - Filter by any property value
3. **`order_by`** - Expose sorting options (modified_desc, created_desc, etc.)
4. **Collection membership** - Filter by collection

See [Query Service](../data/query-service.md) for implementation details.

---

## Summary

The Workflow Automation System transforms NodeSpace from a knowledge management tool into a **complete development platform**:

- **Workflows as data** - Stored as nodes, not code
- **Event-driven execution** - System orchestrates, humans/AI do work
- **AI methodology support** - First-class support for Spec Kit, BMAD, etc.
- **Local-first** - Private, offline-capable, no vendor lock-in
- **Extensible** - Users define their own workflows, schemas, and processes

Combined with Entity Management, Collections, and Semantic Search, this positions NodeSpace as the unified local system for all development artifacts.
