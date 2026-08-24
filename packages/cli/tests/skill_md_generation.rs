//! Drift and completeness guards for `packages/skill/SKILL.md`.
//!
//! The skill is what an external agent (Claude Code, Codex, Gemini CLI,
//! OpenCode) reads to learn what NodeSpace is and how to drive it. Anything it
//! restates by hand can fall out of step with the code, and nothing catches it
//! — the agent simply acts on stale instructions. These tests make each source
//! of truth structurally responsible for its own coverage.
//!
//! ## The contract runs one way
//!
//! Every *source* must have a rendered section. It is **never** asserted that
//! every section has a source. That asymmetry is deliberate and load-bearing:
//! judgment prose — what NodeSpace is, when to reach for it, how to install it
//! — has no upstream to render from and must survive regeneration untouched. A
//! test demanding the reverse would delete exactly the content a generator
//! cannot produce.
//!
//! ## Why enumeration is dynamic
//!
//! Coverage is computed by walking `Cli::command()` and `seed_skill_nodes()`
//! at test time, not by comparing against a hand-maintained list of names. A
//! checked-in list is itself a second representation that drifts, and it fails
//! in the worst direction — silently, by omission. This mirrors the existing
//! precedent in `agent_guidance.rs`'s `guidance_corpus()`.

use clap::{Command as ClapCommand, CommandFactory};
use nodespace_cli::Cli;
use std::path::PathBuf;

fn skill_md() -> String {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skill/SKILL.md");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()))
}

/// Subcommands a user can actually invoke — clap's generated `help` and
/// anything hidden are not part of the documented surface. Mirrors the
/// generator's filter.
fn visible<'a>(cmd: &'a ClapCommand) -> Vec<&'a ClapCommand> {
    cmd.get_subcommands()
        .filter(|c| !c.is_hide_set() && c.get_name() != "help")
        .collect()
}

