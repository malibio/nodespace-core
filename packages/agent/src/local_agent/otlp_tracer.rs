//! OTLP tracer for dev observability.
//!
//! When `NODESPACE_MLFLOW_URL` is set, initialises an OpenTelemetry OTLP
//! exporter pointed at that URL and installs it as the global tracer provider.
//! When unset, the default no-op provider remains — callers pay zero overhead.
//!
//! MLflow 3.x includes a native OTLP HTTP receiver. Point it at
//! `http://localhost:5000/api/2.0/mlflow/otlp`. See `scripts/mlflow-dev.md`.
//!
//! Issue #1341

use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::trace::SdkTracerProvider;

/// Name used for all traces emitted by the agent.
pub const TRACER_NAME: &str = "nodespace-agent";

/// The env var that enables tracing. When unset, all spans are no-ops.
const ENV_MLFLOW_URL: &str = "NODESPACE_MLFLOW_URL";

/// OTLP path appended to the MLflow base URL.
const OTLP_PATH: &str = "/api/2.0/mlflow/otlp";

/// Initialise OTLP tracing if `NODESPACE_MLFLOW_URL` is set.
///
/// Installs the provider as the global OTel tracer provider. Call once at
/// daemon startup. The returned `SdkTracerProvider` must be kept alive for the
/// duration of the process — dropping it shuts down the background exporter.
/// When the env var is absent this is a no-op and returns `None`.
pub fn init_tracer() -> Option<SdkTracerProvider> {
    let mlflow_url = std::env::var(ENV_MLFLOW_URL).ok()?;
    let mlflow_url = mlflow_url.trim();
    if mlflow_url.is_empty() {
        return None;
    }

    let endpoint = format!("{}{}", mlflow_url.trim_end_matches('/'), OTLP_PATH);
    tracing::info!(endpoint = %endpoint, "OTLP tracing enabled (NODESPACE_MLFLOW_URL is set)");

    let exporter = match opentelemetry_otlp::SpanExporter::builder()
        .with_http()
        .with_endpoint(&endpoint)
        .build()
    {
        Ok(e) => e,
        Err(err) => {
            tracing::warn!(error = %err, "Failed to build OTLP exporter; tracing disabled");
            return None;
        }
    };

    let resource = opentelemetry_sdk::Resource::builder()
        .with_service_name("nodespace-agent")
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_resource(resource)
        .build();

    opentelemetry::global::set_tracer_provider(provider.clone());
    Some(provider)
}
