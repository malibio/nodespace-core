//! tonic `SettingsService` implementation.
//!
//! Reads and writes daemon configuration from `~/.nodespace/daemon.toml`.
//! Display preferences (theme, render_markdown) are UI-only and live in the
//! Tauri process — this service does not touch them.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tonic::{Request, Response, Status};

use crate::nodespace::{
    settings_service_server::SettingsService as GrpcSettingsService, CaptureContentLevel,
    CaptureSettingsResponse, GetCaptureSettingsRequest, ListOpenAiCompatConfigsRequest,
    ListOpenAiCompatConfigsResponse, OpenAiCompatConfig as ProtoOpenAiCompatConfig,
    SetOpenAiCompatConfigsRequest, UpdateCaptureSettingsRequest,
};

/// On-disk representation of `~/.nodespace/daemon.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DaemonConfig {
    #[serde(default)]
    capture: CaptureConfig,
    #[serde(default)]
    openai_compat: OpenAiCompatSettings,
}

/// OpenAI-compatible provider configs persisted in daemon.toml under
/// `[[openai_compat.configs]]`. The Settings GUI is the only writer; the
/// daemon reads this by UUID when loading an `openai-compat:<uuid>` model.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OpenAiCompatSettings {
    #[serde(default)]
    pub configs: Vec<OpenAiCompatConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenAiCompatConfig {
    pub id: String,
    pub name: String,
    pub base_url: String,
    #[serde(default)]
    pub api_key: String,
    /// Model identifier sent as the wire-protocol "model" field — distinct
    /// from `name`, which is only a cosmetic UI label.
    #[serde(default)]
    pub model: String,
    /// Cached verdict from the routing-reliability probe (see
    /// `nodespace_agent::local_agent::routing_probe`), keyed to this config's
    /// `(base_url, model)` pair at the time the probe ran.
    ///
    /// `None` means "never probed" — e.g. a config created before this field
    /// existed, or one whose model load has not completed since. `Some(false)`
    /// means the probe observed the injected Stage-2 candidate block suppress
    /// tool-calling on this served model; Stage-2 injection is skipped for it
    /// until a future probe (after the served model changes) says otherwise.
    /// Not part of the gRPC `OpenAiCompatConfig` message: this is daemon-owned
    /// cache state derived from a live probe, not something the Settings GUI
    /// writes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub routing_ok: Option<bool>,
}

impl From<ProtoOpenAiCompatConfig> for OpenAiCompatConfig {
    fn from(c: ProtoOpenAiCompatConfig) -> Self {
        Self {
            id: c.id,
            name: c.name,
            base_url: c.base_url,
            api_key: c.api_key,
            model: c.model,
            // The gRPC message carries no probe verdict — only the Settings
            // GUI writes through this conversion, and `set_open_ai_compat_configs`
            // is responsible for carrying a still-valid cached verdict forward
            // from the config it replaces, rather than this `From` impl
            // inventing one.
            routing_ok: None,
        }
    }
}

impl From<OpenAiCompatConfig> for ProtoOpenAiCompatConfig {
    fn from(c: OpenAiCompatConfig) -> Self {
        Self {
            id: c.id,
            name: c.name,
            base_url: c.base_url,
            api_key: c.api_key,
            model: c.model,
        }
    }
}

/// Session capture settings persisted in daemon.toml under `[capture]`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CaptureConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub sync: bool,
    #[serde(default)]
    pub content: CaptureContentSetting,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sync: false,
            content: CaptureContentSetting::MetadataOnly,
        }
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, Copy, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum CaptureContentSetting {
    #[default]
    MetadataOnly,
    Summary,
    Full,
}

impl From<CaptureContentSetting> for CaptureContentLevel {
    fn from(s: CaptureContentSetting) -> Self {
        match s {
            CaptureContentSetting::MetadataOnly => CaptureContentLevel::MetadataOnly,
            CaptureContentSetting::Summary => CaptureContentLevel::Summary,
            CaptureContentSetting::Full => CaptureContentLevel::Full,
        }
    }
}

impl From<CaptureContentLevel> for CaptureContentSetting {
    fn from(l: CaptureContentLevel) -> Self {
        match l {
            CaptureContentLevel::MetadataOnly => CaptureContentSetting::MetadataOnly,
            CaptureContentLevel::Summary => CaptureContentSetting::Summary,
            CaptureContentLevel::Full => CaptureContentSetting::Full,
        }
    }
}

