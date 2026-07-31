//! Shared source of truth for core-type rules that must stay in sync between
//! [`crate::skill_pipeline::seed_skill_nodes`] (guidance for the in-app,
//! instance-aware local agent) and the generated sections of
//! `packages/skill/SKILL.md` (a static, instance-blind reference installed by
//! external PTY agents).
//!
//! Each rule here covers a constraint that is true regardless of which
//! surface acts on it — schema-authoring conventions, or interaction habits
//! like "search before acting on an ID". Instance-specific guidance (custom
//! schemas actually registered on a running daemon) and CLI-only reference
//! material (flags, `--database` selection, output shapes) do not belong
//! here — they have no analog on the other side and are not a drift risk.
//!
//! Two renderers read these rules: [`SchemaRule::imperative`] /
//! [`InteractionRule::imperative`] produce the terse, ALL-CAPS-header style
//! `seed_skill_nodes()` uses for LLM prompt content, and
//! [`SchemaRule::prose`] / [`InteractionRule::prose`] produce the
//! bold-lead-sentence markdown style `SKILL.md` uses. `bin/gen_skill_md.rs`
//! renders the prose form into `packages/skill/SKILL.md`; a checked-in copy
//! is verified against that output so the file cannot silently go stale.

/// A schema-authoring convention (field naming, enums, relationships,
/// title templates, request-scoping).
pub struct SchemaRule {
    pub id: &'static str,
    /// Terse imperative form for the LLM prompt (seed_skill_nodes()).
    pub imperative: &'static str,
    /// Flowing markdown prose form for SKILL.md, including its own
    /// **Bold lead.** phrase.
    pub prose: &'static str,
}

/// A generic interaction habit repeated across multiple skills/CLI verbs
/// (find-then-act, ask-one-clarifying-question, success-means-stop).
pub struct InteractionRule {
    pub id: &'static str,
    pub imperative: &'static str,
    pub prose: &'static str,
    /// A short, distinctive substring that must appear somewhere in
    /// `packages/skill/SKILL.md` for this rule to be considered present.
    /// Unlike [`SchemaRule`]'s `prose`, `InteractionRule::prose` is woven
    /// mid-sentence into hand-written CLI prose rather than generated
    /// verbatim, so presence is checked via this shorter, drift-resistant
    /// phrase instead of an exact match on `prose` itself.
    pub skill_md_key_phrase: &'static str,
}

pub const ONE_SCHEMA_PER_REQUEST: SchemaRule = SchemaRule {
    id: "one-schema-per-request",
    imperative: "ONE SCHEMA PER REQUEST: Create exactly the type the user named, in a single create_schema call, then stop and report it. Do NOT proactively invent or create related types (e.g. asked for \"Invoice\", do not also create \"Customer\" or \"Product\"), and do NOT follow up with update_schema to wire relationships unless the user explicitly asked for them. A relationship's targetType must already exist in ENTITY TYPES; if it doesn't, omit the relationship rather than creating the other type.",
    prose: "**One schema per request.** Create exactly the type asked for, in a single `schema create` call, then stop and report it. Don't proactively create related types the user didn't ask for (e.g. asked for \"Invoice\" — don't also create \"Customer\" or \"Product\"), and don't follow up with `schema update` to wire relationships unless explicitly asked. A relationship's `targetType` must already exist (check `nodespace schema list`); if it doesn't, omit the relationship rather than creating the other type as a side effect.",
};

pub const SCHEMA_ALREADY_EXISTS: SchemaRule = SchemaRule {
    id: "schema-already-exists",
    imperative: "SUCCESS: After create_schema returns a schema object (with fields, type_id, etc.), the schema was created. Respond to the user immediately — do NOT call create_schema again. If create_schema returns an error saying the schema already exists, stop and tell the user the type already exists and they can create instances with create_node.",
    prose: "If `create` reports the schema already exists, stop and tell the user — they can create instances with `node create` against the existing type.",
};

pub const EDIT_DONT_RECREATE: SchemaRule = SchemaRule {
    id: "edit-dont-recreate",
    imperative: "EDITING A SCHEMA — call update_schema: When the user wants to add a field, remove a field, rename a field, or change a relationship on an existing schema, call update_schema with the schema_id and only the fields that need changing. Do NOT re-create the whole schema. Use add_fields, remove_fields, rename_fields, or update the description/title_template as needed.",
    prose: "**Editing:** to add, remove, or rename a field, or change a relationship on an existing schema, use `schema update` with only the fields that need changing (`add_fields`/`remove_fields`/`rename_fields`, or an updated `description`/`title_template`). Don't re-create the whole schema for a small change.",
};

pub const NO_NAME_TITLE_FIELD: SchemaRule = SchemaRule {
    id: "no-name-title-field",
    imperative: "Do NOT add a 'name' or 'title' field — every node already has a built-in content/title field.",
    prose: "define only type-specific fields — don't add a `name` or `title` field; every node already has a built-in content/title field.",
};

