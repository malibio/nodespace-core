//! Registry and lifecycle skeleton for the daemon's local databases (ADR-053:
//! "One Daemon, Multiple Local Databases").
//!
//! One daemon process serves N registered databases behind its single socket.
//! This module owns the persistent registry — the list of known databases plus
//! which one is the default — and the in-memory map of currently-open
//! databases. Each open database's per-database service set ([`DatabaseServices`])
//! is built on demand by [`DatabaseManager::get_or_open`] from the shared
//! [`SharedContext`] and cached here.
//!
//! The registry persists to a dedicated `~/.nodespace/databases.toml`, kept
//! separate from the `SettingsService`-owned `daemon.toml` so the two writers
//! never clobber each other's sections.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use ulid::Ulid;

use super::assembly::{build_database_services, DatabaseServices, SharedContext};

/// Stable identifier for a registered database.
///
/// Backed by a ULID string: lexicographically sortable and monotonic by
/// creation time, so registry order and creation order agree without a
/// separate sort key.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DatabaseId(String);

impl DatabaseId {
    /// Generate a fresh identifier for a newly registered database.
    pub fn generate() -> Self {
        Self(Ulid::new().to_string())
    }

    /// Borrow the identifier as a string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<String> for DatabaseId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for DatabaseId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A single registered database as stored in `databases.toml`.
///
/// Serialized as a `[[databases]]` array-of-tables entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseEntry {
    /// Stable registry identifier (ULID).
    pub id: DatabaseId,
    /// Human-facing label; renamable without touching the file.
    pub name: String,
    /// Absolute path to the database file.
    pub path: PathBuf,
    /// When the entry was registered.
    pub created_at: DateTime<Utc>,
    /// Last time the database was opened, if ever.
    #[serde(default)]
    pub last_opened_at: Option<DateTime<Utc>>,
}

/// Runtime status of a registered database, derived at read time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseStatus {
    /// The database is currently open and serving requests.
    Open,
    /// The database file exists on disk but is not currently open.
    Closed,
    /// The registry entry points at a path with no database file present.
    Missing,
}

/// A registry entry paired with its current runtime status. This is what the
/// `DatabaseService` handler maps onto the proto `DatabaseInfo` response.
#[derive(Debug, Clone)]
pub struct DatabaseListing {
    pub entry: DatabaseEntry,
    pub status: DatabaseStatus,
    pub is_default: bool,
}

/// An immutable snapshot of the registry plus per-database status.
#[derive(Debug, Clone, Default)]
pub struct RegistrySnapshot {
    pub databases: Vec<DatabaseListing>,
    pub default_database: Option<DatabaseId>,
}

/// On-disk representation of `~/.nodespace/databases.toml`.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Registry {
    /// Registered databases, in registration order.
    #[serde(default)]
    pub databases: Vec<DatabaseEntry>,
    /// Identifier of the database that serves routing-header-less requests.
    #[serde(default)]
    pub default_database: Option<DatabaseId>,
}

impl Registry {
    /// Load the registry from `path`. A missing file yields an empty registry
    /// rather than an error — first boot has no registry yet.
    pub async fn load(path: &Path) -> Result<Self> {
        match tokio::fs::read_to_string(path).await {
            Ok(contents) => toml::from_str(&contents)
                .with_context(|| format!("parsing database registry {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => {
                Err(e).with_context(|| format!("reading database registry {}", path.display()))
            }
        }
    }

    /// Persist the registry to `path`, creating the parent directory if needed.
    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("creating registry directory {}", parent.display()))?;
        }
        let contents =
            toml::to_string_pretty(self).context("serializing database registry to TOML")?;
        tokio::fs::write(path, contents)
            .await
            .with_context(|| format!("writing database registry {}", path.display()))
    }

    fn find(&self, id: &DatabaseId) -> Option<&DatabaseEntry> {
        self.databases.iter().find(|e| &e.id == id)
    }
}

