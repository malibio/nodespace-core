//! Prompt assembly service: graph-only prompt composition.
//!
//! Composes the final agent prompt exclusively from prompt nodes stored in the
//! knowledge graph, assembled in natural child order. Supports Minijinja template rendering.
//! If no prompt nodes are found (corrupted/empty database), falls back to a
//! minimal emergency prompt and logs a warning.
//!
//! Issue #1049, ADR-030 Phase 2.

use std::sync::Arc;

use nodespace_core::markdown::NodeTemplate;
use nodespace_core::models::Node;
use nodespace_core::services::NodeService;

use crate::agent_guidance::{NODE_REFERENCE_FORMAT, SCHEMA_CREATION_RULES, TOOL_STRATEGY_RULES};
use crate::agent_types::ToolDefinition;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Context variables available to Minijinja templates.
#[derive(Debug, Clone, serde::Serialize)]
pub struct TemplateContext {
    pub current_date: String,
    pub model_name: String,
    pub workspace_context: String,
}

/// The assembled prompt ready for inference.
#[derive(Debug, Clone)]
pub struct AssembledPrompt {
    /// Full system prompt text (base + graph overrides)
    pub system_prompt: String,
    /// Tool definitions (may be scoped by active skill in future)
    pub tool_schemas: Vec<ToolDefinition>,
}

// ---------------------------------------------------------------------------
// PromptAssembler
// ---------------------------------------------------------------------------

/// Maximum number of prompt nodes to fetch from the graph.
const MAX_PROMPT_NODES: usize = 50;

/// Minimal emergency fallback when no prompt nodes exist in the graph.
/// This should only fire on corrupted/empty databases — normal operation
/// reads all prompt content from graph nodes seeded on first run.
const EMERGENCY_FALLBACK_PROMPT: &str = "\
You are NodeSpace's built-in assistant. You help users work with their \
knowledge graph — creating, finding, updating, and connecting nodes.\n\n\
Use the available tools to accomplish tasks. Summarize results in natural language.";

/// Assembles final prompts exclusively from graph-stored prompt nodes.
///
/// The assembly order is:
/// 1. Fetch root prompt nodes from the graph
/// 2. For each prompt node, fetch children in natural child order and concatenate
/// 3. Render through Minijinja with context variables
/// 4. If no prompt nodes found, use emergency fallback and log a warning
pub struct PromptAssembler {
    node_service: Arc<NodeService>,
}

impl PromptAssembler {
    pub fn new(node_service: Arc<NodeService>) -> Self {
        Self { node_service }
    }

    /// Assemble the final prompt from graph-stored prompt nodes only.
    ///
    /// `template_ctx` provides variables for Minijinja template rendering, including
    /// `workspace_context` (entity types, collections, playbooks).
    /// `tools` are the available tool definitions (passed through, may be scoped by skill later).
    pub async fn assemble(
        &self,
        template_ctx: &TemplateContext,
        tools: Vec<ToolDefinition>,
    ) -> AssembledPrompt {
        // 1. Fetch root prompt nodes from the graph
        let prompt_nodes = self.fetch_prompt_overrides().await;

        // 2. If no prompt nodes found, use emergency fallback
        if prompt_nodes.is_empty() {
            tracing::warn!(
                "No prompt nodes found in graph — using emergency fallback. \
                 Seed prompt nodes on first run to restore full functionality."
            );
            return AssembledPrompt {
                system_prompt: EMERGENCY_FALLBACK_PROMPT.to_string(),
                tool_schemas: tools,
            };
        }

        // 3. Fetch children for each prompt node, render through minijinja, and concatenate
        let mut sections = Vec::new();

        for node in &prompt_nodes {
            // Fetch children and concatenate their content as the prompt body
            let body = self.fetch_prompt_body(node).await;
            if body.trim().is_empty() {
                continue;
            }
            let rendered = Self::render_template(&body, template_ctx);
            sections.push(rendered);
        }

        let system_prompt = sections.join("\n\n");

        AssembledPrompt {
            system_prompt,
            tool_schemas: tools,
        }
    }

