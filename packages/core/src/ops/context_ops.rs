//! Workspace context assembly for AI agent prompts.
//!
//! Builds a compact representation of collections and active playbooks from
//! the database. The output is formatted as a token-efficient string suitable
//! for injection into a small-model system prompt.
//!
//! Entity types are no longer included here by default — the model discovers
//! type metadata on-demand via `search_skills` which returns `schema_metadata`
//! per match. When a query + embedding service are provided, schemas
//! semantically relevant to the query are injected to cover implicit references
//! (e.g. "track my clients" → `customer` schema).
//!
//! The retrieval query itself is assembled by [`build_retrieval_query`], which
//! blends the preceding conversation turns with the current message so that
//! follow-ups referring to their subject by pronoun or ellipsis still retrieve
//! the right schema.

use crate::models::SchemaNode;
use crate::services::{CollectionService, NodeEmbeddingService, NodeService};
use std::sync::Arc;

use super::OpsError;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Assembled workspace context from the database.
#[derive(Default)]
pub struct WorkspaceContext {
    pub collections: Vec<String>,
    pub active_playbooks: Vec<PlaybookInfo>,
    /// Schemas semantically relevant to the current query (may be empty).
    pub relevant_schemas: Vec<SchemaNode>,
}

/// An active playbook.
pub struct PlaybookInfo {
    pub name: String,
    pub description: String,
}

// ---------------------------------------------------------------------------
// Builder
// ---------------------------------------------------------------------------

/// Similarity threshold for schema semantic search.
///
/// 0.2 is intentionally permissive — the full user message is embedded verbatim
/// (not entity-extracted), so action-oriented phrasing like "Add an invoice for
/// $500 due next Friday" produces a noisier embedding than a bare entity name.
/// At 0.4 the Invoice schema was missed for that query. The MAX_SEMANTIC_SCHEMAS
/// cap keeps the prompt compact even with a lower threshold.
const SCHEMA_SIMILARITY_THRESHOLD: f32 = 0.2;

/// Maximum number of schemas to inject per turn.
///
/// Five covers the common multi-entity case (primary type + related types).
/// The lower threshold above means more candidates pass; the cap keeps the
/// injected ENTITY TYPES section from bloating in large workspaces.
const MAX_SEMANTIC_SCHEMAS: usize = 5;

/// Number of preceding conversation turns blended into the retrieval query.
///
/// Two is what was measured: recall over multi-turn conversations went from 77%
/// to 100% with the last two turns prepended, while topic-switch recall held at
/// 100%. The latest message still dominates the embedding, and the cap is
/// `MAX_SEMANTIC_SCHEMAS`, not one, so the extra context adds candidates rather
/// than displacing the right one.
const BLENDED_HISTORY_TURNS: usize = 2;

/// Build the embedding query for schema retrieval from conversation context.
///
/// The last [`BLENDED_HISTORY_TURNS`] turns are prepended to `current_message`,
/// oldest first, so a follow-up that names its subject only by pronoun or
/// ellipsis ("Set the Redwood one to rejected", "Which ones are still out?")
/// still carries the discriminating words from the turn that introduced it.
///
/// Raw text is concatenated verbatim. Summarizing or entity-extracting the
/// turns first was measured and made recall *worse* (100% → 73%): the
/// abstraction discards the surface words the embedder matches on. Callers
/// should pass conversational turns only — synthetic system messages dilute the
/// query without adding discriminating terms.
///
/// This affects the embedding input only. The rendered prompt block is built
/// from the retrieved schemas and is unchanged by blending.
pub fn build_retrieval_query(prior_turns: &[&str], current_message: &str) -> String {
    let recent: Vec<&str> = prior_turns
        .iter()
        .rev()
        .map(|t| t.trim())
        .filter(|t| !t.is_empty())
        .take(BLENDED_HISTORY_TURNS)
        .collect();

    let mut parts: Vec<&str> = recent.into_iter().rev().collect();
    let current = current_message.trim();
    if !current.is_empty() {
        parts.push(current);
    }
    parts.join("\n")
}

