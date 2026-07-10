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
}

impl From<ProtoOpenAiCompatConfig> for OpenAiCompatConfig {
    fn from(c: ProtoOpenAiCompatConfig) -> Self {
        Self {
            id: c.id,
            name: c.name,
            base_url: c.base_url,
            api_key: c.api_key,
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
        config.openai_compat.configs = req
            .configs
            .into_iter()
            .map(OpenAiCompatConfig::from)
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
}
