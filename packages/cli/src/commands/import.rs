//! `nodespace import` — import markdown files into NodeSpace via the daemon.

use anyhow::Result;
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{
    FileImportResult, ImportMarkdownFilesRequest, ImportMarkdownRequest, ImportOptions,
};
use tokio_stream::StreamExt;
use tonic::Request;

use crate::ImportClient;

#[derive(Subcommand, Debug)]
pub enum ImportAction {
    /// Import a single markdown file.
    File(ImportFileArgs),
    /// Import all markdown files from a directory (recurses into sub-folders by
    /// default; see --no-recursive).
    Dir(ImportDirArgs),
}

#[derive(Args, Debug)]
pub struct ImportFileArgs {
    /// Path to the markdown file.
    pub file: String,

    /// Collection path to assign the document to (e.g. "docs:rust").
    #[arg(long)]
    pub collection: Option<String>,

    /// Use the filename stem as the document title.
    #[arg(long)]
    pub use_filename_as_title: bool,

    /// Route files to collections based on directory structure.
    #[arg(long)]
    pub auto_collection_routing: bool,

    /// Refresh an already-imported document in place: replace its child subtree
    /// from the fresh parse, keeping the root node so inbound links survive.
    /// Without this, an already-imported document is left untouched.
    #[arg(long)]
    pub replace: bool,
}

#[derive(Args, Debug)]
pub struct ImportDirArgs {
    /// Path to the directory containing markdown files.
    pub directory: String,

    /// Collection path to assign all documents to.
    #[arg(long)]
    pub collection: Option<String>,

    /// Use filename stems as document titles.
    #[arg(long)]
    pub use_filename_as_title: bool,

    /// Route files to collections based on directory structure.
    #[arg(long)]
    pub auto_collection_routing: bool,

    /// Directory names to exclude (repeatable, e.g. --exclude node_modules).
    #[arg(long = "exclude")]
    pub exclude_patterns: Vec<String>,

    /// Include CLAUDE.md / AGENTS.md files (default: excluded). Matched by
    /// basename, case-insensitive, at any depth.
    #[arg(long)]
    pub include_agent_files: bool,

    /// Include hidden files and folders — any path component starting with '.',
    /// e.g. .git/, .claude/, dotfiles (default: skipped).
    #[arg(long)]
    pub include_hidden: bool,

    /// Import only the top-level directory; do not descend into sub-folders
    /// (default: recurses into sub-folders).
    #[arg(long)]
    pub no_recursive: bool,

    /// Refresh already-imported documents in place: replace each existing
    /// document's child subtree from the fresh parse, keeping its root node so
    /// inbound links survive. Without this, already-imported documents are
    /// skipped (a plain re-import never duplicates).
    #[arg(long)]
    pub replace: bool,
}

pub async fn run(client: &mut ImportClient, action: ImportAction, json: bool) -> Result<()> {
    match action {
        ImportAction::File(args) => run_file(client, args, json).await,
        ImportAction::Dir(args) => run_dir(client, args, json).await,
    }
}

fn results_to_json(results: &[FileImportResult]) -> serde_json::Value {
    serde_json::Value::Array(
        results
            .iter()
            .map(|r| {
                serde_json::json!({
                    "file_path": r.file_path,
                    "root_id": r.root_id,
                    "nodes_created": r.nodes_created,
                    "success": r.success,
                    "error": r.error,
                    "collection": r.collection,
                    "archived": r.archived,
                })
            })
            .collect(),
    )
}

async fn run_file(client: &mut ImportClient, args: ImportFileArgs, json: bool) -> Result<()> {
    let opts = ImportOptions {
        collection: args.collection.unwrap_or_default(),
        use_filename_as_title: args.use_filename_as_title,
        auto_collection_routing: args.auto_collection_routing,
        exclude_patterns: vec![],
        base_directory: String::new(),
        replace: args.replace,
        // A single explicit file is never walked, so the folder-walk filters
        // are inert here; carry their proto defaults for a uniform option surface.
        include_agent_files: false,
        include_hidden: false,
        no_recursion: false,
    };

    let mut stream = client
        .import_markdown(Request::new(ImportMarkdownRequest {
            file_path: args.file,
            options: Some(opts),
        }))
        .await?
        .into_inner();

    while let Some(event) = stream.next().await {
        let event = event?;
        if json {
            if event.step == 9 {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&results_to_json(&event.results))?
                );
            }
        } else {
            eprintln!("[{}/9] {}: {}", event.step, event.step_name, event.message);
            if event.step == 9 {
                for r in &event.results {
                    if r.success {
                        println!(
                            "✓ {} ({} nodes){}",
                            r.file_path,
                            r.nodes_created,
                            if r.collection.is_empty() {
                                String::new()
                            } else {
                                format!(" → {}", r.collection)
                            }
                        );
                    } else {
                        println!("✗ {}: {}", r.file_path, r.error);
                    }
                }
            }
        }
    }

    Ok(())
}