/// Build workspace context by querying collections and playbooks.
///
/// When `embedding_service` and `query` are both provided, schema nodes
/// semantically similar to the query are retrieved and injected into the
/// context. Falls back to schema-free context when the embedding service is
/// unavailable or the query is empty.
pub async fn build_workspace_context(
    node_service: &Arc<NodeService>,
    embedding_service: Option<&Arc<NodeEmbeddingService>>,
    query: Option<&str>,
) -> Result<WorkspaceContext, OpsError> {
    // Fetch collection names
    let collection_service = CollectionService::new(node_service.store(), node_service);
    let collections = collection_service
        .get_all_collection_names()
        .await
        .unwrap_or_default();

    // Fetch active playbooks
    let playbook_nodes = node_service
        .query_nodes_by_type("playbook", Some("active"))
        .await
        .unwrap_or_default();

    // Convert playbook nodes
    let active_playbooks: Vec<PlaybookInfo> = playbook_nodes
        .into_iter()
        .map(|node| PlaybookInfo {
            name: node.content.clone(),
            description: node
                .properties
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        })
        .collect();

    // Semantic schema retrieval: find schemas relevant to the query.
    // Only runs when both an embedding service and a non-empty query are present.
    let relevant_schemas = match (embedding_service, query.filter(|q| !q.trim().is_empty())) {
        (Some(emb), Some(q)) => {
            match emb
                .semantic_search_nodes_of_type(
                    q,
                    "schema",
                    MAX_SEMANTIC_SCHEMAS,
                    SCHEMA_SIMILARITY_THRESHOLD,
                )
                .await
            {
                Ok(results) => {
                    let schemas: Vec<SchemaNode> = results
                        .into_iter()
                        .filter_map(|(node, _score)| SchemaNode::from_node(node).ok())
                        .collect();
                    tracing::debug!(
                        count = schemas.len(),
                        query = q,
                        "workspace_context: semantic schema retrieval"
                    );
                    schemas
                }
                Err(e) => {
                    tracing::warn!(error = %e, "workspace_context: semantic schema search failed, omitting schemas");
                    vec![]
                }
            }
        }
        _ => vec![],
    };

    Ok(WorkspaceContext {
        collections,
        active_playbooks,
        relevant_schemas,
    })
}

// ---------------------------------------------------------------------------
// Formatter
// ---------------------------------------------------------------------------

impl WorkspaceContext {
    /// Format context as a compact string for injection into a system prompt.
    ///
    /// Semantically-relevant schemas are injected when present (retrieved via
    /// vector similarity by `build_workspace_context` — covers implicit type
    /// references like "clients" → `customer` schema). All other entity
    /// types remain on-demand via `search_skills`.
    ///
    /// `max_chars` is a rough character budget for the combined output.
    pub fn format_for_prompt(&self, max_chars: usize) -> String {
        let mut out = String::new();

        // Collections section
        if !self.collections.is_empty() {
            let section = format!("COLLECTIONS: {}\n", self.collections.join(", "));
            if out.len() + section.len() <= max_chars {
                out.push_str(&section);
            }
        }

        // Relevant schemas section (query-matched via semantic retrieval)
        if !self.relevant_schemas.is_empty() {
            let header = "\nRELEVANT ENTITY TYPES:\n";
            if out.len() + header.len() <= max_chars {
                out.push_str(header);
                for schema in &self.relevant_schemas {
                    let field_names: Vec<&str> =
                        schema.fields.iter().map(|f| f.name.as_str()).collect();
                    let fields_str = if field_names.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", field_names.join(", "))
                    };
                    let line = format!("- {}: {}{}\n", schema.id, schema.content, fields_str);
                    if out.len() + line.len() > max_chars {
                        break;
                    }
                    out.push_str(&line);
                }
            }
        }

        // Playbooks section
        if !self.active_playbooks.is_empty() {
            let header = "\nACTIVE PLAYBOOKS:\n";
            if out.len() + header.len() <= max_chars {
                out.push_str(header);
                for pb in &self.active_playbooks {
                    let line = if pb.description.is_empty() {
                        format!("- \"{}\"\n", pb.name)
                    } else {
                        format!("- \"{}\": {}\n", pb.name, pb.description)
                    };
                    if out.len() + line.len() > max_chars {
                        break;
                    }
                    out.push_str(&line);
                }
            }
        }

        out
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_context() -> WorkspaceContext {
        WorkspaceContext {
            collections: vec!["Projects".into(), "Clients".into(), "Research".into()],
            active_playbooks: vec![PlaybookInfo {
                name: "Task completion".into(),
                description: "When task.status -> Done, evaluate project progress".into(),
            }],
            relevant_schemas: vec![],
        }
    }

    fn sample_schema(id: &str, display_name: &str, fields: &[&str]) -> crate::models::SchemaNode {
        use crate::models::schema::SchemaField;
        crate::models::SchemaNode {
            id: id.to_string(),
            content: display_name.to_string(),
            version: 1,
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            is_core: false,
            schema_version: 1,
            fields: fields
                .iter()
                .map(|name| SchemaField {
                    name: name.to_string(),
                    field_type: "string".to_string(),
                    protection: crate::models::schema::SchemaProtectionLevel::User,
                    core_values: None,
                    user_values: None,
                    indexed: false,
                    required: None,
                    extensible: None,
                    default: None,
                    description: None,
                    item_type: None,
                    fields: None,
                    item_fields: None,
                })
                .collect(),
            relationships: vec![],
            title_template: None,
            properties_header_summary_template: None,
        }
    }

    #[test]
    fn format_for_prompt_includes_collections_and_playbooks() {
        let ctx = sample_context();
        let output = ctx.format_for_prompt(4000);

        // Collections and playbooks are still injected
        assert!(output.contains("COLLECTIONS:"));
        assert!(output.contains("Projects"));
        assert!(output.contains("ACTIVE PLAYBOOKS:"));
        assert!(output.contains("Task completion"));

        // No schemas when relevant_schemas is empty
        assert!(!output.contains("RELEVANT ENTITY TYPES:"));
    }

