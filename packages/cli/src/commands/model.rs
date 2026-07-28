//! `nodespace model ...` — manage the local inference model.
//!
//! The daemon starts with a no-op inference engine; a model must be loaded
//! before ai-chat turns can run. The desktop app normally does this on startup
//! via `EnsureModelReady`. These subcommands expose the same flow to the shell
//! so the agent can be driven (and its prompting tuned) without the UI.

use anyhow::{Context, Result};
use clap::{Args, Subcommand};
use nodespace_daemon::nodespace::{
    EnsureModelReadyRequest, GetLocalStatusRequest, GetSystemRamRequest, ListModelsRequest,
    RecommendedModelRequest,
};
use serde_json::json;

use crate::LocalAgentClient;

#[derive(Subcommand, Debug)]
pub enum ModelAction {
    /// List models in the catalog and their download/load status.
    List,
    /// Load a model (downloading first if needed); streams progress to stdout.
    Load(LoadArgs),
    /// Print the recommended model id for this machine's RAM.
    Recommended,
    /// Print the loaded model, the context window granted to it, and host RAM.
    Status,
}

#[derive(Args, Debug)]
pub struct LoadArgs {
    /// Model id to load, e.g. `gemma-4-e4b-q4km`. Omit to use the recommended model.
    pub model_id: Option<String>,
}

pub async fn run(client: &mut LocalAgentClient, action: ModelAction, json: bool) -> Result<()> {
    match action {
        ModelAction::List => list(client, json).await,
        ModelAction::Load(args) => load(client, args, json).await,
        ModelAction::Recommended => recommended(client, json).await,
        ModelAction::Status => status(client, json).await,
    }
}

async fn list(client: &mut LocalAgentClient, json: bool) -> Result<()> {
    let response = client
        .list_models(ListModelsRequest {})
        .await
        .context("ListModels RPC failed")?
        .into_inner();

    if json {
        let models: Vec<_> = response
            .models
            .iter()
            .map(|m| {
                json!({
                    "id": m.id,
                    "name": m.name,
                    "backend": m.backend,
                    "status": serde_json::from_str::<serde_json::Value>(&m.status_json)
                        .unwrap_or(serde_json::Value::String(m.status_json.clone())),
                    "size_bytes": m.size_bytes,
                    "quantization": m.quantization,
                    "min_memory_gb": m.min_memory_gb,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "models": models }))?
        );
    } else {
        for m in &response.models {
            // status_json is a serialized enum, shaped `{"status": "loaded"}`
            // (internally tagged) or sometimes a bare string. Render the tag.
            let status = serde_json::from_str::<serde_json::Value>(&m.status_json)
                .ok()
                .and_then(|v| match v {
                    serde_json::Value::String(s) => Some(s),
                    serde_json::Value::Object(o) => o
                        .get("status")
                        .and_then(|s| s.as_str().map(str::to_string))
                        .or_else(|| o.keys().next().cloned()),
                    _ => None,
                })
                .unwrap_or_else(|| m.status_json.clone());
            println!(
                "{:<22} {:<8} {:>6} GB  {}",
                m.id, m.backend, m.min_memory_gb, status
            );
        }
    }
    Ok(())
}

async fn load(client: &mut LocalAgentClient, args: LoadArgs, json: bool) -> Result<()> {
    let model_id = match args.model_id {
        Some(id) => id,
        None => {
            client
                .recommended_model(RecommendedModelRequest {})
                .await
                .context("RecommendedModel RPC failed")?
                .into_inner()
                .model_id
        }
    };

    let mut stream = client
        .ensure_model_ready(EnsureModelReadyRequest {
            model_id: model_id.clone(),
        })
        .await
        .context("EnsureModelReady RPC failed")?
        .into_inner();

    let mut last_event = String::new();
    while let Some(event) = stream.message().await.context("model load stream error")? {
        last_event = event.event_type.clone();
        if json {
            println!(
                "{}",
                serde_json::to_string(&json!({
                    "event_type": event.event_type,
                    "model_id": event.model_id,
                    "message": event.message,
                    "bytes_downloaded": event.bytes_downloaded,
                    "bytes_total": event.bytes_total,
                    "error_message": event.error_message,
                    "engine_swapped": event.engine_swapped,
                }))?
            );
        } else {
            match event.event_type.as_str() {
                "downloading" => {
                    if let (Some(d), Some(t)) = (event.bytes_downloaded, event.bytes_total) {
                        let pct = if t > 0 {
                            d as f64 / t as f64 * 100.0
                        } else {
                            0.0
                        };
                        println!("downloading {model_id}: {pct:.0}%");
                    } else {
                        println!("downloading {model_id}...");
                    }
                }
                "error" => {
                    let msg = event.error_message.unwrap_or_default();
                    anyhow::bail!("model load failed: {msg}");
                }
                other => {
                    let detail = event.message.unwrap_or_default();
                    println!("{other}: {detail}");
                }
            }
        }
    }

    if last_event != "ready" && last_event != "error" {
        // Stream ended without an explicit terminal event — surface it.
        eprintln!("warning: model load stream ended on '{last_event}'");
    }
    Ok(())
}

/// Report what the daemon actually has loaded.
///
/// `granted_n_ctx` is the window the engine allocated at load time, which is
/// sized to available memory and can be well below the configured ceiling. A
/// caller that intends to send a large system prompt must be able to check it
/// up front; otherwise an impossible request fails one turn at a time and looks
/// like a model that will not act rather than an environment that cannot run it.
async fn status(client: &mut LocalAgentClient, json: bool) -> Result<()> {
    let status = client
        .get_status(GetLocalStatusRequest { session_id: None })
        .await
        .context("GetStatus RPC failed")?
        .into_inner();

    let ram_bytes = client
        .get_system_ram(GetSystemRamRequest {})
        .await
        .context("GetSystemRam RPC failed")?
        .into_inner()
        .ram_bytes;

    let loaded = !status.model_id.is_empty();

    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "loaded": loaded,
                "model_id": status.model_id,
                "granted_n_ctx": status.granted_n_ctx,
                "status": status.status_json,
                "host_ram_bytes": ram_bytes,
            }))?
        );
    } else if loaded {
        println!(
            "{} (n_ctx {}, host RAM {:.1} GB)",
            status.model_id,
            status.granted_n_ctx,
            ram_bytes as f64 / 1e9,
        );
    } else {
        println!(
            "no model loaded (host RAM {:.1} GB)",
            ram_bytes as f64 / 1e9
        );
    }
    Ok(())
}

async fn recommended(client: &mut LocalAgentClient, json: bool) -> Result<()> {
    let model_id = client
        .recommended_model(RecommendedModelRequest {})
        .await
        .context("RecommendedModel RPC failed")?
        .into_inner()
        .model_id;
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({ "model_id": model_id }))?
        );
    } else {
        println!("{model_id}");
    }
    Ok(())
}