/// Read capture settings from the config file at the given path. Used by the
/// capture service to decide whether and how to capture a completed session.
pub async fn read_capture_settings(config_path: &std::path::Path) -> anyhow::Result<CaptureConfig> {
    match tokio::fs::read_to_string(config_path).await {
        Ok(contents) => {
            let config: DaemonConfig = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse daemon config: {}", e))?;
            Ok(config.capture)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(CaptureConfig::default()),
        Err(e) => Err(anyhow::anyhow!("failed to read daemon config: {}", e)),
    }
}

/// Read every OpenAI-compatible provider config. Used by `LocalAgentService`
/// to query each configured endpoint for the models it serves.
///
/// A missing config file is not an error — it just means no providers are
/// configured yet.
pub async fn load_openai_compat_configs(
    config_path: &std::path::Path,
) -> anyhow::Result<Vec<OpenAiCompatConfig>> {
    match tokio::fs::read_to_string(config_path).await {
        Ok(contents) => {
            let config: DaemonConfig = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse daemon config: {}", e))?;
            Ok(config.openai_compat.configs)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(anyhow::anyhow!("failed to read daemon config: {}", e)),
    }
}

/// Look up a single OpenAI-compatible provider config by UUID. Used by
/// `LocalAgentService` when loading an `openai-compat:<uuid>` model.
pub async fn find_openai_compat_config(
    config_path: &std::path::Path,
    id: &str,
) -> anyhow::Result<Option<OpenAiCompatConfig>> {
    match tokio::fs::read_to_string(config_path).await {
        Ok(contents) => {
            let config: DaemonConfig = toml::from_str(&contents)
                .map_err(|e| anyhow::anyhow!("failed to parse daemon config: {}", e))?;
            Ok(config
                .openai_compat
                .configs
                .into_iter()
                .find(|c| c.id == id))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(anyhow::anyhow!("failed to read daemon config: {}", e)),
    }
}

/// Persist a routing-probe verdict for one OpenAI-compatible config.
///
/// Called by `LocalAgentService` after a model load runs the routing probe —
/// not through `SettingsServiceImpl`'s RPC lock, since the probe runs outside
/// any RPC. Best-effort: a probe verdict is cheap to re-derive on the next
/// model load, so a lost write under a rare concurrent Settings save is not
/// worth a cross-service lock. No-ops if the config was deleted or its
/// `(base_url, model)` no longer match what was probed — the verdict would
/// no longer describe the config it was measured against.
pub async fn record_routing_probe_verdict(
    config_path: &std::path::Path,
    id: &str,
    base_url: &str,
    model: &str,
    routing_ok: bool,
) -> anyhow::Result<()> {
    let contents = match tokio::fs::read_to_string(config_path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(anyhow::anyhow!("failed to read daemon config: {e}")),
    };
    let mut config: DaemonConfig = toml::from_str(&contents)
        .map_err(|e| anyhow::anyhow!("failed to parse daemon config: {e}"))?;

    let Some(entry) = config.openai_compat.configs.iter_mut().find(|c| c.id == id) else {
        return Ok(());
    };
    if entry.base_url != base_url || entry.model != model {
        return Ok(());
    }
    entry.routing_ok = Some(routing_ok);

    if let Some(parent) = config_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| anyhow::anyhow!("failed to create config directory: {e}"))?;
    }
    let serialized = toml::to_string_pretty(&config)
        .map_err(|e| anyhow::anyhow!("failed to serialize daemon config: {e}"))?;
    tokio::fs::write(config_path, serialized)
        .await
        .map_err(|e| anyhow::anyhow!("failed to write daemon config: {e}"))?;
    Ok(())
}

pub struct SettingsServiceImpl {
    config_path: PathBuf,
    /// Serializes concurrent UpdateCaptureSettings RPCs so read-modify-write
    /// operations on daemon.toml are not interleaved.
    write_lock: Arc<Mutex<()>>,
}

impl SettingsServiceImpl {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            write_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Build with the default path `~/.nodespace/daemon.toml`.
    pub fn with_default_path() -> Result<Self, String> {
        let home = std::env::var("HOME")
            .map_err(|_| "$HOME is unset — cannot locate daemon config".to_string())?;
        let path = PathBuf::from(home).join(".nodespace").join("daemon.toml");
        Ok(Self::new(path))
    }

    async fn read_config(&self) -> Result<DaemonConfig, Status> {
        match tokio::fs::read_to_string(&self.config_path).await {
            Ok(contents) => toml::from_str(&contents)
                .map_err(|e| Status::internal(format!("Failed to parse daemon config: {}", e))),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(DaemonConfig::default()),
            Err(e) => Err(Status::internal(format!(
                "Failed to read daemon config: {}",
                e
            ))),
        }
    }

    async fn write_config(&self, config: &DaemonConfig) -> Result<(), Status> {
        if let Some(parent) = self.config_path.parent() {
            tokio::fs::create_dir_all(parent).await.map_err(|e| {
                Status::internal(format!("Failed to create config directory: {}", e))
            })?;
        }
        let contents = toml::to_string_pretty(config)
            .map_err(|e| Status::internal(format!("Failed to serialize daemon config: {}", e)))?;
        tokio::fs::write(&self.config_path, contents)
            .await
            .map_err(|e| Status::internal(format!("Failed to write daemon config: {}", e)))?;

        // daemon.toml now holds real third-party API keys (openai_compat.configs) —
        // restrict to owner-only so other local accounts on a shared machine can't
        // read them off disk. Unix-only; this is a macOS/Linux desktop app.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            tokio::fs::set_permissions(&self.config_path, perms)
                .await
                .map_err(|e| {
                    Status::internal(format!("Failed to set daemon config permissions: {}", e))
                })?;
        }

        Ok(())
    }

    fn capture_to_response(capture: &CaptureConfig) -> CaptureSettingsResponse {
        CaptureSettingsResponse {
            enabled: capture.enabled,
            sync: capture.sync,
            content: CaptureContentLevel::from(capture.content) as i32,
        }
    }
}

