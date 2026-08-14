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
use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use chrono::{DateTime, Utc};
use nodespace_core::models::EmbeddingConfig;
use serde::{Deserialize, Serialize};
use tokio::sync::{watch, RwLock, RwLockWriteGuard};
use ulid::Ulid;

use super::assembly::{build_database_services, DatabaseServices, SharedContext};

/// How often the idle reaper scans open databases for eviction (ADR-053:
/// per-database compute scoping).
const IDLE_CHECK_INTERVAL: Duration = Duration::from_secs(60);

/// Default idle window before a non-default, non-active database is evicted.
/// Overridable via `NODESPACE_DB_IDLE_SECS`.
const DEFAULT_IDLE_WINDOW: Duration = Duration::from_secs(5 * 60);

/// Resolve the idle-eviction window, honoring `NODESPACE_DB_IDLE_SECS` (seconds)
/// when set to a valid non-zero value; otherwise the built-in default.
fn idle_window() -> Duration {
    match std::env::var("NODESPACE_DB_IDLE_SECS") {
        Ok(raw) => match raw.parse::<u64>() {
            Ok(secs) if secs > 0 => Duration::from_secs(secs),
            _ => DEFAULT_IDLE_WINDOW,
        },
        Err(_) => DEFAULT_IDLE_WINDOW,
    }
}

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
    /// The cloud tenant schema this database binds to (ADR-053 per-database cloud
    /// sync), mirrored from the database's DatabaseSettingsNode so the bound
    /// tenant can be shown before the database is opened. Empty until bound.
    #[serde(default)]
    pub bound_tenant_schema: Option<String>,
    /// The default collection id within the bound tenant, mirrored alongside
    /// `bound_tenant_schema`. Empty until bound.
    #[serde(default)]
    pub bound_tenant_collection: Option<String>,
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
///
/// `Clone` backs [`DatabaseManager::mutate_and_save`]'s "compute a candidate,
/// persist it, then swap it in" shape: cloning is cheap (a `Vec<DatabaseEntry>`
/// plus an `Option<DatabaseId>`), and every mutator computes its next value
/// from a clone of the current one rather than mutating the live registry.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
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
    /// Last time each open database served a routed request (ADR-053:
    /// per-database compute scoping). Drives idle eviction. `Instant` is
    /// monotonic, so this is unaffected by wall-clock changes.
    last_activity: RwLock<HashMap<DatabaseId, Instant>>,
    /// Process-global build context (PTY manager + embedding model) every
    /// per-database service set is assembled from.
    context: SharedContext,
    /// Bumped whenever the registry or the open set changes, so observers can
    /// re-read [`DatabaseManager::list`] instead of polling it.
    ///
    /// A counter rather than the snapshot itself: building a snapshot takes the
    /// same locks the mutation is holding, so publishing one from inside a
    /// mutation would invite a deadlock. Observers re-read after waking, which
    /// also means a burst of changes collapses into one refresh — `watch` keeps
    /// only the latest value.
    change_tx: watch::Sender<u64>,
    /// Test-only hook: when set, [`Self::mutate_and_save`] parks at this gate
    /// right before persisting, letting cancellation-safety tests suspend a
    /// mutation mid-flight deterministically (see [`SaveGate`]). This field
    /// does not exist at all in a non-test build.
    #[cfg(test)]
    save_gate: std::sync::Mutex<Option<SaveGate>>,
}

/// Test-only synchronization point injected into
/// [`DatabaseManager::mutate_and_save`] so a test can deterministically
/// suspend a mutation mid-persist and then drop (via `JoinHandle::abort`) the
/// future driving it — reproducing the exact "future dropped while parked at
/// the save await" scenario cancellation-safety must survive, without relying
/// on real filesystem/timing races.
#[cfg(test)]
#[derive(Clone, Default)]
struct SaveGate {
    /// Signaled once a mutation has reached (and is now parked inside) the
    /// gate, so the test knows it is safe to abort the driving task.
    entered: Arc<tokio::sync::Notify>,
    /// The gate waits on this before letting the real save proceed. A test
    /// that never signals it — and instead aborts the driving task — is how
    /// cancellation mid-persist is reproduced.
    release: Arc<tokio::sync::Notify>,
}

/// True if `path` lives under a system temporary directory. macOS purges
/// `$TMPDIR` (`/var/folders/**`) and `/tmp` periodically, so any database file
/// stored there is doomed. Used to catch a registry whose default database was
/// seeded with a throwaway path by a temp-DB run (ADR-053).
fn is_under_system_temp(path: &Path) -> bool {
    // `std::env::temp_dir()` is this process's `$TMPDIR`; also cover the
    // well-known temp roots directly (and their macOS `/private` twins) so the
    // check holds regardless of the daemon's own `$TMPDIR`.
    let mut roots: Vec<PathBuf> = vec![std::env::temp_dir()];
    for extra in [
        "/tmp",
        "/private/tmp",
        "/var/folders",
        "/private/var/folders",
    ] {
        roots.push(PathBuf::from(extra));
    }
    roots.iter().any(|root| path.starts_with(root))
}