async fn run_dir(client: &mut ImportClient, args: ImportDirArgs, json: bool) -> Result<()> {
    // The three folder-walk filters default ON; each CLI flag opts out of one,
    // so the option (recorded in ImportOptions with the inverse polarity) is the
    // flag itself. See WalkFilters for the applied semantics.
    let filters = WalkFilters {
        exclude_agent_files: !args.include_agent_files,
        skip_hidden: !args.include_hidden,
        recursive: !args.no_recursive,
    };
    let file_paths = collect_markdown_files(&args.directory, &args.exclude_patterns, filters)?;

    if file_paths.is_empty() {
        if !json {
            eprintln!("No markdown files found in {}", args.directory);
        }
        return Ok(());
    }

    let opts = ImportOptions {
        collection: args.collection.unwrap_or_default(),
        use_filename_as_title: args.use_filename_as_title,
        auto_collection_routing: args.auto_collection_routing,
        exclude_patterns: args.exclude_patterns,
        base_directory: args.directory,
        replace: args.replace,
        include_agent_files: args.include_agent_files,
        include_hidden: args.include_hidden,
        no_recursion: args.no_recursive,
    };

    let mut stream = client
        .import_markdown_files(Request::new(ImportMarkdownFilesRequest {
            file_paths,
            options: Some(opts),
        }))
        .await?
        .into_inner();

    while let Some(event) = stream.next().await {
        let event = event?;
        if json {
            if event.step == 9 {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&results_to_json(&event.results))?
                );
            }
        } else {
            eprintln!("[{}/9] {}: {}", event.step, event.step_name, event.message);
            if event.step == 9 {
                // The daemon's summary is the source of truth — it distinguishes
                // newly imported, refreshed (--replace), and already-present
                // (skipped) documents, so a plain re-import reads honestly
                // instead of claiming to have re-imported everything.
                println!("{}", event.message);
                for r in event.results.iter().filter(|r| !r.success) {
                    println!("  ✗ {}: {}", r.file_path, r.error);
                }
            }
        }
    }

    Ok(())
}

/// Filters applied while walking a directory for import. Each field is stated
/// in its active/default-on sense (the folder-import options all default ON),
/// so the fields are true when the corresponding filter is applied.
#[derive(Clone, Copy)]
struct WalkFilters {
    /// Skip files whose basename is CLAUDE.md / AGENTS.md (case-insensitive).
    exclude_agent_files: bool,
    /// Skip any entry (file or dir) whose name starts with '.'.
    skip_hidden: bool,
    /// Descend into sub-folders. When false, only the top-level dir is scanned.
    recursive: bool,
}

/// Basenames dropped by the "exclude agent files" filter — matched
/// case-insensitively against the file's basename at any depth.
fn is_agent_file(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.eq_ignore_ascii_case("CLAUDE.md") || n.eq_ignore_ascii_case("AGENTS.md"))
        .unwrap_or(false)
}

/// A hidden entry is one whose own name starts with '.'. The walk visits every
/// path component as an entry, so testing the immediate name at each level is
/// equivalent to "any path component starting with '.'".
fn is_hidden(path: &std::path::Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with('.'))
        .unwrap_or(false)
}

fn collect_markdown_files(
    dir: &str,
    exclude_patterns: &[String],
    filters: WalkFilters,
) -> Result<Vec<String>> {
    let mut files = Vec::new();
    collect_recursive(
        std::path::Path::new(dir),
        &mut files,
        exclude_patterns,
        filters,
    )?;
    files.sort();
    Ok(files)
}

