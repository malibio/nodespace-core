//! Regenerates the schema-rules block of `packages/skill/SKILL.md` from the
//! shared source in [`nodespace_agent::skill_rules`], so that block cannot
//! silently drift from the equivalent rules rendered into
//! `seed_skill_nodes()` (`packages/agent/src/skill_pipeline.rs`).
//!
//! Only the block between the `<!-- BEGIN GENERATED: schema-rules -->` /
//! `<!-- END GENERATED: schema-rules -->` markers is generated — the rest of
//! SKILL.md (CLI reference, flags, examples, database management) has no
//! analog in the seed skill nodes and stays hand-written.
//!
//! Usage:
//!   cargo run -p nodespace-agent --bin gen_skill_md -- --check   # CI/pre-push: exit 1 if stale
//!   cargo run -p nodespace-agent --bin gen_skill_md -- --write   # regenerate and overwrite SKILL.md

use nodespace_agent::skill_rules::{
    EDIT_DONT_RECREATE, ENUM_FORMAT, FIELDS_FROM_REQUEST_ONLY, NAME_PLACEHOLDER_EXCEPTION,
    NO_NAME_TITLE_FIELD, ONE_SCHEMA_PER_REQUEST, RELATIONSHIP_VS_FIELD, RENAME_VS_RELABEL,
    SCHEMA_ALREADY_EXISTS, SCHEMA_VALIDATION_ERROR_RETRY, TARGET_TYPE_MUST_EXIST,
    TITLE_TEMPLATE_PLACEHOLDERS, UNIQUE_FIELD_FLAGS,
};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

const BEGIN_MARKER: &str =
    "<!-- BEGIN GENERATED: schema-rules (see packages/agent/src/skill_rules.rs, packages/agent/src/bin/gen_skill_md.rs) -->";
const END_MARKER: &str = "<!-- END GENERATED: schema-rules -->";

/// Renders the schema-rules block content (the text between the markers,
/// exclusive), joining rules that share a single SKILL.md paragraph.
fn render_schema_rules_block() -> String {
    // ONE_SCHEMA_PER_REQUEST, SCHEMA_ALREADY_EXISTS, and
    // SCHEMA_VALIDATION_ERROR_RETRY render as three paragraphs (SKILL.md
    // separates them with a blank line), everything else is one rule per
    // paragraph. NO_NAME_TITLE_FIELD and NAME_PLACEHOLDER_EXCEPTION share the
    // "**Schema fields:**" paragraph.
    format!(
        "{one_schema_per_request}\n\n{schema_already_exists}\n\n{schema_validation_error_retry}\n\n\
         {edit_dont_recreate}\n\n\
         {rename_vs_relabel}\n\n\
         **Schema fields:** {no_name_title_field} {name_placeholder_exception}\n\n\
         {fields_from_request_only}\n\n\
         {enum_format}\n\n\
         {relationship_vs_field} {target_type_must_exist}\n\n\
         {title_template_placeholders}\n\n\
         {unique_field_flags}",
        one_schema_per_request = ONE_SCHEMA_PER_REQUEST.prose,
        schema_already_exists = SCHEMA_ALREADY_EXISTS.prose,
        schema_validation_error_retry = SCHEMA_VALIDATION_ERROR_RETRY.prose,
        edit_dont_recreate = EDIT_DONT_RECREATE.prose,
        rename_vs_relabel = RENAME_VS_RELABEL.prose,
        no_name_title_field = NO_NAME_TITLE_FIELD.prose,
        name_placeholder_exception = NAME_PLACEHOLDER_EXCEPTION.prose,
        fields_from_request_only = FIELDS_FROM_REQUEST_ONLY.prose,
        enum_format = ENUM_FORMAT.prose,
        relationship_vs_field = RELATIONSHIP_VS_FIELD.prose,
        target_type_must_exist = TARGET_TYPE_MUST_EXIST.prose,
        title_template_placeholders = TITLE_TEMPLATE_PLACEHOLDERS.prose,
        unique_field_flags = UNIQUE_FIELD_FLAGS.prose,
    )
}