/// Owns the database registry and the set of currently-open databases.
///
/// Mutates and persists the registry (create/register/remove/set_default/
/// rename/ensure_default). [`DatabaseManager::get_or_open`] lazily assembles a
/// database's [`DatabaseServices`] from the shared [`SharedContext`] and caches
/// the resulting handle so subsequent requests reuse the same open database.
pub struct DatabaseManager {
    /// Path to `~/.nodespace/databases.toml`.
    registry_path: PathBuf,
    /// The persistent registry, guarded for concurrent mutation.
    registry: RwLock<Registry>,
    /// Databases currently open in this process, keyed by id. Populated lazily
    /// by [`DatabaseManager::get_or_open`]; the shared `Arc` is what request
    /// routing hands to the gRPC handlers.
    open: RwLock<HashMap<DatabaseId, Arc<DatabaseServices>>>,
    /// Process-global build context (PTY manager + embedding model) every
    /// per-database service set is assembled from.
    context: SharedContext,
}

impl DatabaseManager {
    /// Load the manager, reading any existing registry from `registry_path`.
    /// `context` is the process-global build context used to assemble each
    /// database's service set on first open.
    pub async fn load(registry_path: PathBuf, context: SharedContext) -> Result<Self> {
        let registry = Registry::load(&registry_path).await?;
        Ok(Self {
            registry_path,
            registry: RwLock::new(registry),
            open: RwLock::new(HashMap::new()),
            context,
        })
    }

    /// Default registry path `~/.nodespace/databases.toml`.
    pub fn default_registry_path() -> Result<PathBuf> {
        Ok(Self::nodespace_dir()?.join("databases.toml"))
    }

    fn nodespace_dir() -> Result<PathBuf> {
        let home = dirs::home_dir()
            .context("cannot determine the database registry path: home directory is unknown")?;
        Ok(home.join(".nodespace"))
    }

    /// Snapshot every registered database with its current status.
    pub async fn list(&self) -> RegistrySnapshot {
        let registry = self.registry.read().await;
        let open = self.open.read().await;
        let default_database = registry.default_database.clone();
        let databases = registry
            .databases
            .iter()
            .map(|entry| {
                let status = if open.contains_key(&entry.id) {
                    DatabaseStatus::Open
                } else if entry.path.exists() {
                    DatabaseStatus::Closed
                } else {
                    DatabaseStatus::Missing
                };
                let is_default = default_database.as_ref() == Some(&entry.id);
                DatabaseListing {
                    entry: entry.clone(),
                    status,
                    is_default,
                }
            })
            .collect();
        RegistrySnapshot {
            databases,
            default_database,
        }
    }

    /// Register a brand-new database under `name`. When `path` is `None` the
    /// daemon derives a path under its managed database directory. The registry
    /// entry is created and persisted; the database file itself is created
    /// lazily on first open (a follow-on stage), so a freshly created entry
    /// reports [`DatabaseStatus::Missing`] until then.
    pub async fn create(&self, name: String, path: Option<PathBuf>) -> Result<DatabaseEntry> {
        let id = DatabaseId::generate();
        let path = match path {
            Some(path) => path,
            None => Self::nodespace_dir()?
                .join("database")
                .join(format!("{id}.db")),
        };
        self.insert_entry(id, name, path).await
    }

