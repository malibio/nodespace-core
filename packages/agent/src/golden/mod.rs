//! Golden-prompt tuning: prompts as data, validated against the real
//! inference path with no NodeSpace plumbing in between.
//!
//! The methodology this generalizes was proved by
//! `tests/golden_scenario6_handauthored.rs` and
//! `tests/golden_scenario6_sequence.rs`: assemble a sequence of literal prompt
//! strings, validate them directly against llama.cpp, and iterate until a
//! version reliably gets the correct tool call. *That* becomes the target the
//! real assembly pipeline is engineered toward — not the other way around.
//!
//! Those two files stayed bespoke: one scenario, hand-rolled engine loading,
//! assertions written in Rust, prompts as string literals. Every prompt tweak
//! needed a recompile, which is why the approach never became the general
//! tuning loop. Here the prompt is a TOML file and the loop is
//! edit-and-rerun.
//!
//! Two questions this deliberately keeps apart:
//!
//! - *Does this exact prompt text get the right tool call?* — seconds, here.
//! - *Does the real pipeline assemble that exact prompt?* — deterministic and
//!   model-free, and not this module's job.
//!
//! Conflating them is what made every prior checkpoint cost a daemon rebuild,
//! a database purge, and minutes per iteration.
//!
//! [`case`] owns the file format and expectation evaluation, both model-free
//! and unit-tested in the default run. [`runner`] owns execution and is
//! reachable only through the `golden_runner` bin, which is outside
//! `cargo test` entirely — it loads a ~5GB GGUF.
//!
//! Committed cases live in `packages/agent/goldens/`. [`case`]'s module doc
//! states the format; the committed cases are its worked reference
//! instances.

pub mod case;
pub mod runner;