/// Splices `block` between the markers in `source`, replacing whatever was
/// there before. Panics if the markers aren't found, are duplicated, or are
/// malformed — that indicates SKILL.md itself was edited in a way that broke
/// the generation contract and needs a human to look at it, not a silent
/// no-op or a splice into the wrong occurrence.
fn splice_generated_block(source: &str, block: &str) -> String {
    let begin_idx = source
        .find(BEGIN_MARKER)
        .unwrap_or_else(|| panic!("SKILL.md is missing the marker: {BEGIN_MARKER}"));
    assert!(
        !source[begin_idx + BEGIN_MARKER.len()..].contains(BEGIN_MARKER),
        "SKILL.md has more than one occurrence of the begin marker: {BEGIN_MARKER}"
    );
    let after_begin = begin_idx + BEGIN_MARKER.len();
    let end_idx = source[after_begin..]
        .find(END_MARKER)
        .unwrap_or_else(|| panic!("SKILL.md is missing the marker: {END_MARKER}"))
        + after_begin;
    assert!(
        !source[end_idx + END_MARKER.len()..].contains(END_MARKER),
        "SKILL.md has more than one occurrence of the end marker: {END_MARKER}"
    );

    format!(
        "{prefix}{begin}\n{block}\n{end}{suffix}",
        prefix = &source[..begin_idx],
        begin = BEGIN_MARKER,
        end = END_MARKER,
        suffix = &source[end_idx + END_MARKER.len()..],
    )
}

fn skill_md_path() -> PathBuf {
    // CARGO_MANIFEST_DIR is packages/agent — SKILL.md lives in the sibling
    // packages/skill package.
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skill/SKILL.md")
}

fn main() -> ExitCode {
    let mode = env::args().nth(1);
    let path = skill_md_path();
    let current = fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
    let block = render_schema_rules_block();
    let regenerated = splice_generated_block(&current, &block);

    match mode.as_deref() {
        Some("--write") => {
            fs::write(&path, &regenerated)
                .unwrap_or_else(|e| panic!("failed to write {}: {e}", path.display()));
            println!("Regenerated {}", path.display());
            ExitCode::SUCCESS
        }
        Some("--check") => {
            if current == regenerated {
                println!("{} is up to date.", path.display());
                ExitCode::SUCCESS
            } else {
                eprintln!(
                    "{} is stale — its generated schema-rules block no longer matches \
                     packages/agent/src/skill_rules.rs. Run `cargo run -p nodespace-agent \
                     --bin gen_skill_md -- --write` and commit the result.",
                    path.display()
                );
                ExitCode::FAILURE
            }
        }
        _ => {
            eprintln!("Usage: gen_skill_md --check | --write");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splice_replaces_only_the_marked_region() {
        let source = format!("before\n\n{BEGIN_MARKER}\nold content\n{END_MARKER}\n\nafter");
        let result = splice_generated_block(&source, "new content");
        assert_eq!(
            result,
            format!("before\n\n{BEGIN_MARKER}\nnew content\n{END_MARKER}\n\nafter")
        );
    }

    #[test]
    #[should_panic(expected = "missing the marker")]
    fn splice_panics_without_begin_marker() {
        splice_generated_block("no markers here", "block");
    }

    #[test]
    fn checked_in_skill_md_generated_block_is_up_to_date() {
        let path = skill_md_path();
        let current = fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let block = render_schema_rules_block();
        let regenerated = splice_generated_block(&current, &block);
        assert_eq!(
            current, regenerated,
            "packages/skill/SKILL.md's generated schema-rules block is stale — run \
             `cargo run -p nodespace-agent --bin gen_skill_md -- --write` and commit the result"
        );
    }
}