    /// Register an existing database file already present on disk. The name is
    /// derived from the file stem. Registering never creates or moves files.
    pub async fn register(&self, path: PathBuf) -> Result<DatabaseEntry> {
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "database".to_owned());
        let id = DatabaseId::generate();
        self.insert_entry(id, name, path).await
    }

    /// Unregister a database. This only removes the registry entry — it never
    /// deletes the underlying database file. If the removed database was the
    /// default, the default is cleared.
    pub async fn remove(&self, id: &DatabaseId) -> Result<()> {
        {
            let mut registry = self.registry.write().await;
            let before = registry.databases.len();
            registry.databases.retain(|e| &e.id != id);
            if registry.databases.len() == before {
                return Err(anyhow!("no database registered with id {id}"));
            }
            if registry.default_database.as_ref() == Some(id) {
                registry.default_database = None;
            }
            registry.save(&self.registry_path).await?;
        }
        self.open.write().await.remove(id);
        Ok(())
    }

    /// Mark a registered database as the default served for header-less
    /// requests.
    pub async fn set_default(&self, id: &DatabaseId) -> Result<()> {
        let mut registry = self.registry.write().await;
        if registry.find(id).is_none() {
            return Err(anyhow!("no database registered with id {id}"));
        }
        registry.default_database = Some(id.clone());
        registry.save(&self.registry_path).await
    }

    /// Rename the human-facing label of a registered database. Does not touch
    /// the underlying file.
    pub async fn rename(&self, id: &DatabaseId, name: String) -> Result<()> {
        let mut registry = self.registry.write().await;
        let entry = registry
            .databases
            .iter_mut()
            .find(|e| &e.id == id)
            .ok_or_else(|| anyhow!("no database registered with id {id}"))?;
        entry.name = name;
        registry.save(&self.registry_path).await
    }

    /// Ensure a default database is registered, returning its id.
    ///
    /// On first boot the registry is empty; this registers `path` under `name`
    /// and marks it the default so header-less requests always have a database
    /// to route to (ADR-053). Idempotent: if a default is already set it is
    /// returned untouched, and if the registry has entries but no default the
    /// first entry is adopted rather than registering a duplicate. Never
    /// re-adds `path` or overrides an existing default.
    pub async fn ensure_default_registered(
        &self,
        name: String,
        path: PathBuf,
    ) -> Result<DatabaseId> {
        // Fast path: a default is already set.
        if let Some(id) = self.registry.read().await.default_database.clone() {
            return Ok(id);
        }
        // Take the write lock and re-check — a concurrent caller may have
        // registered the default between the read above and this acquire.
        let mut registry = self.registry.write().await;
        if let Some(id) = registry.default_database.clone() {
            return Ok(id);
        }
        // Entries exist but no default → adopt the first as default.
        if let Some(first) = registry.databases.first() {
            let id = first.id.clone();
            registry.default_database = Some(id.clone());
            registry.save(&self.registry_path).await?;
            return Ok(id);
        }
        // Empty registry → register the default database.
        let id = DatabaseId::generate();
        registry.databases.push(DatabaseEntry {
            id: id.clone(),
            name,
            path,
            created_at: Utc::now(),
            last_opened_at: None,
        });
        registry.default_database = Some(id.clone());
        registry.save(&self.registry_path).await?;
        Ok(id)
    }

    /// Resolve the database a request targets from its `x-ns-database-id`
    /// routing header (ADR-053).
    ///
    /// An explicit header selects a registered database; an absent header
    /// routes to the default so existing single-database clients keep working
    /// unchanged. Errors if the header names an unregistered database, or if it
    /// is absent and no default has been set.
    pub async fn resolve_database_id(&self, header: Option<&str>) -> Result<DatabaseId> {
        let registry = self.registry.read().await;
        match header {
            Some(raw) => {
                let id = DatabaseId::from(raw.to_string());
                if registry.find(&id).is_some() {
                    Ok(id)
                } else {
                    Err(anyhow!("no database registered with id {id}"))
                }
            }
            None => registry
                .default_database
                .clone()
                .ok_or_else(|| anyhow!("no default database is set")),
        }
    }

    /// Return the per-database service set for `id`, assembling (opening) it
    /// lazily on first request and caching it for reuse.
    ///
    /// A cached handle is returned immediately; otherwise the registry entry's
    /// path is resolved and its [`DatabaseServices`] are built from the shared
    /// [`SharedContext`], cached, and returned. Errors if `id` is not
    /// registered. Concurrent first-opens of the same id converge on a single
    /// cached handle: the assembly runs outside the `open` lock (so opens of
    /// distinct databases don't serialize), then a re-check under the write
    /// lock keeps whichever handle landed first.
    pub async fn get_or_open(&self, id: &DatabaseId) -> Result<Arc<DatabaseServices>> {
        // Fast path: already open.
        if let Some(services) = self.open.read().await.get(id).cloned() {
            return Ok(services);
        }

        // Resolve the registry entry's on-disk path before the (slow) build.
        let path = {
            let registry = self.registry.read().await;
            registry
                .find(id)
                .map(|entry| entry.path.clone())
                .ok_or_else(|| anyhow!("no database registered with id {id}"))?
        };

        // Assemble the service set (opens SQLite, seeds schema) without holding
        // the `open` lock. The per-database embedding-wiring task is detached,
        // matching the boot path. `last_opened_at` bookkeeping is deferred.
        let (services, _embed_task) = build_database_services(&path, &self.context)
            .await
            .with_context(|| format!("opening database {id}"))?;
        let services = Arc::new(services);

        // Cache under the write lock; if a concurrent caller opened it first,
        // drop ours and reuse theirs so every request shares one open handle.
        let mut open = self.open.write().await;
        if let Some(existing) = open.get(id).cloned() {
            return Ok(existing);
        }
        open.insert(id.clone(), services.clone());
        Ok(services)
    }

    /// Push a new entry into the registry and persist it. The first registered
    /// database becomes the default.
    async fn insert_entry(
        &self,
        id: DatabaseId,
        name: String,
        path: PathBuf,
    ) -> Result<DatabaseEntry> {
        let entry = DatabaseEntry {
            id: id.clone(),
            name,
            path,
            created_at: Utc::now(),
            last_opened_at: None,
        };
        let mut registry = self.registry.write().await;
        registry.databases.push(entry.clone());
        if registry.default_database.is_none() {
            registry.default_database = Some(id);
        }
        registry.save(&self.registry_path).await?;
        Ok(entry)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_agent::pty::PtySessionManager;
    use nodespace_nlp_engine::EmbeddingService;
    use tokio::sync::watch;

    /// A model-less build context: `has_model = false` makes
    /// [`build_database_services`] skip all embedding wiring, so the watch
    /// channel is never consumed and dropping the sender is harmless.
    fn test_context() -> SharedContext {
        let (_tx, model) = watch::channel::<Option<Arc<EmbeddingService>>>(None);
        SharedContext {
            pty_manager: Arc::new(PtySessionManager::new()),
            model,
            has_model: false,
        }
    }

    async fn temp_manager() -> (DatabaseManager, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("databases.toml");
        (
            DatabaseManager::load(path, test_context()).await.unwrap(),
            dir,
        )
    }

    #[tokio::test]
    async fn ensure_default_registers_on_empty_and_is_idempotent() {
        let (mgr, _dir) = temp_manager().await;
        let id = mgr
            .ensure_default_registered("Default".into(), PathBuf::from("/tmp/ns.db"))
            .await
            .unwrap();

        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.default_database.as_ref(), Some(&id));
        assert!(snap.databases[0].is_default);

        // A second call is a no-op: same id, no new entry, name not overridden.
        let again = mgr
            .ensure_default_registered("Other".into(), PathBuf::from("/tmp/other.db"))
            .await
            .unwrap();
        assert_eq!(again, id);
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].entry.name, "Default");
    }

    #[tokio::test]
    async fn resolve_database_id_routes_header_and_default() {
        let (mgr, _dir) = temp_manager().await;
        let default = mgr
            .ensure_default_registered("Default".into(), PathBuf::from("/tmp/ns.db"))
            .await
            .unwrap();

        // Absent header routes to the default.
        assert_eq!(mgr.resolve_database_id(None).await.unwrap(), default);
        // An explicit registered id routes to that id.
        assert_eq!(
            mgr.resolve_database_id(Some(default.as_str()))
                .await
                .unwrap(),
            default
        );

        // Registering a second database does not change routing of header-less
        // requests, and the second id routes explicitly.
        let second = mgr.register(PathBuf::from("/tmp/second.db")).await.unwrap();
        assert_eq!(
            mgr.resolve_database_id(Some(second.id.as_str()))
                .await
                .unwrap(),
            second.id
        );
        assert_eq!(mgr.resolve_database_id(None).await.unwrap(), default);

        // An unregistered id is an error, not a silent fallback to the default.
        assert!(mgr
            .resolve_database_id(Some("ZZZ-NOT-REGISTERED"))
            .await
            .is_err());
    }

    #[tokio::test]
    async fn resolve_without_default_errors() {
        let (mgr, _dir) = temp_manager().await;
        assert!(mgr.resolve_database_id(None).await.is_err());
    }

    #[tokio::test]
    async fn get_or_open_builds_and_caches_the_service_set() {
        let (mgr, dir) = temp_manager().await;
        let db_path = dir.path().join("default.db");
        let id = mgr
            .ensure_default_registered("Default".into(), db_path)
            .await
            .unwrap();

        // First open assembles the service set; the database now reports Open.
        let first = mgr.get_or_open(&id).await.unwrap();
        assert!(matches!(
            mgr.list().await.databases[0].status,
            DatabaseStatus::Open
        ));

        // A second open returns the very same cached handle, not a rebuild.
        let second = mgr.get_or_open(&id).await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        // An unregistered id is an error, not a silent fallback to the default.
        assert!(mgr
            .get_or_open(&DatabaseId::from("ZZZ-NOT-REGISTERED".to_string()))
            .await
            .is_err());
    }
}
