//! Runs a golden prompt case against the locked native model and prints what
//! came back.
//!
//! The loop this exists for: open a case file, shorten the system prompt or
//! reword a tool description, re-run, read the output, decide. No recompile —
//! the prompt is data. No daemon, database, gRPC, agent loop,
//! `PromptAssembler`, or routing — none of them change what the model sees,
//! and all of them cost minutes.
//!
//! It deliberately does not assert or score. The human judges the output.
//!
//! Note which half is the deliverable: the **case files** are the artifact —
//! a set of prompt strings that reliably get the right tool call, which the
//! real assembly pipeline is then engineered to reproduce. This bin is the
//! scaffolding that produces them.
//!
//! A bin rather than a test, so it sits outside `cargo test` entirely and
//! cannot be pulled into the default run: it loads a ~5GB GGUF.
//!
//! Usage:
//! ```text
//! cargo run --release -p nodespace-agent --bin golden_runner -- packages/agent/goldens/indirect-reference-resolves.toml
//! cargo run --release -p nodespace-agent --bin golden_runner -- --reps 5 <case.toml>
//! cargo run --release -p nodespace-agent --bin golden_runner -- --check <case.toml>
//! cargo run --release -p nodespace-agent --bin golden_runner -- --model /path/to.gguf <case.toml>
//! ```
//!
//! `--check` parses the case and prints every string the model would see,
//! escaped, without loading the model. Escaped because the mistakes worth
//! catching are invisible otherwise — a stray leading newline, an indent TOML
//! added, a doubled space at a line continuation.
//!
//! To capture the **post-template** string (what llama.cpp actually receives,
//! after the chat template is applied), set `NODESPACE_PROMPT_DUMP` when
//! running this bin. It hooks the single native-path chokepoint in
//! `nlp-engine`'s `chat/mod.rs`, so it needs nothing from this utility.

use std::path::PathBuf;
use std::process::ExitCode;

use nodespace_agent::golden::case::GoldenCase;
use nodespace_agent::golden::runner::{default_model_path, load_engine, run_case};

const USAGE: &str = "usage: golden_runner [--model <path.gguf>] [--reps <n>] [--check] <case.toml>

  --model   GGUF to load (default: $HOME/.nodespace/models/gemma-4-E4B-it-Q4_K_M.gguf)
  --reps    override the case's declared rep count
  --check   parse the case and print what the model would see, without loading it";

struct Args {
    case: PathBuf,
    model: PathBuf,
    reps: Option<u32>,
    check_only: bool,
}

fn parse_args() -> Result<Args, String> {
    let mut case: Option<PathBuf> = None;
    let mut model = PathBuf::from(default_model_path());
    let mut reps = None;
    let mut check_only = false;

    let mut argv = std::env::args().skip(1);
    while let Some(arg) = argv.next() {
        match arg.as_str() {
            "--model" => model = PathBuf::from(argv.next().ok_or("--model needs a path")?),
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
            "-h" | "--help" => return Err(USAGE.into()),
            other if other.starts_with('-') => return Err(format!("unknown flag {other}")),
            other if case.is_none() => case = Some(PathBuf::from(other)),
            other => return Err(format!("unexpected second case file {other}; pass one")),
        }
    }

    Ok(Args {
        case: case.ok_or(USAGE)?,
        model,
        reps,
        check_only,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("{msg}");
            return ExitCode::FAILURE;
        }
    };

    let mut case = match GoldenCase::load(&args.case) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}: {e}", args.case.display());
            return ExitCode::FAILURE;
        }
    };
    if let Some(n) = args.reps {
        case.reps = n;
    }

    if args.check_only {
        print_case(&case);
        return ExitCode::SUCCESS;
    }

    if !args.model.exists() {
        eprintln!(
            "model not found at {} — pass --model, or fetch the locked native model",
            args.model.display()
        );
        return ExitCode::FAILURE;
    }

    println!("loading {} …", args.model.display());
    let engine = match load_engine(&args.model) {
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
    runtime.block_on(run_case(&engine, &case));

    ExitCode::SUCCESS
}

/// Print exactly what the model would be sent, for `--check`.
fn print_case(case: &GoldenCase) {
    println!(
        "{} ({} turn(s), {} rep(s))",
        case.name,
        case.turns.len(),
        case.reps
    );
    for (i, turn) in case.turns.iter().enumerate() {
        println!("  {}", case.turn_label(i));
        println!("    system  = {:?}", turn.system);
        for (h, msg) in turn.history.iter().enumerate() {
            println!("    hist[{h}] = {:?}", msg.content);
        }
        println!("    user    = {:?}", turn.user);
        for tool in &turn.tools {
            match tool.to_tool_definition() {
                Ok(def) => {
                    println!("    tool {} = {:?}", def.name, def.description);
                    println!(
                        "      schema = {}",
                        serde_json::to_string(&def.parameters_schema)
                            .unwrap_or_else(|e| format!("<unserializable: {e}>"))
                    );
                }
                Err(e) => println!("    tool {} = <invalid: {e}>", tool.name),
            }
        }
        for (name, result) in &turn.tool_results {
            println!("    result[{name}] = {result:?}");
        }
        if !turn.expect.is_empty() {
            println!("    expect  = {}", turn.expect);
        }
    }
}