#[tonic::async_trait]
impl GrpcSettingsService for SettingsServiceImpl {
    async fn get_capture_settings(
        &self,
        _request: Request<GetCaptureSettingsRequest>,
    ) -> Result<Response<CaptureSettingsResponse>, Status> {
        let config = self.read_config().await?;
        Ok(Response::new(Self::capture_to_response(&config.capture)))
    }

    async fn update_capture_settings(
        &self,
        request: Request<UpdateCaptureSettingsRequest>,
    ) -> Result<Response<CaptureSettingsResponse>, Status> {
        let req = request.into_inner();
        let _guard = self.write_lock.lock().await;

        let mut config = self.read_config().await?;

        if let Some(enabled) = req.enabled {
            config.capture.enabled = enabled;
        }
        if let Some(sync) = req.sync {
            config.capture.sync = sync;
        }
        if let Some(content_i32) = req.content {
            let level = CaptureContentLevel::try_from(content_i32)
                .unwrap_or(CaptureContentLevel::MetadataOnly);
            config.capture.content = CaptureContentSetting::from(level);
        }

        self.write_config(&config).await?;
        Ok(Response::new(Self::capture_to_response(&config.capture)))
    }

    async fn list_open_ai_compat_configs(
        &self,
        _request: Request<ListOpenAiCompatConfigsRequest>,
    ) -> Result<Response<ListOpenAiCompatConfigsResponse>, Status> {
        let config = self.read_config().await?;
        Ok(Response::new(ListOpenAiCompatConfigsResponse {
            configs: config
                .openai_compat
                .configs
                .into_iter()
                .map(ProtoOpenAiCompatConfig::from)
                .collect(),
        }))
    }

