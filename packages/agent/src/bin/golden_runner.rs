//! Runs golden prompt cases against the locked native model.
//!
//! The tuning loop this exists for: open a case file, shorten the system
//! prompt or reword a tool description, re-run, see whether the model still
//! does the right thing. No recompile — the prompt is data. No daemon,
//! database, gRPC, agent loop, `PromptAssembler`, or routing — none of them
//! change what the model sees, and all of them cost minutes.
//!
//! A bin rather than a test, so it sits outside `cargo test` entirely and
//! cannot be pulled into the default run: it loads a ~5GB GGUF.
//!
//! Usage:
//! ```text
//! cargo run -p nodespace-agent --bin golden_runner -- packages/agent/goldens/scenario6-turn3-zero-history.toml
//! cargo run -p nodespace-agent --bin golden_runner -- packages/agent/goldens          # every case in a directory
//! cargo run -p nodespace-agent --bin golden_runner -- --reps 5 <case.toml>            # override the case's reps
//! cargo run -p nodespace-agent --bin golden_runner -- --check <case.toml>             # parse only, no model load
//! cargo run -p nodespace-agent --bin golden_runner -- --model /path/to.gguf <case>
//! ```
//!
//! Exit code is 1 if any case failed, so a case set can gate something later.
//! `observe` cases never contribute a failure.
//!
//! `NODESPACE_PROMPT_DUMP` still applies: set it to dump the exact final
//! templated string for each turn, which is the way to seed a new case file
//! from a real session's assembled prompt.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use nodespace_agent::golden::case::GoldenCase;
use nodespace_agent::golden::runner::{
    default_model_path, load_engine, render_report, run_case, CaseRun,
};

struct Args {
    paths: Vec<PathBuf>,
    model: PathBuf,
    reps: Option<u32>,
    check_only: bool,
}

fn usage() -> &'static str {
    "usage: golden_runner [--model <path.gguf>] [--reps <n>] [--check] <case.toml|dir> ...

  --model   GGUF to load (default: $HOME/.nodespace/models/gemma-4-E4B-it-Q4_K_M.gguf)
  --reps    override every case's declared rep count
  --check   parse and validate the case files without loading the model"
}

fn parse_args() -> Result<Args, String> {
    let mut paths = Vec::new();
    let mut model = PathBuf::from(default_model_path());
    let mut reps = None;
    let mut check_only = false;

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--model" => {
                let v = argv.next().ok_or("--model needs a path")?;
                model = PathBuf::from(v);
            }
            "--reps" => {
                let v = argv.next().ok_or("--reps needs a number")?;
                let n: u32 = v
                    .parse()
                    .map_err(|_| format!("--reps: {v} is not a number"))?;
                if n == 0 {
                    return Err("--reps must be at least 1".into());
                }
                reps = Some(n);
            }
            "--check" => check_only = true,
            "-h" | "--help" => return Err(usage().into()),
            other if other.starts_with('-') => return Err(format!("unknown flag {other}")),
            other => paths.push(PathBuf::from(other)),
        }
    }

    if paths.is_empty() {
        return Err(usage().into());
    }
    Ok(Args {
        paths,
        model,
        reps,
        check_only,
    })
}

/// Expand directories into the `.toml` files directly inside them, sorted so
/// a run's output order is stable across machines.
fn collect_case_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    for path in paths {
        if path.is_dir() {
            let mut found: Vec<PathBuf> = std::fs::read_dir(path)
                .map_err(|e| format!("could not read {}: {e}", path.display()))?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "toml"))
                .collect();
            found.sort();
            if found.is_empty() {
                return Err(format!("{} contains no .toml case files", path.display()));
            }
            files.extend(found);
        } else {
            files.push(path.clone());
        }
    }
    Ok(files)
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    let files = match collect_case_files(&args.paths) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    // Every case is parsed before the model loads. A typo in the last file of
    // a set should not surface several minutes and one 5GB load later.
    let mut cases = Vec::with_capacity(files.len());
    for file in &files {
        match GoldenCase::load(file) {
            Ok(mut case) => {
                if let Some(n) = args.reps {
                    case.reps = n;
                }
                cases.push(case);
            }
            Err(e) => {
                eprintln!("{}: {e}", file.display());
                return ExitCode::FAILURE;
            }
        }
    }

    if args.check_only {
        for (case, file) in cases.iter().zip(&files) {
            println!(
                "ok  {} ({} turn(s), {} rep(s))  {}",
                case.name,
                case.turns.len(),
                case.reps,
                file.display()
            );
            // The tool surface after TOML→JSON conversion, because a schema
            // that converted to a shape the author did not intend is
            // invisible otherwise: the model just behaves oddly and the
            // prompt gets blamed.
            for (i, turn) in case.turns.iter().enumerate() {
                for tool in &turn.tools {
                    match tool.to_tool_definition() {
                        Ok(def) => println!(
                            "      {} {}: {}",
                            case.turn_label(i),
                            def.name,
                            serde_json::to_string(&def.parameters_schema)
                                .unwrap_or_else(|e| format!("<unserializable: {e}>"))
                        ),
                        Err(e) => {
                            eprintln!("{}: {e}", file.display());
                            return ExitCode::FAILURE;
                        }
                    }
                }
            }
        }
        return ExitCode::SUCCESS;
    }

    if !args.model.exists() {
        eprintln!(
            "model not found at {} — pass --model, or fetch the locked native model",
            args.model.display()
        );
        return ExitCode::FAILURE;
    }

    run(&args.model, &cases)
}

fn run(model: &Path, cases: &[GoldenCase]) -> ExitCode {
    println!("loading {} …", model.display());
    let engine = match load_engine(model) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("could not start the tokio runtime: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut runs: Vec<CaseRun> = Vec::with_capacity(cases.len());
    for case in cases {
        runs.push(runtime.block_on(run_case(&engine, case)));
    }

    for run in &runs {
        print!("{}", render_report(run));
    }

    let failed: Vec<&CaseRun> = runs.iter().filter(|r| !r.is_pass()).collect();
    println!(
        "\n{} of {} case(s) passed all reps",
        runs.len() - failed.len(),
        runs.len()
    );
    if failed.is_empty() {
        ExitCode::SUCCESS
    } else {
        for run in failed {
            println!("  FAILED {}: {}/{}", run.name, run.passes(), run.reps.len());
        }
        ExitCode::FAILURE
    }
}