    /// Fetch root-level prompt nodes from the graph (no parent).
    async fn fetch_prompt_overrides(&self) -> Vec<Node> {
        let filter = nodespace_core::ops::node_ops::QueryNodesInput {
            node_type: Some("prompt".to_string()),
            parent_id: None,
            root_id: None,
            limit: Some(MAX_PROMPT_NODES),
            offset: None,
            collection_id: None,
            collection: None,
            filters: None,
        };

        let all_nodes: Vec<Node> = match nodespace_core::ops::node_ops::query_nodes(
            &self.node_service,
            filter,
        )
        .await
        {
            Ok(result) => result
                .nodes
                .into_iter()
                .filter_map(|v| match serde_json::from_value(v) {
                    Ok(node) => Some(node),
                    Err(e) => {
                        tracing::warn!(error = %e, "Failed to deserialize prompt node, skipping");
                        None
                    }
                })
                .collect(),
            Err(e) => {
                tracing::warn!(error = %e, "Failed to fetch prompt overrides, using base only");
                return Vec::new();
            }
        };

        // Keep only true root nodes (no parent edge pointing to them).
        // query_nodes ignores parent_id filter, so all prompt nodes are returned;
        // we must post-filter to avoid treating mid-hierarchy nodes as roots.
        let mut roots = Vec::new();
        for node in all_nodes {
            match self.node_service.get_parent(&node.id).await {
                Ok(None) => roots.push(node),
                Ok(Some(_)) => {} // has a parent — skip
                Err(e) => {
                    tracing::warn!(error = %e, node_id = %node.id, "Failed to check parent, skipping node");
                }
            }
        }
        roots
    }