    async fn set_open_ai_compat_configs(
        &self,
        request: Request<SetOpenAiCompatConfigsRequest>,
    ) -> Result<Response<ListOpenAiCompatConfigsResponse>, Status> {
        let req = request.into_inner();
        let _guard = self.write_lock.lock().await;

        let mut config = self.read_config().await?;
        let previous = std::mem::take(&mut config.openai_compat.configs);
        config.openai_compat.configs = req
            .configs
            .into_iter()
            .map(|proto| {
                let mut c = OpenAiCompatConfig::from(proto);
                // The Settings GUI round-trips every config through this RPC on
                // every save (even one editing an unrelated config), which would
                // otherwise erase a cached probe verdict this same id already
                // earned. Carry it forward only when base_url and model — the
                // pair the probe actually measured — are unchanged; either
                // changing is exactly the case the probe must re-run for.
                if let Some(prev) = previous.iter().find(|p| p.id == c.id) {
                    if prev.base_url == c.base_url && prev.model == c.model {
                        c.routing_ok = prev.routing_ok;
                    }
                }
                c
            })
            .collect();

        self.write_config(&config).await?;
        Ok(Response::new(ListOpenAiCompatConfigsResponse {
            configs: config
                .openai_compat
                .configs
                .into_iter()
                .map(ProtoOpenAiCompatConfig::from)
                .collect(),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_impl() -> (SettingsServiceImpl, tempfile::TempDir) {
        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let config_path = tempdir.path().join("daemon.toml");
        (SettingsServiceImpl::new(config_path), tempdir)
    }

    #[tokio::test]
    async fn list_openai_compat_configs_empty_when_no_file() {
        let (svc, _tempdir) = test_impl();
        let resp = svc
            .list_open_ai_compat_configs(Request::new(ListOpenAiCompatConfigsRequest {}))
            .await
            .expect("list should succeed")
            .into_inner();
        assert!(resp.configs.is_empty());
    }

    #[tokio::test]
    async fn set_then_list_openai_compat_configs_roundtrips() {
        let (svc, _tempdir) = test_impl();
        let config = ProtoOpenAiCompatConfig {
            id: "abc-123".to_string(),
            name: "My Endpoint".to_string(),
            base_url: "https://api.example.com/v1".to_string(),
            api_key: "sk-test".to_string(),
            model: "gpt-4o".to_string(),
        };

        let set_resp = svc
            .set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
                configs: vec![config.clone()],
            }))
            .await
            .expect("set should succeed")
            .into_inner();
        assert_eq!(set_resp.configs.len(), 1);

        let list_resp = svc
            .list_open_ai_compat_configs(Request::new(ListOpenAiCompatConfigsRequest {}))
            .await
            .expect("list should succeed")
            .into_inner();
        assert_eq!(list_resp.configs.len(), 1);
        assert_eq!(list_resp.configs[0].id, "abc-123");
        assert_eq!(list_resp.configs[0].name, "My Endpoint");
        assert_eq!(list_resp.configs[0].base_url, "https://api.example.com/v1");
        assert_eq!(list_resp.configs[0].api_key, "sk-test");
    }

    #[tokio::test]
    async fn set_openai_compat_configs_replaces_full_list() {
        let (svc, _tempdir) = test_impl();
        let first = ProtoOpenAiCompatConfig {
            id: "a".to_string(),
            name: "A".to_string(),
            base_url: "https://a.example.com".to_string(),
            api_key: String::new(),
            model: "model-a".to_string(),
        };
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![first],
        }))
        .await
        .expect("first set should succeed");

        let second = ProtoOpenAiCompatConfig {
            id: "b".to_string(),
            name: "B".to_string(),
            base_url: "https://b.example.com".to_string(),
            api_key: String::new(),
            model: "model-b".to_string(),
        };
        let resp = svc
            .set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
                configs: vec![second],
            }))
            .await
            .expect("second set should succeed")
            .into_inner();

        assert_eq!(resp.configs.len(), 1);
        assert_eq!(resp.configs[0].id, "b");
    }

    #[tokio::test]
    async fn find_openai_compat_config_by_id() {
        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        let config = ProtoOpenAiCompatConfig {
            id: "target".to_string(),
            name: "Target".to_string(),
            base_url: "https://target.example.com".to_string(),
            api_key: "key".to_string(),
            model: "target-model".to_string(),
        };
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![config],
        }))
        .await
        .expect("set should succeed");

        let found = find_openai_compat_config(&config_path, "target")
            .await
            .expect("read should succeed");
        assert!(found.is_some());
        assert_eq!(found.unwrap().name, "Target");

        let missing = find_openai_compat_config(&config_path, "nonexistent")
            .await
            .expect("read should succeed");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn find_openai_compat_config_returns_none_when_file_missing() {
        let tempdir = tempfile::TempDir::new().expect("tempdir");
        let config_path = tempdir.path().join("daemon.toml");
        let found = find_openai_compat_config(&config_path, "anything")
            .await
            .expect("missing file should not error");
        assert!(found.is_none());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn write_config_restricts_daemon_toml_to_owner_only() {
        use std::os::unix::fs::PermissionsExt;

        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![ProtoOpenAiCompatConfig {
                id: "a".to_string(),
                name: "A".to_string(),
                base_url: "https://a.example.com".to_string(),
                api_key: "sk-secret".to_string(),
                model: "gpt-4o".to_string(),
            }],
        }))
        .await
        .expect("set should succeed");

        let metadata = tokio::fs::metadata(&config_path)
            .await
            .expect("daemon.toml should exist after write");
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "daemon.toml should be owner-read/write only, got {:o}",
            mode
        );
    }

    fn probe_config(id: &str, base_url: &str, model: &str) -> ProtoOpenAiCompatConfig {
        ProtoOpenAiCompatConfig {
            id: id.to_string(),
            name: "Test Endpoint".to_string(),
            base_url: base_url.to_string(),
            api_key: String::new(),
            model: model.to_string(),
        }
    }

    #[tokio::test]
    async fn record_routing_probe_verdict_persists_and_is_readable() {
        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![probe_config("a", "http://localhost:11434/v1", "mistral:7b")],
        }))
        .await
        .expect("set should succeed");

        record_routing_probe_verdict(
            &config_path,
            "a",
            "http://localhost:11434/v1",
            "mistral:7b",
            false,
        )
        .await
        .expect("record should succeed");

        let found = find_openai_compat_config(&config_path, "a")
            .await
            .expect("read should succeed")
            .expect("config exists");
        assert_eq!(found.routing_ok, Some(false));
    }

    #[tokio::test]
    async fn record_routing_probe_verdict_noops_when_config_deleted() {
        let (_svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        // No config file at all yet.
        record_routing_probe_verdict(&config_path, "ghost", "http://x", "m", false)
            .await
            .expect("noop on missing file must not error");
    }

    #[tokio::test]
    async fn record_routing_probe_verdict_noops_when_base_url_or_model_changed() {
        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![probe_config("a", "http://localhost:11434/v1", "mistral:7b")],
        }))
        .await
        .expect("set should succeed");

        // Probe result for a model the config no longer names — must not be
        // written, or a later load would trust a verdict about a different
        // served model.
        record_routing_probe_verdict(
            &config_path,
            "a",
            "http://localhost:11434/v1",
            "llama3.1:8b",
            true,
        )
        .await
        .expect("stale-target write should not error, just noop");

        let found = find_openai_compat_config(&config_path, "a")
            .await
            .expect("read should succeed")
            .expect("config exists");
        assert_eq!(
            found.routing_ok, None,
            "verdict for a different model must not attach to this config"
        );
    }

    #[tokio::test]
    async fn set_open_ai_compat_configs_carries_forward_verdict_when_endpoint_unchanged() {
        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![probe_config("a", "http://localhost:11434/v1", "mistral:7b")],
        }))
        .await
        .expect("set should succeed");
        record_routing_probe_verdict(
            &config_path,
            "a",
            "http://localhost:11434/v1",
            "mistral:7b",
            false,
        )
        .await
        .expect("record should succeed");

        // Settings GUI saves again — e.g. the user renamed the config — without
        // touching base_url or model.
        let mut renamed = probe_config("a", "http://localhost:11434/v1", "mistral:7b");
        renamed.name = "Renamed".to_string();
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![renamed],
        }))
        .await
        .expect("set should succeed");

        let found = find_openai_compat_config(&config_path, "a")
            .await
            .expect("read should succeed")
            .expect("config exists");
        assert_eq!(
            found.routing_ok,
            Some(false),
            "an unrelated field edit must not erase the cached probe verdict"
        );
    }

    #[tokio::test]
    async fn set_open_ai_compat_configs_drops_verdict_when_model_changes() {
        let (svc, tempdir) = test_impl();
        let config_path = tempdir.path().join("daemon.toml");
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![probe_config("a", "http://localhost:11434/v1", "mistral:7b")],
        }))
        .await
        .expect("set should succeed");
        record_routing_probe_verdict(
            &config_path,
            "a",
            "http://localhost:11434/v1",
            "mistral:7b",
            false,
        )
        .await
        .expect("record should succeed");

        // The user points the same config id at a different served model.
        svc.set_open_ai_compat_configs(Request::new(SetOpenAiCompatConfigsRequest {
            configs: vec![probe_config(
                "a",
                "http://localhost:11434/v1",
                "llama3.1:8b",
            )],
        }))
        .await
        .expect("set should succeed");

        let found = find_openai_compat_config(&config_path, "a")
            .await
            .expect("read should succeed")
            .expect("config exists");
        assert_eq!(
            found.routing_ok, None,
            "a verdict measured against the old model must not carry onto the new one"
        );
    }
}