pub const FIELDS_FROM_REQUEST_ONLY: SchemaRule = SchemaRule {
    id: "fields-from-request-only",
    imperative: "FIELD SOURCE: derive every field from what the user's OWN request describes wanting to track — never from another schema shown in RELEVANT ENTITY TYPES. That block lists types that already exist so you don't recreate them; it is not a shape to copy fields from for a new, different type. A new type about albums does not inherit fields from an unrelated equipment or invoice schema just because one is listed there.",
    prose: "**Field source:** derive every field from what the user's own request describes wanting to track — never from another schema shown in the entity-types context. That listing exists so you don't recreate a type that already exists; it is not a shape to copy fields from for a new, unrelated type.",
};

pub const NAME_PLACEHOLDER_EXCEPTION: SchemaRule = SchemaRule {
    id: "name-placeholder-exception",
    imperative: "EXCEPTION: if you use a 'name' placeholder in title_template (e.g. \"{name} ({status})\"), you MUST define 'name' as a text field so title generation works.",
    prose: "Exception: if `title_template` uses a `{name}` placeholder, `name` must be defined as a field (any placeholder in `title_template` must have a matching field).",
};

pub const ENUM_FORMAT: SchemaRule = SchemaRule {
    id: "enum-format",
    imperative: "ENUMS: Use lowercase values with readable labels, e.g. {\"value\": \"in_progress\", \"label\": \"In Progress\"}.",
    prose: "**Enums:** lowercase values with readable labels — `{\"value\":\"in_progress\",\"label\":\"In Progress\"}`.",
};

pub const RELATIONSHIP_VS_FIELD: SchemaRule = SchemaRule {
    id: "relationship-vs-field",
    imperative: "RELATIONSHIPS: Use relationships (not fields) when a field references another node type.",
    prose: "**Relationships vs. fields:** use a relationship (not a field) when a value references another node type.",
};

pub const TARGET_TYPE_MUST_EXIST: SchemaRule = SchemaRule {
    id: "target-type-must-exist",
    imperative: "The targetType MUST be an existing schema ID from the ENTITY TYPES list in the system prompt — do NOT invent types that aren't listed. If the target type doesn't exist yet, omit the relationship entirely. Examples:\n- Invoice billed_to customer (one): {\"name\": \"billed_to\", \"targetType\": \"customer\", \"direction\": \"out\", \"cardinality\": \"one\"}\n- Project has_task task (many): {\"name\": \"has_task\", \"targetType\": \"task\", \"direction\": \"out\", \"cardinality\": \"many\"}",
    prose: "`targetType` must be an existing schema ID. Examples: `{\"name\":\"billed_to\",\"targetType\":\"customer\",\"direction\":\"out\",\"cardinality\":\"one\"}`, `{\"name\":\"has_task\",\"targetType\":\"task\",\"direction\":\"out\",\"cardinality\":\"many\"}`.",
};

pub const TITLE_TEMPLATE_PLACEHOLDERS: SchemaRule = SchemaRule {
    id: "title-template-placeholders",
    imperative: "TITLE TEMPLATE: Set title_template when a node's identity comes from its fields rather than free-form content. Use {field_name} placeholders. CRITICAL: every placeholder in title_template MUST be defined as a field in the fields array. Omit title_template if the content/title field alone identifies the node.",
    prose: "**Title template:** set `title_template` when a node's identity comes from its fields rather than free-form content, using `{field_name}` placeholders — every placeholder must be a defined field. Omit it if the content/title field alone identifies the node.",
};

/// All schema-authoring rules, in the order they should be rendered.
pub const SCHEMA_RULES: &[SchemaRule] = &[
    ONE_SCHEMA_PER_REQUEST,
    SCHEMA_ALREADY_EXISTS,
    EDIT_DONT_RECREATE,
    NO_NAME_TITLE_FIELD,
    FIELDS_FROM_REQUEST_ONLY,
    NAME_PLACEHOLDER_EXCEPTION,
    ENUM_FORMAT,
    RELATIONSHIP_VS_FIELD,
    TARGET_TYPE_MUST_EXIST,
    TITLE_TEMPLATE_PLACEHOLDERS,
];

pub const FIND_THEN_ACT: InteractionRule = InteractionRule {
    id: "find-then-act",
    imperative: "If you don't have the node's ID, call search_semantic or search_nodes first to locate it. Then act on the resolved ID — do not guess IDs.",
    prose: "if you don't already have the target node's ID, search for it first, then act on the resolved ID.",
    skill_md_key_phrase: "if you don't have its ID",
};

pub const AMBIGUITY_CLARIFY: InteractionRule = InteractionRule {
    id: "ambiguity-clarify",
    imperative: "AMBIGUITY: If search returns 0 results or multiple results that don't clearly match what the user described, ask one specific clarifying question rather than retrying.",
    prose: "if the search comes back with zero matches or several equally plausible matches, ask the user one specific clarifying question rather than retrying.",
    skill_md_key_phrase: "ask the user one specific clarifying question rather than retrying",
};