/// Decide whether the registry's default database is doomed: its file lives
/// under a system temp directory while the registry itself does not. A registry
/// that itself lives under a temp dir is an intentionally isolated test/dev
/// environment, where a temp default is expected and must be left alone.
fn default_is_doomed(registry_path: &Path, default_path: &Path) -> bool {
    !is_under_system_temp(registry_path) && is_under_system_temp(default_path)
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
            last_activity: RwLock::new(HashMap::new()),
            context,
            change_tx: watch::channel(0).0,
            #[cfg(test)]
            save_gate: std::sync::Mutex::new(None),
        })
    }

    /// Observe registry/open-set changes. Wake, then call
    /// [`DatabaseManager::list`] for the current state.
    ///
    /// Used by the tray's Databases submenu, which would otherwise keep showing
    /// the registry as it was when the daemon booted.
    pub fn subscribe_changes(&self) -> watch::Receiver<u64> {
        self.change_tx.subscribe()
    }

    /// Signal that the registry or the open set changed.
    ///
    /// Deliberately takes no locks: callers invoke it *after* releasing theirs,
    /// so a woken observer calling `list()` cannot deadlock against the mutation
    /// that woke it.
    fn notify_changed(&self) {
        self.change_tx.send_modify(|n| *n = n.wrapping_add(1));
    }

    /// Arm the test-only [`SaveGate`] so the next [`Self::mutate_and_save`]
    /// parks right before persisting instead of proceeding straight to disk.
    #[cfg(test)]
    fn set_save_gate(&self, gate: SaveGate) {
        *self.save_gate.lock().unwrap() = Some(gate);
    }

    /// Disarm the test-only [`SaveGate`], restoring normal (non-suspending)
    /// persist behavior.
    #[cfg(test)]
    fn clear_save_gate(&self) {
        *self.save_gate.lock().unwrap() = None;
    }

    /// Default registry path `<nodespace_dir>/databases.toml`.
    ///
    /// Resolved through [`crate::nodespace_dir`] so it follows `NODESPACE_HOME`
    /// in lockstep with the database path — redirecting one without the other is
    /// exactly what let a temp-DB test run poison the real user's registry
    /// (ADR-053).
    pub fn default_registry_path() -> Result<PathBuf> {
        Ok(crate::nodespace_dir()?.join("databases.toml"))
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

    /// Create a brand-new database under `name`. When `path` is `None` the
    /// daemon derives a path under its managed database directory.
    ///
    /// The database file is created and opened BEFORE the registry entry is
    /// persisted, so the registry never advertises a database whose file does
    /// not exist. The open handle is cached, so a freshly created database
    /// reports [`DatabaseStatus::Open`] and serves routed requests immediately.
    /// If registration fails after the file was created, the creation is rolled
    /// back — the handle is closed and the just-created file removed — leaving
    /// the daemon exactly as it was before the call.
    pub async fn create(&self, name: String, path: Option<PathBuf>) -> Result<DatabaseEntry> {
        let id = DatabaseId::generate();
        let path = match path {
            Some(path) => path,
            None => crate::nodespace_dir()?
                .join("database")
                .join(format!("{id}.db")),
        };
        // Creating on top of an existing file would silently adopt (and share)
        // foreign data; `register` is the path for existing database files.
        // This check is also what makes the rollback below safe: any file at
        // `path` afterwards is one this call created.
        if path.exists() {
            return Err(anyhow!(
                "a file already exists at {}; use register to add an existing database",
                path.display()
            ));
        }

        // Create and open the database (file, schema, service set) first.
        let (services, _embed_task) = build_database_services(&path, &self.context, id.as_str())
            .await
            .with_context(|| format!("creating database file {}", path.display()))?;

        // Cache the open handle before the entry becomes resolvable: no request
        // can route to the id until `insert_entry` persists it, so the first
        // routed request reuses this handle instead of racing a second open.
        self.open
            .write()
            .await
            .insert(id.clone(), Arc::new(services));
        self.touch(&id).await;

        match self.insert_entry(id.clone(), name, path.clone()).await {
            Ok(entry) => Ok(entry),
            Err(e) => {
                // Registration failed → roll back the creation: drop the open
                // handle and remove the file this call just created, so a
                // failed create leaves no half-registered state behind.
                self.close(&id).await;
                remove_database_files(&path).await;
                Err(e)
            }
        }
    }

    /// Register an existing database file already present on disk. The name is
    /// derived from the file stem. Registering never creates or moves files, so
    /// the file must already exist — a registry entry pointing at nothing would
    /// report [`DatabaseStatus::Missing`] forever. Use
    /// [`DatabaseManager::create`] for a new database.
    pub async fn register(&self, path: PathBuf) -> Result<DatabaseEntry> {
        if !path.exists() {
            return Err(anyhow!(
                "no database file exists at {}; use create to make a new database",
                path.display()
            ));
        }
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_owned)
            .unwrap_or_else(|| "database".to_owned());
        let id = DatabaseId::generate();
        self.insert_entry(id, name, path).await
    }

    /// Compute a candidate registry, persist it, and only then splice it into
    /// the live `RwLock` — with no `.await` between the save resolving and
    /// the swap, so cancellation of the calling future can only ever observe
    /// "nothing happened" or "fully happened," never a half-applied mutation.
    ///
    /// `mutate` receives the *current* registry and returns the candidate
    /// next value; it does not touch `registry` itself. That is what makes
    /// this cancellation-safe where the previous `persist_or_rollback` design
    /// was not: that version mutated the live registry in the caller *before*
    /// entering the save `await`, so a future dropped while parked there left
    /// the mutation applied with neither a restore nor `notify_changed` ever
    /// having run (a client disconnect, a `tonic` timeout, a `select!`, or a
    /// shutdown signal mid-mutation). Here, nothing observable changes until
    /// `candidate.save(..)` has already resolved `Ok` — at which point the
    /// remaining code (the assignment, the notify) runs synchronously to
    /// completion with no further await point, so it cannot be interrupted:
    /// Rust futures can only be dropped while parked at an `.await`, not
    /// mid-poll of synchronous code. If the future is dropped while still
    /// parked at the save, `registry` (still holding the untouched pre-call
    /// value) is simply dropped via its own `Drop`, releasing the write lock
    /// with the live registry exactly as it was.
    ///
    /// Every registry mutator below funnels through this one helper so that
    /// property holds everywhere, not just in whichever mutator happened to
    /// get it right — which is exactly how `set_default`/`set_bound_tenant`
    /// first shipped without it.
    async fn mutate_and_save(
        &self,
        mut registry: RwLockWriteGuard<'_, Registry>,
        mutate: impl FnOnce(&Registry) -> Registry,
    ) -> Result<()> {
        let candidate = mutate(&registry);
        #[cfg(test)]
        {
            // The `MutexGuard` must not live across the `.await` below (it
            // would make this whole future non-`Send`), so the lock is taken
            // and released in this separate statement, before the gate is
            // ever awaited.
            let gate = self.save_gate.lock().unwrap().clone();
            if let Some(gate) = gate {
                gate.entered.notify_one();
                gate.release.notified().await;
            }
        }
        candidate.save(&self.registry_path).await?;
        *registry = candidate;
        drop(registry);
        self.notify_changed();
        Ok(())
    }

    /// Unregister a database. This only removes the registry entry — it never
    /// deletes the underlying database file. If the removed database was the
    /// default, the default is cleared. Persists via
    /// [`Self::mutate_and_save`], which computes and saves the post-removal
    /// registry before it ever becomes the live one, so a failed (or
    /// cancelled) save leaves `list()` reporting the database as still
    /// present, never gone when it is still on disk.
    ///
    /// Removes at most one entry, by `id`'s first match — consistent with
    /// every other lookup in this file (`rename`, `set_default`,
    /// `set_bound_tenant`, `get_or_open`, `resolve_database_id`, `Registry::find`
    /// all resolve `id` to a single entry), and with [`DatabaseId`]'s own
    /// contract as a unique key. A registry file that was hand-edited into
    /// having duplicate ids is already in an unsupported state; leaving a
    /// second, id-colliding entry in place after removing the first is no
    /// worse than every other method here already implicitly assuming
    /// uniqueness.
    pub async fn remove(&self, id: &DatabaseId) -> Result<()> {
        let registry = self.registry.write().await;
        if registry.find(id).is_none() {
            return Err(anyhow!("no database registered with id {id}"));
        }
        self.mutate_and_save(registry, |registry| {
            let mut next = registry.clone();
            if let Some(index) = next.databases.iter().position(|e| &e.id == id) {
                next.databases.remove(index);
            }
            if next.default_database.as_ref() == Some(id) {
                next.default_database = None;
            }
            next
        })
        .await?;
        // Tear down the database's compute (processor + event watcher) as well as
        // its registry entry — unregistering must not leave a detached watcher
        // running (ADR-053: per-database compute scoping). Uses the no-notify
        // variant: `mutate_and_save` already notified observers of the
        // registry change above (covering the case where the removed database
        // was never opened, so the ordinary `close` below would be a no-op),
        // and calling the public `close` here too would double-notify for a
        // database that happened to be open at removal time.
        self.close_without_notify(id).await;
        Ok(())
    }

    /// Mark a registered database as the default served for header-less
    /// requests. Persists via [`Self::mutate_and_save`]: the new default is
    /// only computed as a candidate and saved before it ever becomes the live
    /// default, so a failed (or cancelled) save leaves the previous default
    /// in place.
    pub async fn set_default(&self, id: &DatabaseId) -> Result<()> {
        let registry = self.registry.write().await;
        if registry.find(id).is_none() {
            return Err(anyhow!("no database registered with id {id}"));
        }
        self.mutate_and_save(registry, |registry| {
            let mut next = registry.clone();
            next.default_database = Some(id.clone());
            next
        })
        .await
    }

    /// Rename the human-facing label of a registered database. Does not touch
    /// the underlying file. Persists via [`Self::mutate_and_save`]: the
    /// renamed candidate is saved before it ever becomes the live registry,
    /// so a failed (or cancelled) save leaves the previous name in place.
    pub async fn rename(&self, id: &DatabaseId, name: String) -> Result<()> {
        let registry = self.registry.write().await;
        if registry.find(id).is_none() {
            return Err(anyhow!("no database registered with id {id}"));
        }
        self.mutate_and_save(registry, |registry| {
            let mut next = registry.clone();
            if let Some(entry) = next.databases.iter_mut().find(|e| &e.id == id) {
                entry.name = name;
            }
            next
        })
        .await
    }

    /// Mirror a database's bound cloud tenant into the registry (ADR-053
    /// per-database cloud sync) so the binding can be shown before the database
    /// is opened. Pass `None` to clear it on unbind. The authoritative record is
    /// the database's DatabaseSettingsNode; this registry field is a display
    /// mirror kept in step with it. Persists via [`Self::mutate_and_save`]:
    /// the new binding is saved before it ever becomes the live registry, so
    /// a failed (or cancelled) save leaves the previous binding in place.
    pub async fn set_bound_tenant(
        &self,
        id: &DatabaseId,
        schema: Option<String>,
        collection: Option<String>,
    ) -> Result<()> {
        let registry = self.registry.write().await;
        if registry.find(id).is_none() {
            return Err(anyhow!("no database registered with id {id}"));
        }
        self.mutate_and_save(registry, |registry| {
            let mut next = registry.clone();
            if let Some(entry) = next.databases.iter_mut().find(|e| &e.id == id) {
                entry.bound_tenant_schema = schema;
                entry.bound_tenant_collection = collection;
            }
            next
        })
        .await
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
        let registry = self.registry.write().await;
        if let Some(id) = registry.default_database.clone() {
            return Ok(id);
        }
        // Entries exist but no default → adopt the first as default.
        if let Some(first) = registry.databases.first() {
            let id = first.id.clone();
            self.mutate_and_save(registry, {
                let id = id.clone();
                move |registry| {
                    let mut next = registry.clone();
                    next.default_database = Some(id.clone());
                    next
                }
            })
            .await?;
            return Ok(id);
        }
        // Empty registry → register the default database.
        let id = DatabaseId::generate();
        self.mutate_and_save(registry, {
            let id = id.clone();
            move |registry| {
                let mut next = registry.clone();
                next.databases.push(DatabaseEntry {
                    id: id.clone(),
                    name,
                    path,
                    created_at: Utc::now(),
                    last_opened_at: None,
                    bound_tenant_schema: None,
                    bound_tenant_collection: None,
                });
                next.default_database = Some(id.clone());
                next
            }
        })
        .await?;
        Ok(id)
    }

    /// The on-disk path of the default database, if a default is set.
    ///
    /// This is the path the daemon actually serves for header-less requests —
    /// resolved from the registry, not the `NODESPACED_DB_PATH`/default the
    /// caller passed at boot — so callers can log the truth rather than a value
    /// the registry may override.
    pub async fn default_database_path(&self) -> Option<PathBuf> {
        let registry = self.registry.read().await;
        let id = registry.default_database.as_ref()?;
        registry.find(id).map(|entry| entry.path.clone())
    }

    /// Guard against silent data loss (ADR-053): if the registry's default
    /// database points under a system temp directory — which the OS periodically
    /// purges — replace it with a fresh default at `standard_path` so the daemon
    /// never serves a disappearing database as the user's "Default".
    ///
    /// A no-op when there is no default, when the default already lives at a
    /// durable path, or when the registry itself lives under a temp directory
    /// (an intentionally isolated test/dev environment). Returns the new default
    /// id when a repair occurred.
    pub async fn repair_doomed_default(&self, standard_path: &Path) -> Result<Option<DatabaseId>> {
        let doomed = {
            let registry = self.registry.read().await;
            match registry
                .default_database
                .as_ref()
                .and_then(|id| registry.find(id))
            {
                Some(entry) if default_is_doomed(&self.registry_path, &entry.path) => {
                    entry.path.clone()
                }
                _ => return Ok(None),
            }
        };
        let new_id = self.set_standard_default(standard_path).await?;
        tracing::warn!(
            doomed_path = %doomed.display(),
            repaired_to = %standard_path.display(),
            "registry default database pointed under a system temp directory (the OS purges these); re-pointed the default to the standard location to prevent silent data loss"
        );
        Ok(Some(new_id))
    }

    /// Drop the current default entry (if any) and register a fresh "Default"
    /// at `standard_path`, marking it the default. Persists via
    /// [`Self::mutate_and_save`]: the candidate (doomed entry removed, fresh
    /// one added) is saved before it ever becomes the live registry, so a
    /// failed (or cancelled) save leaves neither the doomed entry gone nor
    /// the new one half-registered. Returns the new id. The doomed file
    /// itself is never touched — only the registry entry is replaced. Runs on
    /// every daemon boot via [`Self::repair_doomed_default`], so a save
    /// failure here is not a rare edge path.
    async fn set_standard_default(&self, standard_path: &Path) -> Result<DatabaseId> {
        let registry = self.registry.write().await;
        let id = DatabaseId::generate();
        let standard_path = standard_path.to_path_buf();
        self.mutate_and_save(registry, {
            let id = id.clone();
            move |registry| {
                let mut next = registry.clone();
                // `id`s are unique (see `DatabaseId`'s docs), so at most one
                // entry matches the previous default.
                if let Some(prev) = next.default_database.take() {
                    if let Some(index) = next.databases.iter().position(|e| e.id == prev) {
                        next.databases.remove(index);
                    }
                }
                next.databases.push(DatabaseEntry {
                    id: id.clone(),
                    name: "Default".to_string(),
                    path: standard_path.clone(),
                    created_at: Utc::now(),
                    last_opened_at: None,
                    bound_tenant_schema: None,
                    bound_tenant_collection: None,
                });
                next.default_database = Some(id.clone());
                next
            }
        })
        .await?;
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
            self.touch(id).await;
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
        let (services, _embed_task) = build_database_services(&path, &self.context, id.as_str())
            .await
            .with_context(|| format!("opening database {id}"))?;
        let services = Arc::new(services);

        // Cache under the write lock; if a concurrent caller opened it first,
        // drop ours and reuse theirs so every request shares one open handle.
        let mut open = self.open.write().await;
        if let Some(existing) = open.get(id).cloned() {
            drop(open);
            self.touch(id).await;
            return Ok(existing);
        }
        open.insert(id.clone(), services.clone());
        drop(open);
        self.touch(id).await;
        // The open set changed: observers showing an open/closed marker need this.
        self.notify_changed();
        Ok(services)
    }

    /// Record that `id` just served a request, resetting its idle timer
    /// (ADR-053: per-database compute scoping).
    async fn touch(&self, id: &DatabaseId) {
        self.last_activity
            .write()
            .await
            .insert(id.clone(), Instant::now());
    }

    /// Close one open database, dropping only that database's compute — its
    /// `EmbeddingProcessor` and event watcher (ADR-053: per-database compute
    /// scoping). Returns `true` if the database was open.
    ///
    /// This never touches the process-global embedding model: the shared NLP
    /// engine and its GPU context stay up for the other databases. Releasing the
    /// GPU context is one-way and belongs solely to daemon shutdown. Callers must
    /// not close the default or the active database.
    pub async fn close(&self, id: &DatabaseId) -> bool {
        let closed = self.close_without_notify(id).await;
        if closed {
            self.notify_changed();
        }
        closed
    }

    /// Core of [`Self::close`] without the notify. `remove` uses this
    /// directly: it already notifies once (unconditionally, via
    /// `mutate_and_save`) for the registry change, and would otherwise
    /// double-notify — once for the registry, once more here — when the
    /// removed database happened to be open.
    async fn close_without_notify(&self, id: &DatabaseId) -> bool {
        let services = self.open.write().await.remove(id);
        self.last_activity.write().await.remove(id);
        let Some(services) = services else {
            return false;
        };
        // Stop the per-database ai-chat event watcher.
        services.local_agent.shutdown();
        // Drop only this database's embedding processor (stops its background
        // task on drop). The shared model is left untouched.
        if let Some(ready) = services.embedding_state.write().await.take() {
            drop(ready.processor);
        }
        true
    }

    /// Close every open database (ADR-053: per-database compute scoping),
    /// dropping each one's processor and event watcher. Called on daemon
    /// shutdown before the process-global GPU context is released exactly once.
    pub async fn shutdown_all(&self) {
        let mut open = self.open.write().await;
        for services in open.values() {
            services.local_agent.shutdown();
            if let Some(ready) = services.embedding_state.write().await.take() {
                drop(ready.processor);
            }
        }
        open.clear();
        self.last_activity.write().await.clear();
    }

    /// Spawn the background idle reaper (ADR-053: per-database compute scoping).
    ///
    /// Every [`IDLE_CHECK_INTERVAL`] it evicts each open database that is all of:
    /// not the default, not the active database, idle beyond the configured
    /// window, and not mid-drain on embeddings. Evicted databases reopen
    /// transparently on their next request via [`DatabaseManager::get_or_open`].
    /// The default and active databases are never evicted, so the single-database
    /// community path is unaffected.
    pub fn spawn_idle_reaper(self: &Arc<Self>) {
        let this = Arc::clone(self);
        let window = idle_window();
        tokio::spawn(async move {
            let mut ticker = tokio::time::interval(IDLE_CHECK_INTERVAL);
            // Skip the immediate first tick so nothing is evicted at boot.
            ticker.tick().await;
            loop {
                ticker.tick().await;
                this.evict_idle_databases(window).await;
            }
        });
    }

    /// Evict every open database that has been idle longer than `window` and is
    /// neither the default, the active database, nor mid-drain on embeddings
    /// (ADR-053: per-database compute scoping).
    pub async fn evict_idle_databases(&self, window: Duration) {
        let default_id = self.registry.read().await.default_database.clone();
        let active_id = self.context.scheduler.active_id();
        let now = Instant::now();

        // Collect candidates under the read locks, then close them afterward so
        // we never hold `open` while awaiting the per-database writes in `close`.
        let candidates: Vec<(DatabaseId, Arc<DatabaseServices>)> = {
            let open = self.open.read().await;
            let activity = self.last_activity.read().await;
            open.iter()
                .filter(|(id, _)| default_id.as_ref() != Some(*id))
                .filter(|(id, _)| active_id.as_deref() != Some(id.as_str()))
                .filter(|(id, _)| {
                    // A database with no recorded activity has only just been
                    // inserted into `open` (the touch lands a moment later) — treat
                    // it as freshly used, never as idle, so it can't be evicted in
                    // that window.
                    activity
                        .get(*id)
                        .map(|seen| now.duration_since(*seen) > window)
                        .unwrap_or(false)
                })
                .map(|(id, services)| (id.clone(), services.clone()))
                .collect()
        };

        for (id, services) in candidates {
            // Never evict a database with embedding work still queued — its
            // processor is mid-drain.
            if has_pending_embeddings(&services).await {
                continue;
            }
            if self.close(&id).await {
                tracing::info!(
                    database_id = %id,
                    "Evicted idle database (ADR-053 per-database compute scoping)"
                );
            }
        }
    }

    /// Push a new entry into the registry and persist it. The first registered
    /// database becomes the default. Persists via [`Self::mutate_and_save`]:
    /// the candidate (with the new entry appended) is saved before it ever
    /// becomes the live registry, so a failed (or cancelled) save leaves the
    /// registry map exactly as it was, never drifting from the file on disk.
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
            bound_tenant_schema: None,
            bound_tenant_collection: None,
        };
        let registry = self.registry.write().await;
        self.mutate_and_save(registry, {
            let entry = entry.clone();
            move |registry| {
                let mut next = registry.clone();
                next.databases.push(entry.clone());
                if next.default_database.is_none() {
                    next.default_database = Some(id.clone());
                }
                next
            }
        })
        .await?;
        Ok(entry)
    }
}

