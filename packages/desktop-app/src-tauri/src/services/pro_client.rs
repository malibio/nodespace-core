//! Pro-tier gRPC client (`nodespace.pro.v1.CloudSyncService`).
//!
//! Used when the Tauri app talks to `nodespaced-pro` (from the
//! private `nodespace-sync` repo). The same `tonic::transport::Channel`
//! that drives `GrpcClient` is reused — one connection, two service
//! surfaces (community `nodespace.v1` + Pro `nodespace.pro.v1`).
//!
//! Capability probe: the daemon is "Pro tier" if a single
//! `WatchSyncStatus` call returns at least one event. A community
//! daemon (`nodespaced` from core) doesn't register this service and
//! returns `Status::Unimplemented`, which we surface as "community"
//! to the rest of the app.

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::RwLock;
use tonic::transport::Channel;

/// Generated bindings for `nodespace.pro.v1`. The proto file lives
/// under `proto/nodespace_pro.proto` in this crate (vendored from
/// `nodespace-sync/nodespaced-pro/proto/`).
pub mod pb {
    tonic::include_proto!("nodespace.pro.v1");
}

use pb::cloud_sync_service_client::CloudSyncServiceClient;
use pb::sync_status_event::State as PbState;
use pb::{SyncStatusEvent, WatchSyncStatusRequest};

const PROBE_TIMEOUT: Duration = Duration::from_secs(2);

/// Pro-tier client + tier-detection state.
#[derive(Clone)]
pub struct ProClient {
    inner: Arc<RwLock<ProClientInner>>,
}

struct ProClientInner {
    client: CloudSyncServiceClient<Channel>,
    tier: ProTier,
    last_status: Option<SyncStatusEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProTier {
    /// Daemon implements `CloudSyncService` — Pro tier.
    Pro,
    /// Daemon returned `Unimplemented` on the probe — community tier.
    Community,
    /// Probe didn't complete (timeout, transport error, …). Treated
    /// as community by the UI but kept distinct for diagnostics.
    Unknown,
}

impl ProClient {
    /// Probe for the Pro service on an existing channel. The channel
    /// is shared with `GrpcClient` so both service surfaces ride the
    /// same h2 connection — opening a parallel channel here caused
    /// "Service was not ready: transport error" on subsequent calls
    /// after the probe stream was dropped.
    pub async fn probe_on_channel(channel: Channel) -> Self {
        let mut client = CloudSyncServiceClient::new(channel);
        let (tier, last_status) = probe(&mut client).await;
        tracing::info!(?tier, "Pro capability probe complete");

        Self {
            inner: Arc::new(RwLock::new(ProClientInner {
                client,
                tier,
                last_status,
            })),
        }
    }

    pub async fn tier(&self) -> ProTier {
        self.inner.read().await.tier
    }

    pub async fn last_status(&self) -> Option<SyncStatusEvent> {
        self.inner.read().await.last_status.clone()
    }

    pub async fn client(&self) -> CloudSyncServiceClient<Channel> {
        self.inner.read().await.client.clone()
    }
}

/// Single-shot probe of `WatchSyncStatus`. Returns the detected tier
/// and the first event if one arrives within the timeout.
async fn probe(client: &mut CloudSyncServiceClient<Channel>) -> (ProTier, Option<SyncStatusEvent>) {
    let probe_call = async {
        let stream = client
            .watch_sync_status(WatchSyncStatusRequest {})
            .await?
            .into_inner();
        Ok::<_, tonic::Status>(stream)
    };

    let stream_result = match tokio::time::timeout(PROBE_TIMEOUT, probe_call).await {
        Ok(Ok(s)) => s,
        Ok(Err(status)) => {
            return if status.code() == tonic::Code::Unimplemented {
                tracing::info!("CloudSyncService unimplemented — community tier");
                (ProTier::Community, None)
            } else {
                tracing::warn!(error = %status, "Pro probe returned error");
                (ProTier::Unknown, None)
            };
        }
        Err(_) => {
            tracing::warn!("Pro probe timed out — treating as community");
            return (ProTier::Unknown, None);
        }
    };

    // Pull the first event so the UI gets the current snapshot
    // immediately instead of waiting for the next transition.
    use tokio_stream::StreamExt;
    let mut stream = stream_result;
    match tokio::time::timeout(PROBE_TIMEOUT, stream.next()).await {
        Ok(Some(Ok(evt))) => {
            tracing::info!(
                state = evt.state,
                detail = %evt.detail,
                "Pro probe received first event"
            );
            (ProTier::Pro, Some(evt))
        }
        Ok(Some(Err(status))) => {
            tracing::warn!(error = %status, "Pro probe stream error");
            (ProTier::Unknown, None)
        }
        Ok(None) | Err(_) => {
            // The daemon implements the service but didn't push an
            // event in time. Still Pro — just no current snapshot.
            (
                ProTier::Pro,
                Some(SyncStatusEvent {
                    state: PbState::Unspecified as i32,
                    detail: "probe completed without initial event".into(),
                }),
            )
        }
    }
}