/// Every command and subcommand reachable from the clap root appears in
/// SKILL.md.
///
/// This is the test that closes the drift documented on issue #1958, where
/// `session`, `uninstall`, and `model` had zero occurrences in a hand-written
/// CLI reference. Because the expected set is derived from the parser itself,
/// a command added later is covered with no change here.
#[test]
fn every_cli_command_is_documented() {
    let skill = skill_md();
    let cli = Cli::command();
    let mut missing = Vec::new();

    for sub in visible(&cli) {
        let leaves = visible(sub);
        if leaves.is_empty() {
            let needle = format!("nodespace {}", sub.get_name());
            if !skill.contains(&needle) {
                missing.push(needle);
            }
            continue;
        }
        for leaf in leaves {
            let needle = format!("nodespace {} {}", sub.get_name(), leaf.get_name());
            if !skill.contains(&needle) {
                missing.push(needle);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "SKILL.md does not document these CLI commands: {missing:#?}\n\
         Run `bun run skill:gen` and commit the result."
    );
}

/// Every non-global, non-hidden flag on every leaf command appears in
/// SKILL.md.
///
/// Commands were the coarse drift; flags were the fine drift — issue #1958
/// recorded the whole `import dir` flag set (`--exclude`,
/// `--include-agent-files`, `--include-hidden`, `--no-recursive`, `--replace`,
/// `--collection`, `--use-filename-as-title`) and `node query --id` as
/// undocumented while their parent commands were present. A flag an agent
/// cannot see is a capability it will not use.
#[test]
fn every_cli_flag_is_documented() {
    let skill = skill_md();
    let cli = Cli::command();
    let mut missing = Vec::new();

    fn check(cmd: &ClapCommand, path: &str, skill: &str, missing: &mut Vec<String>) {
        for arg in cmd.get_arguments() {
            if arg.is_hide_set() || arg.is_global_set() {
                continue;
            }
            if matches!(arg.get_id().as_str(), "help" | "version") {
                continue;
            }
            let Some(long) = arg.get_long() else {
                continue; // positionals are covered by the command test
            };
            let needle = format!("--{long}");
            if !skill.contains(&needle) {
                missing.push(format!("{path} {needle}"));
            }
        }
    }

    for sub in visible(&cli) {
        let leaves = visible(sub);
        if leaves.is_empty() {
            check(sub, sub.get_name(), &skill, &mut missing);
            continue;
        }
        for leaf in leaves {
            let path = format!("{} {}", sub.get_name(), leaf.get_name());
            check(leaf, &path, &skill, &mut missing);
        }
    }

    assert!(
        missing.is_empty(),
        "SKILL.md does not document these CLI flags: {missing:#?}\n\
         Run `bun run skill:gen` and commit the result."
    );
}

/// Every global flag is documented once.
#[test]
fn every_global_cli_flag_is_documented() {
    let skill = skill_md();
    let cli = Cli::command();
    let mut missing = Vec::new();

    for arg in cli.get_arguments() {
        if arg.is_hide_set() || !arg.is_global_set() {
            continue;
        }
        if let Some(long) = arg.get_long() {
            let needle = format!("--{long}");
            if !skill.contains(&needle) {
                missing.push(needle);
            }
        }
    }

    assert!(
        missing.is_empty(),
        "SKILL.md does not document these global flags: {missing:#?}"
    );
}

/// Every seeded skill is represented in SKILL.md.
///
/// Issue #1958 recorded that `seed_skill_nodes()` defines **eight** skills, not
/// the six built from named `*_guidance()` functions: **Research & Search** and
/// **Node Creation** are inline raw strings with no builder function. Any check
/// keyed on function names would silently skip both — and Research & Search is
/// the largest body of the set, and the un-generated source of SKILL.md's own
/// Tool Decision Guide.
///
/// Enumerating `seed_skill_nodes()` directly is what makes that impossible: a
/// skill defined any way at all is a `NodeTemplate` in the returned vector.
#[test]
fn every_seeded_skill_is_represented() {
    let skill = skill_md().to_lowercase();
    let mut missing = Vec::new();

    for template in nodespace_agent::skill_pipeline::seed_skill_nodes() {
        // Match on the skill's distinguishing noun rather than its exact
        // title: SKILL.md is organized by task ("Create a node", "Semantic
        // search"), not by the internal skill names, and requiring the literal
        // titles would force the file into the agent's vocabulary instead of
        // the user's.
        let covered = match template.title.as_str() {
            "Research & Search" => skill.contains("search"),
            "Node Creation" => skill.contains("node create"),
            "Schema Creation" => skill.contains("schema create"),
            "Graph Editing" => skill.contains("node update"),
            "Relationship Management" => skill.contains("relationship create"),
            "Node Deletion" => skill.contains("node delete"),
            "Bulk Import" => skill.contains("import"),
            "Organization" => skill.contains("collection"),
            // A skill added later has no arm here and fails loudly, which is
            // the intent: someone must decide how it surfaces to an external
            // agent rather than have it silently omitted.
            other => {
                missing.push(format!("{other} (no coverage rule defined)"));
                true
            }
        };
        if !covered {
            missing.push(template.title.clone());
        }
    }

    assert!(
        missing.is_empty(),
        "these seeded skills have no counterpart in SKILL.md: {missing:#?}\n\
         Every skill the local agent is taught should be reachable by an \
         external agent too, or explicitly justified as internal-only."
    );
}

/// The checked-in SKILL.md matches what the generator produces.
///
/// This is the same guarantee `bun run skill:check` gives in the pre-push
/// gate, asserted here too so `cargo test` alone catches drift — a contributor
/// editing `skill_rules.rs` or a clap doc comment sees the failure without
/// needing to remember a separate command.
#[test]
fn checked_in_skill_md_is_up_to_date() {
    let status = std::process::Command::new(env!("CARGO"))
        .args([
            "run",
            "-q",
            "-p",
            "nodespace-cli",
            "--example",
            "gen_skill_md",
            "--",
            "--check",
        ])
        .current_dir(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))
        .status()
        .expect("failed to run the gen_skill_md example");

    assert!(
        status.success(),
        "packages/skill/SKILL.md is stale — run `bun run skill:gen` and commit the result"
    );
}
