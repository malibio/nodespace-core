//! The single renderer for the `RELEVANT ENTITY TYPES` prompt block.
//!
//! The block is emitted into the agent's system prompt from two places — the
//! workspace-context path ([`super::context_ops`]) and the skill-routing path
//! (`nodespace-agent`'s `local_agent::routing`) — which historically held two
//! independent renderers reading two different sources. They drifted: the
//! node-creation guidance conditioned `properties` population on field
//! metadata that only one of them emitted, so the model saw an instruction
//! with no referent and silently dropped every user-supplied value.
//!
//! Both paths now convert into [`EntityTypeDescriptor`] and render through
//! [`render_entity_types`]. The descriptor is the choke point: a field added
//! to `SchemaField` reaches the prompt only by passing through it, so it
//! cannot land in one path and not the other.
//!
//! The routing path additionally needs the descriptor to survive a JSON
//! round-trip, because it arrives as the `schema_metadata` body of the
//! model-facing `search_skills` tool response rather than as a typed value.
//! [`EntityTypeDescriptor::to_json`] and [`EntityTypeDescriptor::from_json`]
//! own that encoding so it is one reversible mapping rather than a
//! hand-written projection on each side.

use crate::models::SchemaNode;
use serde_json::{json, Value};

/// One entity type as the prompt block renders it.
///
/// Deliberately smaller than [`SchemaNode`]: it carries exactly what the block
/// shows, so "what the model is told about a type" is a single reviewable
/// struct rather than a property of whichever renderer ran.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityTypeDescriptor {
    /// The type id the model must pass as `node_type`.
    pub type_id: String,
    /// Human-readable name. Rendered only when it adds information beyond the
    /// id — the routing path frequently has nothing else.
    pub name: Option<String>,
    pub fields: Vec<EntityFieldDescriptor>,
    /// Rendered because the `create_node` tool description promises the
    /// template is "shown in ENTITY TYPES"; that promise needs a referent.
    pub title_template: Option<String>,
}

/// One field of an entity type as the prompt block renders it.
#[derive(Debug, Clone, PartialEq)]
pub struct EntityFieldDescriptor {
    pub name: String,
    pub field_type: String,
    /// Legal values for an enum field. A bare `status: enum` tells the model a
    /// value is wanted but not which ones the schema accepts, so it invents one
    /// and the write is rejected.
    pub enum_values: Vec<String>,
    /// The node-creation guidance treats a required field as mandatory in the
    /// `properties` map, so the flag must reach the prompt to be actionable.
    pub required: bool,
}

impl EntityFieldDescriptor {
    /// Render as `name: type`, with enum values and required-ness appended.
    ///
    /// This is the notation both call sites emit under a shared heading, so
    /// two spellings of one concept would read to the model as two different
    /// things.
    pub fn render(&self) -> String {
        let mut descriptor = if self.enum_values.is_empty() {
            format!("{}: {}", self.name, self.field_type)
        } else {
            format!(
                "{}: {} ({})",
                self.name,
                self.field_type,
                self.enum_values.join(", ")
            )
        };
        if self.required {
            descriptor.push_str(", required");
        }
        descriptor
    }

    fn from_schema_field(f: &crate::models::schema::SchemaField) -> Self {
        // Core and user values both carry legal enum members; the model needs
        // the union, since either kind is accepted on a write.
        let enum_values: Vec<String> = f
            .core_values
            .iter()
            .chain(f.user_values.iter())
            .flatten()
            .map(|v| v.value.clone())
            .collect();
        Self {
            name: f.name.clone(),
            field_type: f.field_type.clone(),
            enum_values,
            required: f.required.unwrap_or(false),
        }
    }
}

impl EntityTypeDescriptor {
    /// Build from a [`SchemaNode`] — the lossless conversion, used wherever the
    /// typed value is in hand.
    pub fn from_schema(schema: &SchemaNode) -> Self {
        Self {
            type_id: schema.id.clone(),
            name: Some(schema.content.clone()),
            fields: schema
                .fields
                .iter()
                .map(EntityFieldDescriptor::from_schema_field)
                .collect(),
            title_template: schema.title_template.clone(),
        }
    }

    /// Encode for the `search_skills` tool response.
    ///
    /// This shape is model-facing and pinned by the routing latency suites, so
    /// it is a contract rather than an internal detail. Keeping the encode and
    /// decode adjacent is what stops it from becoming a second renderer.
    ///
    /// One intentional difference from the projection this replaced: that code
    /// emitted `enum_values` only when `field_type == "enum"`, whereas this
    /// emits them whenever a field carries values. The two agree on every
    /// seeded core schema (none has a non-enum field with values), but they can
    /// differ at runtime: validation only enforces enum ⇒ has-values, never the
    /// converse, so a `create_schema` call may legitimately land `coreValues` on
    /// a `text` field. Emitting the values there is the better behaviour — the
    /// model can only use legal values it is shown — but it is a widening of
    /// this response, not a no-op.
    pub fn to_json(&self) -> Value {
        let fields: Vec<Value> = self
            .fields
            .iter()
            .map(|f| {
                let mut field = json!({
                    "name": f.name,
                    "type": f.field_type,
                });
                if f.required {
                    field["required"] = json!(true);
                }
                if !f.enum_values.is_empty() {
                    field["enum_values"] = json!(f.enum_values);
                }
                field
            })
            .collect();

        let mut entry = json!({
            "type_id": self.type_id,
            "fields": fields,
        });
        if let Some(name) = &self.name {
            entry["name"] = json!(name);
        }
        if let Some(tmpl) = &self.title_template {
            entry["title_template"] = json!(tmpl);
        }
        entry
    }