fn collect_recursive(
    dir: &std::path::Path,
    files: &mut Vec<String>,
    exclude_patterns: &[String],
    filters: WalkFilters,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let path_str = path.to_string_lossy();

        // User-supplied --exclude patterns (component name or path substring),
        // preserved and composed with the new default-on filters below.
        let excluded = exclude_patterns.iter().any(|p| {
            path.components().any(|c| {
                c.as_os_str()
                    .to_str()
                    .map(|s| s.eq_ignore_ascii_case(p))
                    .unwrap_or(false)
            }) || path_str.contains(p.as_str())
        });

        if excluded {
            continue;
        }

        // Skip hidden entries (files and dirs) before any descent or push.
        if filters.skip_hidden && is_hidden(&path) {
            continue;
        }

        if path.is_dir() {
            if filters.recursive {
                collect_recursive(&path, files, exclude_patterns, filters)?;
            }
        } else if path.extension().map(|e| e == "md").unwrap_or(false) {
            if filters.exclude_agent_files && is_agent_file(&path) {
                continue;
            }
            if let Some(s) = path.to_str() {
                files.push(s.to_string());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    use std::fs;
    use tempfile::TempDir;

    /// Build a fixture tree covering every filter axis:
    ///   top.md            top-level plain doc
    ///   CLAUDE.md         top-level agent file (mixed case)
    ///   AGENTS.md         top-level agent file
    ///   .dotfile.md       top-level hidden dotfile doc
    ///   sub/nested.md     doc in a sub-folder
    ///   sub/claude.md     nested, lowercase agent file (case-insensitive + depth)
    ///   .hidden/hidden.md doc under a hidden directory
    fn fixture() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        fs::write(root.join("top.md"), "# top").unwrap();
        fs::write(root.join("CLAUDE.md"), "# claude").unwrap();
        fs::write(root.join("AGENTS.md"), "# agents").unwrap();
        fs::write(root.join(".dotfile.md"), "# dotfile").unwrap();

        let sub = root.join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("nested.md"), "# nested").unwrap();
        fs::write(sub.join("claude.md"), "# nested claude").unwrap();

        let hidden = root.join(".hidden");
        fs::create_dir(&hidden).unwrap();
        fs::write(hidden.join("hidden.md"), "# hidden").unwrap();

        tmp
    }

    /// Sorted set of basenames from the collected absolute paths.
    fn names(tmp: &TempDir, exclude: &[String], filters: WalkFilters) -> BTreeSet<String> {
        collect_markdown_files(tmp.path().to_str().unwrap(), exclude, filters)
            .unwrap()
            .into_iter()
            .map(|p| {
                std::path::Path::new(&p)
                    .file_name()
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }

    fn set(items: &[&str]) -> BTreeSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    const ALL_ON: WalkFilters = WalkFilters {
        exclude_agent_files: true,
        skip_hidden: true,
        recursive: true,
    };

    #[test]
    fn defaults_exclude_agent_and_hidden_but_recurse() {
        let tmp = fixture();
        // Agent files (any depth, any case) and hidden entries are dropped;
        // sub-folders are still walked.
        assert_eq!(names(&tmp, &[], ALL_ON), set(&["top.md", "nested.md"]));
    }

    #[test]
    fn including_agent_files_keeps_claude_and_agents_at_any_depth() {
        let tmp = fixture();
        let filters = WalkFilters {
            exclude_agent_files: false,
            ..ALL_ON
        };
        // Top-level CLAUDE.md/AGENTS.md and the nested lowercase claude.md return;
        // hidden entries stay excluded.
        assert_eq!(
            names(&tmp, &[], filters),
            set(&["top.md", "nested.md", "CLAUDE.md", "AGENTS.md", "claude.md"])
        );
    }

    #[test]
    fn including_hidden_keeps_dotfiles_and_dot_dirs() {
        let tmp = fixture();
        let filters = WalkFilters {
            skip_hidden: false,
            ..ALL_ON
        };
        // The dotfile doc and the doc under .hidden/ appear; agent files still drop.
        assert_eq!(
            names(&tmp, &[], filters),
            set(&["top.md", "nested.md", ".dotfile.md", "hidden.md"])
        );
    }

    #[test]
    fn non_recursive_scans_only_top_level() {
        let tmp = fixture();
        let filters = WalkFilters {
            recursive: false,
            ..ALL_ON
        };
        // Only the top-level plain doc; sub/ is not descended into, agent +
        // hidden top-level entries still filtered.
        assert_eq!(names(&tmp, &[], filters), set(&["top.md"]));
    }

    #[test]
    fn all_filters_off_collects_everything() {
        let tmp = fixture();
        let filters = WalkFilters {
            exclude_agent_files: false,
            skip_hidden: false,
            recursive: true,
        };
        assert_eq!(
            names(&tmp, &[], filters),
            set(&[
                "top.md",
                "nested.md",
                "CLAUDE.md",
                "AGENTS.md",
                "claude.md",
                ".dotfile.md",
                "hidden.md",
            ])
        );
    }

    #[test]
    fn filters_compose_with_exclude_patterns() {
        let tmp = fixture();
        // --exclude sub removes the whole sub-folder; default filters still drop
        // agent + hidden entries, leaving only the top-level plain doc.
        assert_eq!(names(&tmp, &["sub".to_string()], ALL_ON), set(&["top.md"]));
    }

    #[test]
    fn non_recursive_still_excludes_top_level_agent_and_hidden() {
        let tmp = fixture();
        // Turn every filter off except recursion to prove the non-recursive mode
        // is orthogonal: at top level it keeps agent + hidden entries too.
        let filters = WalkFilters {
            exclude_agent_files: false,
            skip_hidden: false,
            recursive: false,
        };
        assert_eq!(
            names(&tmp, &[], filters),
            set(&["top.md", "CLAUDE.md", "AGENTS.md", ".dotfile.md"])
        );
    }
}
