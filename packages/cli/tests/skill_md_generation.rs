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

/// Every markdown file the skill ships, concatenated.
///
/// Coverage is asserted against the whole skill folder, not `SKILL.md` alone.
/// The body is kept within the standard's size recommendation by moving the CLI
/// reference into `references/`, which the spec defines as the on-demand tier
/// and which is portable across every target. Content that moves between the
/// two tiers is still shipped and still reachable, so a check that only read
/// `SKILL.md` would report false drift the moment anything was moved.
fn skill_md() -> String {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skill");
    let mut combined = String::new();

    let read = |p: &PathBuf| -> String {
        std::fs::read_to_string(p).unwrap_or_else(|e| panic!("failed to read {}: {e}", p.display()))
    };

    combined.push_str(&read(&dir.join("SKILL.md")));

    let refs = dir.join("references");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&refs)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", refs.display()))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "md"))
        .collect();
    // Sorted so the concatenation is deterministic regardless of directory order.
    entries.sort();
    assert!(
        !entries.is_empty(),
        "no reference files found in {} — the CLI reference is expected to live there",
        refs.display()
    );
    for path in entries {
        combined.push('\n');
        combined.push_str(&read(&path));
    }

    combined
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

/// Every built-in structural relationship name is documented.
///
/// The skill used to say a relationship name must be defined on the source
/// node's schema, full stop — stricter than the system is. Four names are legal
/// between any two nodes without a declaration, and the local agent's own
/// guidance says so, so the two surfaces disagreed. Enumerating the constant
/// the validator checks against means a name added to it cannot be omitted.
#[test]
fn every_builtin_relationship_name_is_documented() {
    let skill = skill_md();
    let missing: Vec<&str> = nodespace_core::models::schema::BUILTIN_RELATIONSHIP_NAMES
        .iter()
        .copied()
        .filter(|name| !skill.contains(name))
        .collect();

    assert!(
        missing.is_empty(),
        "the shipped skill does not mention these built-in relationship names: \
         {missing:#?}\nRun `bun run skill:gen` and commit the result."
    );
}

/// The SKILL.md body stays within the Agent Skills size recommendations.
///
/// The body is loaded in full the moment the skill activates, so its size is a
/// per-activation cost paid by every agent on every matching task — which is
/// what the standard's guidance (≤500 lines, <5000 tokens) is about. Detail
/// belongs in `references/`, loaded only when actually needed.
///
/// This guard exists because the file has already crossed the line once: it
/// was 534 lines before the CLI surface was generated into it, and generating
/// the previously-missing commands pushed it to 786. Without a test, the next
/// addition repeats that quietly.
#[test]
fn skill_md_body_is_within_spec_size_recommendations() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skill/SKILL.md");
    let body = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));

    const MAX_LINES: usize = 500;
    let lines = body.lines().count();
    assert!(
        lines <= MAX_LINES,
        "SKILL.md body is {lines} lines, over the {MAX_LINES}-line recommendation. \
         Move detail into packages/skill/references/ rather than growing the body."
    );

    // ~4 chars/token is the usual English approximation; the spec's limit is
    // advisory, so an approximation is the right instrument. Being wrong by a
    // few percent does not change whether a 500-line file is acceptable.
    const MAX_TOKENS: usize = 5000;
    let approx_tokens = body.chars().count() / 4;
    assert!(
        approx_tokens < MAX_TOKENS,
        "SKILL.md body is ~{approx_tokens} tokens, over the {MAX_TOKENS}-token recommendation. \
         Move detail into packages/skill/references/ rather than growing the body."
    );
}

/// Reference files are reachable from the body.
///
/// A reference nothing points at is a file an agent never opens. The standard's
/// progressive disclosure only works if the body names what to load.
#[test]
fn every_reference_file_is_linked_from_the_body() {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../skill");
    let body = std::fs::read_to_string(dir.join("SKILL.md")).expect("failed to read SKILL.md");

    let mut unlinked = Vec::new();
    for entry in std::fs::read_dir(dir.join("references")).expect("failed to read references/") {
        let path = entry.expect("bad dir entry").path();
        if path.extension().is_some_and(|x| x == "md") {
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            if !body.contains(&format!("references/{name}")) {
                unlinked.push(name);
            }
        }
    }

    assert!(
        unlinked.is_empty(),
        "these reference files are never mentioned in SKILL.md, so an agent will \
         not know to read them: {unlinked:#?}"
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