/// Best-effort removal of a database file and its SQLite sidecars (`-wal`,
/// `-shm`) while rolling back a failed create. Failures are logged rather than
/// returned — the caller is already unwinding a more meaningful error.
async fn remove_database_files(path: &Path) {
    let mut candidates = vec![path.to_path_buf()];
    for suffix in ["-wal", "-shm"] {
        let mut file = path.as_os_str().to_owned();
        file.push(suffix);
        candidates.push(PathBuf::from(file));
    }
    for candidate in candidates {
        match tokio::fs::remove_file(&candidate).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => tracing::warn!(
                path = %candidate.display(),
                error = %e,
                "failed to remove database file while rolling back a failed create"
            ),
        }
    }
}

/// Whether a database still has stale embeddings queued (ADR-053). Used to hold
/// off idle eviction while its processor is mid-drain. A database whose model
/// has not yet wired (or has none) has no processor running, so it reports no
/// pending work.
async fn has_pending_embeddings(services: &DatabaseServices) -> bool {
    let guard = services.embedding_state.read().await;
    let Some(ready) = guard.as_ref() else {
        return false;
    };
    // Debounce window `0` counts every stale root, not just those past debounce —
    // any queued work should defer eviction.
    match ready
        .embedding_service
        .store()
        .get_stale_embedding_root_ids(None, 0, EmbeddingConfig::default().max_retries)
        .await
    {
        Ok(ids) => !ids.is_empty(),
        Err(e) => {
            tracing::warn!(error = %e, "failed to check pending embeddings during idle sweep");
            // On error, be conservative and treat the database as busy.
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nodespace_agent::pty::PtySessionManager;
    use nodespace_core::services::EmbeddingScheduler;
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
            scheduler: Arc::new(EmbeddingScheduler::new()),
            subtree_gate_factory: Arc::new(std::sync::OnceLock::new()),
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

    #[test]
    fn default_is_doomed_flags_temp_default_under_a_durable_registry() {
        let temp_default = std::env::temp_dir().join("throwaway").join("db");
        let durable_registry = PathBuf::from("/durable-home/.nodespace/databases.toml");
        let durable_default = PathBuf::from("/durable-home/.nodespace/database/nodespace.db");

        // Durable registry + temp default → doomed (the poisoning we repair).
        assert!(default_is_doomed(&durable_registry, &temp_default));
        // Durable registry + durable default → healthy.
        assert!(!default_is_doomed(&durable_registry, &durable_default));
        // Isolated registry (itself under temp) → never doomed; a temp default
        // is intentional there, and repairing it would clobber test fixtures.
        let temp_registry = std::env::temp_dir().join("iso").join("databases.toml");
        assert!(!default_is_doomed(&temp_registry, &temp_default));
    }

    #[tokio::test]
    async fn repair_doomed_default_is_noop_for_isolated_registry() {
        // temp_manager's registry lives under a temp dir → isolated env, so even
        // a temp default must be left alone. This is what keeps the daemon's own
        // test suites from "repairing" (and clobbering) their own fixtures.
        let (mgr, _dir) = temp_manager().await;
        let temp_db = std::env::temp_dir().join("iso-db").join("nodespace.db");
        mgr.ensure_default_registered("Default".into(), temp_db.clone())
            .await
            .unwrap();

        let repaired = mgr
            .repair_doomed_default(&PathBuf::from(
                "/durable-home/.nodespace/database/nodespace.db",
            ))
            .await
            .unwrap();

        assert!(repaired.is_none());
        assert_eq!(mgr.default_database_path().await.as_ref(), Some(&temp_db));
    }

    #[tokio::test]
    async fn set_standard_default_replaces_the_previous_default() {
        let (mgr, _dir) = temp_manager().await;
        let doomed = std::env::temp_dir().join("old").join("db");
        mgr.ensure_default_registered("Default".into(), doomed)
            .await
            .unwrap();

        let standard = PathBuf::from("/durable-home/.nodespace/database/nodespace.db");
        let new_id = mgr.set_standard_default(&standard).await.unwrap();

        // Exactly one entry — the doomed one is gone, replaced by the standard
        // default the registry now serves.
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.default_database.as_ref(), Some(&new_id));
        assert_eq!(mgr.default_database_path().await.as_ref(), Some(&standard));
    }

    #[tokio::test]
    async fn set_standard_default_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();
        let doomed = std::env::temp_dir()
            .join("doomed-for-set-standard-default-rollback-test")
            .join("db");
        let doomed_id = mgr
            .ensure_default_registered("Default".into(), doomed.clone())
            .await
            .unwrap();

        break_registry_persistence(&registry_path).await;

        let standard = PathBuf::from("/durable-home/.nodespace/database/nodespace.db");
        mgr.set_standard_default(&standard).await.unwrap_err();

        // Pre-call state fully restored: the doomed entry is still
        // registered (not replaced by a new, unpersisted one) and still the
        // default. Runs on every daemon boot via `repair_doomed_default`, so
        // a failed save here must not leave the registry worse off than
        // either the pre- or post-repair state.
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1, "the doomed entry must be restored");
        assert_eq!(snap.databases[0].entry.id, doomed_id);
        assert_eq!(snap.databases[0].entry.path, doomed);
        assert_eq!(snap.default_database.as_ref(), Some(&doomed_id));

        // With persistence restored, the same call succeeds cleanly.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let new_id = mgr.set_standard_default(&standard).await.unwrap();
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].entry.id, new_id);
        assert_eq!(snap.default_database.as_ref(), Some(&new_id));
    }

    #[tokio::test]
    async fn ensure_default_registered_empty_registry_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();

        break_registry_persistence(&registry_path).await;

        mgr.ensure_default_registered("Default".into(), dir.path().join("db"))
            .await
            .unwrap_err();

        // Pre-call state restored: no entry, no default.
        let snap = mgr.list().await;
        assert!(snap.databases.is_empty());
        assert_eq!(snap.default_database, None);

        // With persistence restored, the same call succeeds cleanly.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let id = mgr
            .ensure_default_registered("Default".into(), dir.path().join("db"))
            .await
            .unwrap();
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.default_database.as_ref(), Some(&id));
    }

    #[tokio::test]
    async fn ensure_default_registered_adopt_first_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();

        // Reach "entries exist, no default" through the public API (rather
        // than only the state ensure_default_registered itself would
        // produce): register two, then remove the default one — exactly
        // what a caller unregistering the current default does in practice.
        let first = mgr
            .ensure_default_registered("First".into(), dir.path().join("first.db"))
            .await
            .unwrap();
        let second_path = dir.path().join("second.db");
        std::fs::write(&second_path, b"").unwrap();
        let second = mgr.register(second_path).await.unwrap();
        mgr.remove(&first).await.unwrap();
        assert_eq!(mgr.list().await.default_database, None);

        break_registry_persistence(&registry_path).await;

        mgr.ensure_default_registered("Ignored".into(), dir.path().join("ignored.db"))
            .await
            .unwrap_err();

        // Pre-call state restored: still no default, no new entry, the
        // surviving entry untouched.
        let snap = mgr.list().await;
        assert_eq!(snap.default_database, None);
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].entry.id, second.id);

        // With persistence restored, the same call succeeds and adopts
        // `second` as the default.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let adopted = mgr
            .ensure_default_registered("Ignored".into(), dir.path().join("ignored.db"))
            .await
            .unwrap();
        assert_eq!(adopted, second.id);
        assert_eq!(mgr.list().await.default_database.as_ref(), Some(&second.id));
    }

    #[tokio::test]
    async fn set_bound_tenant_mirrors_persists_and_clears() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(path.clone(), test_context())
            .await
            .unwrap();
        let id = mgr
            .ensure_default_registered("Default".into(), PathBuf::from("/tmp/ns.db"))
            .await
            .unwrap();

        // Fresh registrations are unbound.
        let snap = mgr.list().await;
        assert_eq!(snap.databases[0].entry.bound_tenant_schema, None);
        assert_eq!(snap.databases[0].entry.bound_tenant_collection, None);

        mgr.set_bound_tenant(&id, Some("tenant_demo".into()), Some("c0".into()))
            .await
            .unwrap();
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases[0].entry.bound_tenant_schema.as_deref(),
            Some("tenant_demo")
        );
        assert_eq!(
            snap.databases[0].entry.bound_tenant_collection.as_deref(),
            Some("c0")
        );

        // The binding survives a reload — the mirror is persisted to the registry.
        drop(mgr);
        let mgr = DatabaseManager::load(path, test_context()).await.unwrap();
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases[0].entry.bound_tenant_schema.as_deref(),
            Some("tenant_demo")
        );

        // Unbind clears the mirror.
        mgr.set_bound_tenant(&id, None, None).await.unwrap();
        let snap = mgr.list().await;
        assert_eq!(snap.databases[0].entry.bound_tenant_schema, None);
        assert_eq!(snap.databases[0].entry.bound_tenant_collection, None);
    }

    /// Put a directory at `registry_path`, so any subsequent `Registry::save`
    /// fails at the `tokio::fs::write` step. Mirrors the technique
    /// `failed_create_rolls_back_and_leaves_the_daemon_serviceable` uses to
    /// break persistence without touching in-memory state. Tolerates
    /// `registry_path` not existing yet — a manager that has never
    /// successfully saved has no file there to remove first.
    async fn break_registry_persistence(registry_path: &Path) {
        match tokio::fs::remove_file(registry_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => panic!("failed to remove {}: {e}", registry_path.display()),
        }
        tokio::fs::create_dir(registry_path).await.unwrap();
    }

    #[tokio::test]
    async fn remove_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();

        // Two entries, so removing the first also exercises index-preserving
        // restoration rather than a trivial single-element case.
        let first = mgr
            .ensure_default_registered("First".into(), dir.path().join("first.db"))
            .await
            .unwrap();
        let second_path = dir.path().join("second.db");
        std::fs::write(&second_path, b"").unwrap();
        let second = mgr.register(second_path).await.unwrap();

        break_registry_persistence(&registry_path).await;

        mgr.remove(&first).await.unwrap_err();

        // Pre-call state fully restored: both entries present, in their
        // original order (not the removed one pushed to the end), and the
        // default — cleared in memory as part of the same mutation — back in
        // place.
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 2, "removed entry must be restored");
        assert_eq!(
            snap.databases[0].entry.id, first,
            "restored entry must be back at its original index"
        );
        assert_eq!(snap.databases[1].entry.id, second.id);
        assert_eq!(
            snap.default_database.as_ref(),
            Some(&first),
            "default cleared in memory must be restored on a failed save"
        );

        // With persistence restored, the same removal succeeds cleanly. `first`
        // was never opened (Closed), so `close()`'s own notify is a no-op —
        // this exercises `remove`'s unconditional notify on success.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let changes = mgr.subscribe_changes();
        mgr.remove(&first).await.unwrap();
        assert!(
            changes.has_changed().unwrap(),
            "a successful remove must notify subscribers even for a database that was never opened"
        );
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].entry.id, second.id);
    }

    #[tokio::test]
    async fn remove_of_a_non_default_entry_rolls_back_without_touching_the_default() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();

        // `first` becomes the default (first entry ever registered); `second`
        // is not. This is the scenario `remove_rolls_back_when_save_fails`
        // does not cover — it only removes the entry that IS the default, so
        // the apply phase there always clears `default_database` and a bug
        // that instead unconditionally *assigns* `default_database` in the
        // rollback (rather than restoring it only when the apply phase
        // touched it) would go unnoticed.
        let first = mgr
            .ensure_default_registered("First".into(), dir.path().join("first.db"))
            .await
            .unwrap();
        let second_path = dir.path().join("second.db");
        std::fs::write(&second_path, b"").unwrap();
        let second = mgr.register(second_path).await.unwrap();
        assert_eq!(mgr.list().await.default_database.as_ref(), Some(&first));

        break_registry_persistence(&registry_path).await;

        // Remove the NON-default database. The apply phase never touches
        // `default_database` at all in this case.
        mgr.remove(&second.id).await.unwrap_err();

        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 2, "removed entry must be restored");
        assert_eq!(
            snap.default_database.as_ref(),
            Some(&first),
            "the default must be untouched by a failed removal of a different, non-default entry"
        );
    }

    #[tokio::test]
    async fn remove_of_an_open_database_notifies_exactly_once() {
        let (mgr, dir) = temp_manager().await;
        let entry = mgr
            .create("Open".into(), Some(dir.path().join("open.db")))
            .await
            .unwrap();
        // `create` opens the database as a side effect of registering it.
        assert_eq!(
            mgr.list().await.databases[0].status,
            DatabaseStatus::Open,
            "precondition: the database must be open for this test to exercise the close() path"
        );

        let before = *mgr.subscribe_changes().borrow();
        mgr.remove(&entry.id).await.unwrap();
        let after = *mgr.subscribe_changes().borrow();

        // `remove` notifies once for the registry change (via
        // `mutate_and_save`); closing the open handle must not notify a
        // second time for what is, to a subscriber, a single logical event.
        assert_eq!(
            after.wrapping_sub(before),
            1,
            "removing an open database must notify exactly once, not twice"
        );
    }

    #[tokio::test]
    async fn rename_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();
        let id = mgr
            .ensure_default_registered("Original".into(), dir.path().join("db"))
            .await
            .unwrap();

        break_registry_persistence(&registry_path).await;

        mgr.rename(&id, "Renamed".into()).await.unwrap_err();

        // list() must still report the pre-call name — a rename that only
        // "succeeded" in memory would silently revert on the next restart.
        let snap = mgr.list().await;
        assert_eq!(snap.databases[0].entry.name, "Original");

        // With persistence restored, the same rename succeeds cleanly.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        mgr.rename(&id, "Renamed".into()).await.unwrap();
        assert_eq!(mgr.list().await.databases[0].entry.name, "Renamed");
    }

    #[tokio::test]
    async fn set_default_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();
        let first = mgr
            .ensure_default_registered("First".into(), dir.path().join("first.db"))
            .await
            .unwrap();
        let second_path = dir.path().join("second.db");
        std::fs::write(&second_path, b"").unwrap();
        let second = mgr.register(second_path).await.unwrap();

        break_registry_persistence(&registry_path).await;

        mgr.set_default(&second.id).await.unwrap_err();

        // The previous default must still be reported — not the one that only
        // got as far as being written into memory before the save failed.
        assert_eq!(mgr.list().await.default_database.as_ref(), Some(&first));

        // With persistence restored, the same call succeeds cleanly and
        // notifies subscribers (matching insert_entry/rename).
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let changes = mgr.subscribe_changes();
        mgr.set_default(&second.id).await.unwrap();
        assert!(
            changes.has_changed().unwrap(),
            "a successful set_default must notify subscribers"
        );
        assert_eq!(mgr.list().await.default_database.as_ref(), Some(&second.id));
    }

    #[tokio::test]
    async fn set_bound_tenant_rolls_back_when_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();
        let id = mgr
            .ensure_default_registered("Default".into(), dir.path().join("db"))
            .await
            .unwrap();
        mgr.set_bound_tenant(&id, Some("tenant_a".into()), Some("c0".into()))
            .await
            .unwrap();

        break_registry_persistence(&registry_path).await;

        mgr.set_bound_tenant(&id, Some("tenant_b".into()), Some("c1".into()))
            .await
            .unwrap_err();

        // The previous binding must still be reported after a failed save.
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases[0].entry.bound_tenant_schema.as_deref(),
            Some("tenant_a")
        );
        assert_eq!(
            snap.databases[0].entry.bound_tenant_collection.as_deref(),
            Some("c0")
        );

        // With persistence restored, the same call succeeds cleanly and
        // notifies subscribers (matching insert_entry/rename).
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let changes = mgr.subscribe_changes();
        mgr.set_bound_tenant(&id, Some("tenant_b".into()), Some("c1".into()))
            .await
            .unwrap();
        assert!(
            changes.has_changed().unwrap(),
            "a successful set_bound_tenant must notify subscribers"
        );
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases[0].entry.bound_tenant_schema.as_deref(),
            Some("tenant_b")
        );
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
        let second_path = _dir.path().join("second.db");
        std::fs::write(&second_path, b"").unwrap();
        let second = mgr.register(second_path).await.unwrap();
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
    async fn create_creates_and_opens_the_database_file_before_registering() {
        let (mgr, dir) = temp_manager().await;
        let path = dir.path().join("fresh.db");

        let entry = mgr
            .create("Fresh".into(), Some(path.clone()))
            .await
            .unwrap();

        // The database file exists on disk the moment create returns — the
        // registry never advertises a database with no file behind it.
        assert!(path.exists(), "create must create the database file");

        // The fresh database is open and, as the first registration, default.
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].status, DatabaseStatus::Open);
        assert!(snap.databases[0].is_default);
        assert_eq!(snap.default_database.as_ref(), Some(&entry.id));

        // The handle cached by create is the one routing reuses — no second open.
        let first = mgr.get_or_open(&entry.id).await.unwrap();
        let second = mgr.get_or_open(&entry.id).await.unwrap();
        assert!(Arc::ptr_eq(&first, &second));
    }

    #[tokio::test]
    async fn create_rejects_an_existing_file() {
        let (mgr, dir) = temp_manager().await;
        let path = dir.path().join("existing.db");
        std::fs::write(&path, b"data").unwrap();

        let err = mgr
            .create("Clobber".into(), Some(path.clone()))
            .await
            .unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "unexpected error: {err}"
        );

        // Nothing was registered and the existing file is untouched.
        assert!(mgr.list().await.databases.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), b"data");
    }

    #[tokio::test]
    async fn failed_create_rolls_back_and_leaves_the_daemon_serviceable() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = DatabaseManager::load(registry_path.clone(), test_context())
            .await
            .unwrap();

        // Register + open a healthy default first, as the daemon boot path does.
        let default_path = dir.path().join("default.db");
        let default_id = mgr
            .ensure_default_registered("Default".into(), default_path)
            .await
            .unwrap();
        mgr.get_or_open(&default_id).await.unwrap();

        // Break registry persistence, so create fails at the registration
        // step — after the database file has already been created.
        break_registry_persistence(&registry_path).await;

        let fresh_path = dir.path().join("fresh.db");
        mgr.create("Fresh".into(), Some(fresh_path.clone()))
            .await
            .unwrap_err();

        // Rolled back: no registry entry, no leftover file, and the default
        // still resolves and serves — a failed create must not wedge routing.
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases.len(),
            1,
            "a failed create must not leave a registry entry"
        );
        assert!(
            !fresh_path.exists(),
            "a failed create must remove the file it created"
        );
        assert_eq!(mgr.resolve_database_id(None).await.unwrap(), default_id);
        assert!(mgr.get_or_open(&default_id).await.is_ok());

        // With persistence restored, the same create succeeds cleanly.
        tokio::fs::remove_dir(&registry_path).await.unwrap();
        let entry = mgr
            .create("Fresh".into(), Some(fresh_path.clone()))
            .await
            .unwrap();
        assert!(fresh_path.exists());
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 2);
        assert_eq!(
            snap.databases
                .iter()
                .find(|d| d.entry.id == entry.id)
                .unwrap()
                .status,
            DatabaseStatus::Open
        );
    }

    #[tokio::test]
    async fn register_requires_an_existing_file() {
        let (mgr, dir) = temp_manager().await;

        // An absent path is rejected — a registration pointing at nothing would
        // report Missing forever.
        let absent = dir.path().join("not-here.db");
        let err = mgr.register(absent).await.unwrap_err();
        assert!(
            err.to_string().contains("no database file exists"),
            "unexpected error: {err}"
        );
        assert!(mgr.list().await.databases.is_empty());

        // An existing file registers (name from the file stem) and reports
        // Closed until first opened.
        let present = dir.path().join("present.db");
        std::fs::write(&present, b"").unwrap();
        mgr.register(present).await.unwrap();
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].status, DatabaseStatus::Closed);
        assert_eq!(snap.databases[0].entry.name, "present");
        assert!(snap.databases[0].is_default);
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

    /// Empirically proves `mutate_and_save` is cancellation-safe: drops the
    /// future driving a mutation while it is genuinely suspended mid-persist
    /// (not merely "before it started") and asserts the live registry is
    /// untouched afterward.
    ///
    /// The old `persist_or_rollback` design applied the mutation to the live
    /// registry *before* entering the save `.await` — so a future dropped
    /// there (a gRPC client disconnect, a `tonic` timeout, a `select!`
    /// racing the call, a shutdown signal mid-mutation) left the mutation
    /// applied in memory with neither `restore()` nor `notify_changed()`
    /// ever having run. This test uses the test-only [`SaveGate`] to
    /// deterministically suspend a real `rename` call at exactly that point,
    /// then `abort()`s the task driving it — a genuine future-drop, not a
    /// simulation — and checks the invariant empirically rather than by
    /// reading the code and asserting intent.
    #[tokio::test]
    async fn dropping_the_future_mid_persist_leaves_the_registry_untouched() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = Arc::new(
            DatabaseManager::load(registry_path.clone(), test_context())
                .await
                .unwrap(),
        );
        let id = mgr
            .ensure_default_registered("Original".into(), dir.path().join("db"))
            .await
            .unwrap();

        let gate = SaveGate::default();
        mgr.set_save_gate(gate.clone());

        // Drive the mutation on a separate task so it can be cancelled from
        // the outside while parked mid-persist.
        let driver = {
            let mgr = mgr.clone();
            let id = id.clone();
            tokio::spawn(async move { mgr.rename(&id, "Renamed".into()).await })
        };

        // Wait until `rename` has computed its candidate and is genuinely
        // parked at the gate, mid-persist — not merely "spawned."
        gate.entered.notified().await;

        // Drop the driving future while it is suspended there. `abort`
        // cancels at the task's next (i.e. current) yield point, dropping
        // the future in place — the write-lock guard it was holding
        // releases via `Drop`, exactly matching a real
        // disconnect/timeout/select!/shutdown cancellation.
        driver.abort();
        let outcome = driver.await;
        assert!(
            outcome.unwrap_err().is_cancelled(),
            "the driving task must have been cancelled, not completed"
        );

        // The live registry must be untouched: still "Original", never
        // half-applied to "Renamed". `mutate_and_save` only swaps the
        // candidate in *after* save resolves, with no further await in
        // between, so a drop mid-save can only ever observe "nothing
        // happened."
        mgr.clear_save_gate();
        let snap = mgr.list().await;
        assert_eq!(
            snap.databases[0].entry.name, "Original",
            "a mutation cancelled mid-persist must leave the live registry exactly as it was"
        );

        // The manager still works correctly for a real, uncancelled call
        // afterward — the gate doesn't leave anything wedged.
        mgr.rename(&id, "Renamed".into()).await.unwrap();
        assert_eq!(mgr.list().await.databases[0].entry.name, "Renamed");
    }

    /// Same proof as
    /// [`dropping_the_future_mid_persist_leaves_the_registry_untouched`], but
    /// for the "push a new entry" shape (`insert_entry`, reached here via
    /// `register`) rather than mutating an existing entry — the issue's scope
    /// note specifically calls out verifying the push-based mutators
    /// translate cleanly to this design too.
    #[tokio::test]
    async fn dropping_the_future_mid_persist_during_insert_leaves_no_partial_entry() {
        let dir = tempfile::tempdir().unwrap();
        let registry_path = dir.path().join("databases.toml");
        let mgr = Arc::new(
            DatabaseManager::load(registry_path.clone(), test_context())
                .await
                .unwrap(),
        );

        let gate = SaveGate::default();
        mgr.set_save_gate(gate.clone());

        let path = dir.path().join("fresh.db");
        std::fs::write(&path, b"").unwrap();
        let driver = {
            let mgr = mgr.clone();
            let path = path.clone();
            tokio::spawn(async move { mgr.register(path).await })
        };

        gate.entered.notified().await;
        driver.abort();
        let outcome = driver.await;
        assert!(
            outcome.unwrap_err().is_cancelled(),
            "the driving task must have been cancelled, not completed"
        );

        // Cancelling mid-persist during an insert must leave no partial
        // entry — not a half-registered database, and no default silently
        // adopted.
        mgr.clear_save_gate();
        let snap = mgr.list().await;
        assert!(
            snap.databases.is_empty(),
            "a cancelled insert must leave no registry entry behind"
        );
        assert_eq!(snap.default_database, None);

        // A subsequent real call still works and adopts the entry as default
        // (first-ever registration).
        let entry = mgr.register(path).await.unwrap();
        let snap = mgr.list().await;
        assert_eq!(snap.databases.len(), 1);
        assert_eq!(snap.databases[0].entry.id, entry.id);
        assert_eq!(snap.default_database.as_ref(), Some(&entry.id));
    }
}