pub const SUCCESS_NO_REVERIFY: InteractionRule = InteractionRule {
    id: "success-no-reverify",
    imperative: "SUCCESS: After the mutating call returns, confirm the change to the user. Do NOT re-fetch or re-search to confirm — the response itself is the confirmation.",
    prose: "confirm the change to the user from the response — don't re-fetch or re-search afterward just to double-check it landed.",
    skill_md_key_phrase: "don't re-fetch",
};

pub const TASK_STATUS_DEDICATED_VERB: InteractionRule = InteractionRule {
    id: "task-status-dedicated-verb",
    imperative: "TASK STATUS: To change a task's status (open, in_progress, done, cancelled), call update_task_status with the task ID and the new status string. Do NOT use update_node for task status changes.",
    prose: "task status changes must go through the dedicated status-update verb, not a generic property update.",
    skill_md_key_phrase: "for task status changes",
};

pub const SINGLE_ITEM_PER_CALL: InteractionRule = InteractionRule {
    id: "single-item-per-call",
    imperative: "SINGLE DELETE: Call delete_node once per node. Confirm each deletion before proceeding to the next.",
    prose: "act on one node per call; confirm each individually before moving to the next.",
    skill_md_key_phrase: "Delete one node per call; confirm each deletion before moving to the next",
};

pub const ORG_NEEDS_EXISTING_COLLECTION: InteractionRule = InteractionRule {
    id: "org-needs-existing-collection",
    imperative: "If the collection doesn't exist as a node yet, ask the user to create it first using the Node Creation skill.",
    prose: "if the target collection doesn't exist as a node yet, ask the user to create it first rather than creating it implicitly.",
    skill_md_key_phrase: "If the collection doesn't exist as a node yet, ask the user to create it first",
};

pub const BULK_IMPORT_NO_FOLLOWUP_SEARCH: InteractionRule = InteractionRule {
    id: "bulk-import-no-followup-search",
    imperative: "SUCCESS: After create_nodes_from_markdown returns, report the number of nodes created. Do NOT follow up with search calls.",
    prose: "report the number of nodes created; don't follow up with search calls to verify.",
    skill_md_key_phrase: "don't follow up with search calls to verify",
};

/// All generic interaction-pattern rules, in no particular required order —
/// each is consumed independently by whichever skill/CLI section needs it.
pub const INTERACTION_RULES: &[InteractionRule] = &[
    FIND_THEN_ACT,
    AMBIGUITY_CLARIFY,
    SUCCESS_NO_REVERIFY,
    TASK_STATUS_DEDICATED_VERB,
    SINGLE_ITEM_PER_CALL,
    ORG_NEEDS_EXISTING_COLLECTION,
    BULK_IMPORT_NO_FOLLOWUP_SEARCH,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_schema_rule_ids_are_unique() {
        let mut ids: Vec<&str> = SCHEMA_RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), SCHEMA_RULES.len(), "duplicate SchemaRule id");
    }

    #[test]
    fn all_interaction_rule_ids_are_unique() {
        let mut ids: Vec<&str> = INTERACTION_RULES.iter().map(|r| r.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(
            ids.len(),
            INTERACTION_RULES.len(),
            "duplicate InteractionRule id"
        );
    }

    #[test]
    fn no_rule_text_is_empty() {
        for r in SCHEMA_RULES {
            assert!(!r.imperative.is_empty(), "{} imperative is empty", r.id);
            assert!(!r.prose.is_empty(), "{} prose is empty", r.id);
        }
        for r in INTERACTION_RULES {
            assert!(!r.imperative.is_empty(), "{} imperative is empty", r.id);
            assert!(!r.prose.is_empty(), "{} prose is empty", r.id);
            assert!(
                !r.skill_md_key_phrase.is_empty(),
                "{} skill_md_key_phrase is empty",
                r.id
            );
        }
    }

    /// Interaction rules are woven mid-sentence into hand-written SKILL.md
    /// prose (unlike SchemaRule, which is regenerated verbatim by
    /// bin/gen_skill_md.rs — see that binary's own staleness test). This
    /// only catches outright removal of a rule's substance; it does not
    /// guarantee SKILL.md's wording matches `prose` exactly.
    #[test]
    fn skill_md_still_mentions_every_interaction_rule() {
        let skill_md_path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../skill/SKILL.md");
        let skill_md = std::fs::read_to_string(&skill_md_path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", skill_md_path.display()));

        for r in INTERACTION_RULES {
            assert!(
                skill_md.contains(r.skill_md_key_phrase),
                "packages/skill/SKILL.md no longer mentions the '{}' rule \
                 (expected to find the phrase {:?}) — if this rule's guidance \
                 moved or was reworded, update skill_md_key_phrase in \
                 skill_rules.rs to match; if the rule's substance was removed \
                 from SKILL.md, that's the drift this test exists to catch",
                r.id,
                r.skill_md_key_phrase
            );
        }
    }
}