    /// Decode one entry of a `schema_metadata` array.
    ///
    /// Returns `None` for an entry with no `type_id`: without it the model has
    /// nothing to pass as `node_type`, so the line would describe a type it
    /// cannot name.
    pub fn from_json(entry: &Value) -> Option<Self> {
        let type_id = entry.get("type_id").and_then(|v| v.as_str())?;
        let fields = entry
            .get("fields")
            .and_then(|f| f.as_array())
            .map(|fs| {
                fs.iter()
                    .filter_map(|f| {
                        let name = f.get("name").and_then(|v| v.as_str())?;
                        // A field whose type was dropped in transit is still
                        // worth naming; `text` is the schema system's own
                        // default for an unspecified type.
                        let field_type = f
                            .get("type")
                            .and_then(|v| v.as_str())
                            .unwrap_or("text")
                            .to_string();
                        let enum_values = f
                            .get("enum_values")
                            .and_then(|v| v.as_array())
                            .map(|vals| {
                                vals.iter()
                                    .filter_map(|v| v.as_str().map(str::to_string))
                                    .collect()
                            })
                            .unwrap_or_default();
                        Some(EntityFieldDescriptor {
                            name: name.to_string(),
                            field_type,
                            enum_values,
                            required: f.get("required").and_then(|v| v.as_bool()).unwrap_or(false),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Some(Self {
            type_id: type_id.to_string(),
            name: entry
                .get("name")
                .and_then(|v| v.as_str())
                .map(str::to_string),
            fields,
            title_template: entry
                .get("title_template")
                .and_then(|v| v.as_str())
                .map(str::to_string),
        })
    }

    /// Render this type as one line of the block.
    ///
    /// A type with no fields renders as a bare name: the trailing `": "` of the
    /// populated form would read as a promise of a field list that never
    /// arrives.
    pub fn render_line(&self) -> String {
        let head = match &self.name {
            // The workspace-context path has both id and display name and shows
            // both, since the id is what `node_type` takes while the name is
            // what the user's phrasing will resemble.
            Some(name) => format!("- {}: {}", self.type_id, name),
            None => format!("- {}", self.type_id),
        };

        let mut line = if self.fields.is_empty() {
            head
        } else {
            let rendered: Vec<String> = self.fields.iter().map(|f| f.render()).collect();
            match &self.name {
                Some(_) => format!("{head} ({})", rendered.join("; ")),
                None => format!("{head}: {}", rendered.join("; ")),
            }
        };

        if let Some(tmpl) = &self.title_template {
            line.push_str(&format!(" [title_template: {tmpl}]"));
        }
        line
    }
}

/// Render a set of entity types as the body of the block, one line per type.
///
/// Returns `None` when there is nothing to show, so callers can omit the
/// heading rather than emit one over an empty list.
pub fn render_entity_types(types: &[EntityTypeDescriptor]) -> Option<String> {
    if types.is_empty() {
        return None;
    }
    let lines: Vec<String> = types.iter().map(|t| t.render_line()).collect();
    Some(lines.join("\n"))
}

/// Decode a `schema_metadata` array into descriptors, skipping malformed
/// entries.
///
/// Skipping rather than failing is deliberate: propagating out of the loop
/// would discard every remaining type because one lacked an id.
pub fn descriptors_from_json(meta: &Value) -> Vec<EntityTypeDescriptor> {
    meta.as_array()
        .map(|arr| {
            arr.iter()
                .filter_map(EntityTypeDescriptor::from_json)
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::schema::{EnumValue, SchemaField, SchemaProtectionLevel};

    fn field(name: &str, ty: &str) -> SchemaField {
        SchemaField {
            name: name.to_string(),
            field_type: ty.to_string(),
            protection: SchemaProtectionLevel::User,
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
            unique: None,
            unique_case_insensitive: None,
            local_only: false,
        }
    }

    fn enum_value(value: &str) -> EnumValue {
        EnumValue {
            value: value.to_string(),
            label: value.to_string(),
        }
    }

    fn sample_schema() -> SchemaNode {
        let mut amount = field("amount", "number");
        amount.required = Some(true);

        let mut status = field("status", "enum");
        status.core_values = Some(vec![enum_value("draft")]);
        status.user_values = Some(vec![enum_value("sent")]);

        SchemaNode {
            id: "invoice".to_string(),
            content: "Invoice".to_string(),
            version: 1,
            created_at: chrono::Utc::now(),
            modified_at: chrono::Utc::now(),
            is_core: false,
            schema_version: 1,
            fields: vec![field("reference", "string"), amount, status],
            relationships: vec![],
            title_template: Some("{reference}".to_string()),
            properties_header_summary_template: None,
        }
    }

    #[test]
    fn renders_type_field_enum_required_and_template() {
        let d = EntityTypeDescriptor::from_schema(&sample_schema());
        let line = d.render_line();

        assert!(line.contains("- invoice: Invoice"));
        assert!(line.contains("reference: string"));
        assert!(line.contains("amount: number, required"));
        // Core and user enum values are unioned; both are legal on a write.
        assert!(line.contains("status: enum (draft, sent)"));
        assert!(line.contains("[title_template: {reference}]"));
    }

    /// The property this whole module exists to enforce: the typed path and the
    /// JSON path must describe a schema identically. A field added to
    /// `SchemaField` that reaches only one of them fails here.
    #[test]
    fn json_round_trip_preserves_every_rendered_field() {
        let typed = EntityTypeDescriptor::from_schema(&sample_schema());
        let decoded = EntityTypeDescriptor::from_json(&typed.to_json())
            .expect("descriptor with a type_id must decode");

        assert_eq!(typed, decoded, "JSON round-trip must not lose information");
        assert_eq!(typed.render_line(), decoded.render_line());
    }

    #[test]
    fn fieldless_type_renders_without_a_dangling_separator() {
        let d = EntityTypeDescriptor {
            type_id: "venue".to_string(),
            name: None,
            fields: vec![],
            title_template: None,
        };
        assert_eq!(d.render_line(), "- venue");
    }

    #[test]
    fn malformed_entry_is_skipped_without_dropping_the_rest() {
        let meta = json!([
            { "fields": [] },
            { "type_id": "venue", "fields": [{ "name": "capacity", "type": "number" }] },
        ]);
        let descriptors = descriptors_from_json(&meta);

        assert_eq!(descriptors.len(), 1, "the entry lacking type_id is skipped");
        assert_eq!(descriptors[0].type_id, "venue");
    }

    #[test]
    fn empty_type_list_renders_nothing() {
        assert!(render_entity_types(&[]).is_none());
    }

    /// The block that reaches the model must be the same whether it came from
    /// the workspace-context path (typed `SchemaNode`) or the skill-routing
    /// path (`schema_metadata` JSON). Both are concatenated into one system
    /// prompt under one heading; describing a type two ways there is what
    /// caused the model to follow guidance with no referent.
    ///
    /// The JSON side is written out by hand rather than produced by
    /// [`EntityTypeDescriptor::to_json`]. Deriving it from `to_json` would make
    /// both sides originate from the same `from_schema` call, which asserts
    /// round-trip fidelity (already covered by
    /// `json_round_trip_preserves_every_rendered_field`) rather than agreement
    /// between the paths. Spelling the wire form out independently is what lets
    /// this fail when an encoder change stops matching what the decoder reads.
    ///
    /// Note the structural guarantee is the stronger one and does not depend on
    /// this test: a single `render_line` means a new `SchemaField` reaches both
    /// paths or neither. This pins the conversion seams on either side of it.
    #[test]
    fn both_prompt_paths_render_a_schema_identically() {
        let via_schema = EntityTypeDescriptor::from_schema(&sample_schema());

        // The `schema_metadata` wire form for the same schema, spelled out as
        // `skill_ops` emits it.
        let via_json = descriptors_from_json(&json!([{
            "type_id": "invoice",
            "name": "Invoice",
            "fields": [
                {"name": "reference", "type": "string"},
                {"name": "amount", "type": "number", "required": true},
                {"name": "status", "type": "enum", "enum_values": ["draft", "sent"]}
            ],
            "title_template": "{reference}"
        }]));

        assert_eq!(
            render_entity_types(std::slice::from_ref(&via_schema)),
            render_entity_types(&via_json),
            "the two prompt paths must describe a schema identically"
        );
    }

    /// Guards the specific regression that motivated the shared renderer:
    /// required-ness and enum values must reach the prompt on *both* paths,
    /// since the node-creation guidance keys `properties` population on them.
    #[test]
    fn required_and_enum_values_survive_the_routing_path() {
        let via_json =
            descriptors_from_json(&json!([
                EntityTypeDescriptor::from_schema(&sample_schema()).to_json()
            ]));
        let rendered = render_entity_types(&via_json).expect("one type renders");

        assert!(rendered.contains("amount: number, required"));
        assert!(rendered.contains("status: enum (draft, sent)"));
        assert!(rendered.contains("[title_template: {reference}]"));
    }
}