    /// Fetch children of a prompt node and concatenate their content as the body.
    /// Uses get_children for edge-based graph traversal in natural fractional order.
    async fn fetch_prompt_body(&self, node: &Node) -> String {
        match self.node_service.get_children(&node.id).await {
            Ok(children) => children
                .iter()
                .map(|c| c.content.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            Err(e) => {
                tracing::warn!(error = %e, node_id = %node.id, "Failed to fetch prompt children");
                String::new()
            }
        }
    }

    /// Render a Minijinja template with the given context.
    ///
    /// On error, returns the raw template text and logs a warning.
    /// Template errors should never crash the turn.
    ///
    /// Note: auto-escaping is intentionally disabled (minijinja default) because
    /// output goes into a system prompt, not HTML. Do not enable HTML escaping.
    fn render_template(template_str: &str, ctx: &TemplateContext) -> String {
        let env = minijinja::Environment::new();
        match env.render_str(template_str, ctx) {
            Ok(rendered) => rendered,
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    "Minijinja template render failed, using raw content"
                );
                template_str.to_string()
            }
        }
    }

    /// Assemble prompt with an active skill context injected.
    ///
    /// When a skill is active:
    /// 1. Graph-only prompt assembly (same as regular)
    /// 2. Skill header with name and description
    /// 3. Tool whitelist applied to tool schemas
    pub async fn assemble_with_skill(
        &self,
        template_ctx: &TemplateContext,
        tools: Vec<ToolDefinition>,
        skill: &Node,
    ) -> AssembledPrompt {
        // Regular assembly first
        let mut assembled = self.assemble(template_ctx, tools).await;

        // Add skill context
        let skill_name = &skill.content;
        let skill_desc = skill
            .properties
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let skill_section = format!(
            "\n\nACTIVE SKILL: {}\n{}\n\
             Focus on this skill's capabilities. Use only the tools provided.",
            skill_name, skill_desc
        );

        assembled.system_prompt.push_str(&skill_section);
        assembled
    }

    /// Assemble the base system prompt from seed nodes without a database.
    ///
    /// Uses `markdown_content` as the prompt body for each seed, rendered
    /// through Minijinja. Intended for use in unit/integration tests where
    /// no DB is available.
    pub fn assemble_static(workspace_context: &str, current_date: Option<&str>) -> String {
        let seeds = Self::seed_prompt_nodes();

        let ctx = TemplateContext {
            current_date: current_date.unwrap_or("2025-01-01").to_string(),
            model_name: "test".to_string(),
            workspace_context: workspace_context.to_string(),
        };

        let sections: Vec<String> = seeds
            .iter()
            .filter_map(|s| {
                // Body is the markdown_content (child content)
                let body = &s.markdown_content;
                if body.trim().is_empty() {
                    return None;
                }
                Some(Self::render_template(body, &ctx))
            })
            .collect();

        sections.join("\n\n")
    }

    /// Get seed prompt templates for first-run creation.
    ///
    /// Each [`NodeTemplate`] produces a prompt root node with text child nodes for body content.
    /// All prompt content lives in these graph nodes — there is no hardcoded
    /// base prompt.  Users can customise any seed by editing the graph node.
    ///
    /// Use [`nodespace_core::markdown::prepare_nodes_from_template`]
    /// to expand into a [`PreparedNode`] for insertion via `NodeService`.
    pub fn seed_prompt_nodes() -> Vec<NodeTemplate> {
        vec![
            NodeTemplate {
                title: "Core Identity".to_string(),
                content: None,
                root_node_type: "prompt".to_string(),
                root_properties: serde_json::json!({}),
                child_node_type: Some("text".to_string()),
                child_properties: None,
                markdown_content: "You are NodeSpace's built-in assistant. You help users work with their \
                    knowledge graph — creating, finding, updating, and connecting nodes."
                        .to_string(),
            },
            NodeTemplate {
                title: "Workspace Context Template".to_string(),
                content: None,
                root_node_type: "prompt".to_string(),
                root_properties: serde_json::json!({}),
                child_node_type: Some("text".to_string()),
                child_properties: None,
                markdown_content: "Current date: {{ current_date }}\nActive model: {{ model_name }}\n\n{{ workspace_context }}"
                    .to_string(),
            },
            NodeTemplate {
                title: "Tool Strategy Guide".to_string(),
                content: None,
                root_node_type: "prompt".to_string(),
                root_properties: serde_json::json!({}),
                child_node_type: Some("text".to_string()),
                child_properties: None,
                markdown_content: format!("{}\n\n{}", SCHEMA_CREATION_RULES, TOOL_STRATEGY_RULES),
            },
            NodeTemplate {
                title: "Response Formatting Rules".to_string(),
                content: None,
                root_node_type: "prompt".to_string(),
                root_properties: serde_json::json!({}),
                child_node_type: Some("text".to_string()),
                child_properties: None,
                markdown_content: format!(
                    "RESPONSE RULES:\n\
                    - Call tools immediately when intent is clear. Do NOT narrate your plan or reasoning first.\n\
                    - After tool results: respond in natural language. Never paste raw JSON.\n\
                    - {}\n\
                    - Tool call enums: exact schema values (\"done\", \"in_progress\"). User-facing: friendly labels (\"Done\").\n\
                    - Listing: **Title** (nodespace://id) — description. Search results: \"Found N nodes...\" then top results.\n\
                    - Empty/error tool result: state it in one sentence and stop. Do NOT retry the same tool, and do NOT call another tool to compensate — just answer.\n\
                    - Keep responses to 1-2 sentences unless the user asks for detail. No preamble, no sign-off.",
                    NODE_REFERENCE_FORMAT
                ),
            },
            NodeTemplate {
                title: "Tool Call Formatting".to_string(),
                content: None,
                root_node_type: "prompt".to_string(),
                root_properties: serde_json::json!({}),
                child_node_type: Some("text".to_string()),
                child_properties: None,
                markdown_content: "TOOL CALL FORMAT: Pass arguments flat (not nested under \"properties\"/\"arguments\"). Use exact field names from the schema."
                        .to_string(),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_prompts_have_valid_properties() {
        let seeds = PromptAssembler::seed_prompt_nodes();
        assert!(seeds.len() >= 5, "Should have at least 5 seed prompts");

        for seed in &seeds {
            assert!(
                !seed.markdown_content.is_empty(),
                "Seed '{}' markdown_content must not be empty",
                seed.title
            );
            assert!(!seed.title.is_empty(), "Seed title must not be empty");
            assert_eq!(seed.root_node_type, "prompt");
        }
    }

    /// Lock in the exact bytes of the two seeds composed from `agent_guidance`
    /// constants. If a future edit to `agent_guidance.rs` or the surrounding
    /// `format!()` glue silently changes the rendered seed body, this test
    /// fails — preventing the local Ollama agent's prompt from drifting
    /// unintentionally. Edit the expected strings deliberately when you change
    /// agent guidance.
    #[test]
    fn seed_prompt_bodies_match_expected_bytes() {
        let seeds = PromptAssembler::seed_prompt_nodes();
        let by_title: std::collections::HashMap<&str, &str> = seeds
            .iter()
            .map(|s| (s.title.as_str(), s.markdown_content.as_str()))
            .collect();

        let expected_tool_strategy = "NODE MODEL: Everything is a node. Built-in types: task, text, date. Custom types need a schema first (create_schema). Once a schema exists, create instances with create_node(node_type=<schema_id>). Never call create_schema for a type already in ENTITY TYPES.\n\
            \"DATABASE\" = SCHEMA: When the user asks to set up a tracker, database, system, or \"a way to track X\" (e.g. \"create an invoice tracking database\", \"set up a CRM\"), they want a new entity TYPE — call create_schema to define it, not search or create_node. The singular entity name is the schema (an \"invoice tracking database\" → an Invoice schema).\n\n\
            TOOL STRATEGY:\n\
            - CONVERSATIONAL TURNS USE NO TOOLS. Greetings (\"hi\", \"hello\"), thanks, small talk, capability questions (\"what can you do?\"), and meta questions about yourself — answer directly in text. Do NOT call any tool, not even search_skills. Only reach for tools when the user asks you to find, create, update, delete, or connect something in their graph.\n\
            - For a real graph action: call search_skills(query) first to find a matching skill. Empty result = no skill, proceed with general tools.\n\
            - ALWAYS search first before updating or getting a node. NEVER use placeholder IDs like \"abc-123\".\n\
            - By keyword/type/property: search_nodes(query, node_type, filters). By meaning: search_semantic(query, node_types, scope, threshold, graph_boost).\n\
            - search_semantic result: if 'markdown' is non-empty, summarize from it directly — skip get_node.\n\
            - To get full content: get_node(id, format=markdown). To get connections: get_related_nodes(id).\n\
            - To update task status: search_nodes for it, then update_task_status with the real ID. To update node content: search_nodes first, then update_node with the real ID.\n\
            - To create a node: create_node(content, node_type). Pass 'properties' only if ENTITY TYPES shows fields. Include title_template fields in properties.\n\
            - To add/modify an entity type: create_schema or update_schema(schema_id).\n\
            - To connect nodes: create_relationship with names from schemas above.\n\
            - Tool arguments must be valid JSON. No comments (#) in JSON.";

        let expected_response_rules = "RESPONSE RULES:\n\
            - Call tools immediately when intent is clear. Do NOT narrate your plan or reasoning first.\n\
            - After tool results: respond in natural language. Never paste raw JSON.\n\
            - Reference nodes with bare URI: nodespace://abc-123 (no markdown links, no backticks)\n\
            - Tool call enums: exact schema values (\"done\", \"in_progress\"). User-facing: friendly labels (\"Done\").\n\
            - Listing: **Title** (nodespace://id) — description. Search results: \"Found N nodes...\" then top results.\n\
            - Empty/error tool result: state it in one sentence and stop. Do NOT retry the same tool, and do NOT call another tool to compensate — just answer.\n\
            - Keep responses to 1-2 sentences unless the user asks for detail. No preamble, no sign-off.";

        assert_eq!(
            by_title.get("Tool Strategy Guide").copied(),
            Some(expected_tool_strategy),
            "Tool Strategy Guide body drifted — review agent_guidance.rs edits"
        );
        assert_eq!(
            by_title.get("Response Formatting Rules").copied(),
            Some(expected_response_rules),
            "Response Formatting Rules body drifted — review agent_guidance.rs edits"
        );
    }

    #[test]
    fn seed_prompt_template_produces_prompt_node() {
        use nodespace_core::markdown::prepare_nodes_from_template;
        let seeds = PromptAssembler::seed_prompt_nodes();
        for seed in &seeds {
            let nodes = prepare_nodes_from_template(seed)
                .unwrap_or_else(|e| panic!("Template '{}' failed: {:?}", seed.title, e));
            assert!(!nodes.is_empty());
            let root = &nodes[0];
            assert_eq!(root.node_type, "prompt");
            assert_eq!(root.id.len(), 36, "Node ID should be a UUID");
            assert_eq!(root.id.chars().filter(|c| *c == '-').count(), 4);
            // content is the title (no content override on prompt root nodes)
            assert_eq!(root.content, seed.title);
        }
    }

    #[test]
    fn render_plain_template() {
        let plain = "Use search_semantic for meaning queries";
        // minijinja with no template syntax should pass through unchanged
        let env = minijinja::Environment::new();
        let ctx = TemplateContext {
            current_date: "2026-04-06".to_string(),
            model_name: "ministral-3b".to_string(),
            workspace_context: "test context".to_string(),
        };
        let result = env.render_str(plain, &ctx).unwrap();
        assert_eq!(result, plain);
    }

    #[test]
    fn render_minijinja_template() {
        let ctx = TemplateContext {
            current_date: "2026-04-06".to_string(),
            model_name: "ministral-3b".to_string(),
            workspace_context: "Entity types: customer, invoice".to_string(),
        };
        let template = "Date: {{ current_date }}\nModel: {{ model_name }}";
        let result = PromptAssembler::render_template(template, &ctx);
        assert!(result.contains("2026-04-06"));
        assert!(result.contains("ministral-3b"));
    }

    #[test]
    fn render_template_error_returns_raw() {
        let ctx = TemplateContext {
            current_date: "2026-04-06".to_string(),
            model_name: "test".to_string(),
            workspace_context: "".to_string(),
        };
        let bad_template = "{{ undefined_function() }}";
        let result = PromptAssembler::render_template(bad_template, &ctx);
        // Should fall back to raw template on error
        assert_eq!(result, bad_template);
    }

    #[test]
    fn template_context_serializable() {
        let ctx = TemplateContext {
            current_date: "2026-04-06".to_string(),
            model_name: "ministral-3b".to_string(),
            workspace_context: "some context".to_string(),
        };
        let json = serde_json::to_value(&ctx).unwrap();
        assert_eq!(json["current_date"], "2026-04-06");
        assert_eq!(json["model_name"], "ministral-3b");
    }
}
