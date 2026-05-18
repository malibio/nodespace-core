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
    settings_service_server::SettingsService as GrpcSettingsService, DaemonConfigResponse,
    GetDaemonConfigRequest, UpdateDaemonConfigRequest,
};

const DEFAULT_GRPC_ADDRESS: &str = "[::1]:50051";

/// On-disk representation of `~/.nodespace/daemon.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
struct DaemonConfig {
    active_database_path: Option<String>,
    grpc_address: Option<String>,
}

pub struct SettingsServiceImpl {
    config_path: PathBuf,
    /// Serializes concurrent UpdateDaemonConfig RPCs so read-modify-write
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
            .map_err(|e| Status::internal(format!("Failed to write daemon config: {}", e)))
    }

    fn config_to_response(config: &DaemonConfig) -> DaemonConfigResponse {
        DaemonConfigResponse {
            active_database_path: config.active_database_path.clone().unwrap_or_default(),
            grpc_address: config
                .grpc_address
                .clone()
                .unwrap_or_else(|| DEFAULT_GRPC_ADDRESS.to_string()),
        }
    }
}

#[tonic::async_trait]
impl GrpcSettingsService for SettingsServiceImpl {
    async fn get_daemon_config(
        &self,
        _request: Request<GetDaemonConfigRequest>,
    ) -> Result<Response<DaemonConfigResponse>, Status> {
        let config = self.read_config().await?;
        Ok(Response::new(Self::config_to_response(&config)))
    }

    async fn update_daemon_config(
        &self,
        request: Request<UpdateDaemonConfigRequest>,
    ) -> Result<Response<DaemonConfigResponse>, Status> {
        let req = request.into_inner();
        let _guard = self.write_lock.lock().await;

        let mut config = self.read_config().await?;

        if !req.active_database_path.is_empty() {
            config.active_database_path = Some(req.active_database_path);
        }
        if !req.grpc_address.is_empty() {
            config.grpc_address = Some(req.grpc_address);
        }

        self.write_config(&config).await?;
        Ok(Response::new(Self::config_to_response(&config)))
    }
}