    #[test]
    fn format_for_prompt_includes_relevant_schemas() {
        let mut ctx = sample_context();
        ctx.relevant_schemas = vec![sample_schema("customer", "Customer", &["name", "email"])];
        let output = ctx.format_for_prompt(4000);

        assert!(output.contains("RELEVANT ENTITY TYPES:"));
        assert!(output.contains("customer: Customer"));
        assert!(output.contains("name, email"));
    }

    #[test]
    fn format_for_prompt_schema_no_fields() {
        let ctx = WorkspaceContext {
            collections: vec![],
            active_playbooks: vec![],
            relevant_schemas: vec![sample_schema("invoice", "Invoice", &[])],
        };
        let output = ctx.format_for_prompt(4000);
        assert!(output.contains("invoice: Invoice\n"));
        // No parentheses when there are no fields
        assert!(!output.contains("("));
    }

    #[test]
    fn format_for_prompt_truncates_on_budget() {
        let ctx = sample_context();
        // Very small budget — output is silently capped (no truncation note emitted)
        let output = ctx.format_for_prompt(100);
        assert!(output.len() <= 100);
    }

    #[test]
    fn format_for_prompt_empty_context() {
        let ctx = WorkspaceContext {
            collections: vec![],
            active_playbooks: vec![],
            relevant_schemas: vec![],
        };
        let output = ctx.format_for_prompt(4000);
        assert!(output.is_empty());
    }

    #[test]
    fn format_for_prompt_collections_only() {
        let ctx = WorkspaceContext {
            collections: vec!["Projects".into(), "Clients".into()],
            active_playbooks: vec![],
            relevant_schemas: vec![],
        };
        let output = ctx.format_for_prompt(4000);
        assert!(output.contains("COLLECTIONS:"));
        assert!(output.contains("Projects"));
        assert!(output.contains("Clients"));
        assert!(!output.contains("ACTIVE PLAYBOOKS:"));
    }

    // -----------------------------------------------------------------------
    // Retrieval query blending
    // -----------------------------------------------------------------------
    //
    // These assert the property the recall gain rests on: the words that
    // discriminate the target schema are present in the embedded string. A
    // follow-up phrased as a pronoun or an ellipsis carries none of its own, so
    // the query is only usable if the earlier turn's words survive into it.

    #[test]
    fn retrieval_query_keeps_discriminating_words_for_pronoun_reference() {
        // "the Redwood one" names no type; "conference proposal" did.
        let prior = [
            "Add a conference proposal for Redwood Summit",
            "Added the Redwood Summit proposal.",
        ];
        let query = build_retrieval_query(&prior, "Set the Redwood one to rejected");

        assert!(query.contains("conference proposal"));
        assert!(query.contains("Set the Redwood one to rejected"));
    }

    #[test]
    fn retrieval_query_keeps_discriminating_words_for_ellipsis() {
        // "Which ones are still out?" elides its subject entirely.
        let prior = [
            "Track the invoices I send to clients",
            "Created the invoice type.",
        ];
        let query = build_retrieval_query(&prior, "Which ones are still out?");

        assert!(query.contains("invoices"));
        assert!(query.contains("Which ones are still out?"));
    }

    #[test]
    fn retrieval_query_preserves_topic_switch() {
        // A self-contained message after an unrelated exchange must still lead
        // with its own words — this is the no-regression case for blending.
        let prior = ["Add an invoice for $500", "Invoice recorded."];
        let query = build_retrieval_query(&prior, "Create a venue named The Fillmore");

        assert!(query.contains("Create a venue named The Fillmore"));
        assert!(query.ends_with("Create a venue named The Fillmore"));
    }

    #[test]
    fn retrieval_query_blends_turns_oldest_first() {
        let prior = ["first turn", "second turn"];
        let query = build_retrieval_query(&prior, "current message");
        assert_eq!(query, "first turn\nsecond turn\ncurrent message");
    }

    #[test]
    fn retrieval_query_uses_only_the_last_two_turns() {
        let prior = ["oldest turn", "middle turn", "newest turn"];
        let query = build_retrieval_query(&prior, "current message");

        assert!(!query.contains("oldest turn"));
        assert_eq!(query, "middle turn\nnewest turn\ncurrent message");
    }

    #[test]
    fn retrieval_query_without_history_is_the_message_alone() {
        // First turn of a conversation must be byte-identical to the old
        // behaviour, so single-turn retrieval is unaffected.
        assert_eq!(
            build_retrieval_query(&[], "Add an invoice"),
            "Add an invoice"
        );
    }

    #[test]
    fn retrieval_query_skips_blank_turns() {
        let prior = ["real turn", "   ", ""];
        let query = build_retrieval_query(&prior, "current message");
        assert_eq!(query, "real turn\ncurrent message");
    }

    #[test]
    fn retrieval_query_trims_and_tolerates_empty_current_message() {
        assert_eq!(
            build_retrieval_query(&["  prior turn  "], "  current  "),
            "prior turn\ncurrent"
        );
        assert_eq!(build_retrieval_query(&["prior turn"], "   "), "prior turn");
        assert_eq!(build_retrieval_query(&[], "   "), "");
    }
}
