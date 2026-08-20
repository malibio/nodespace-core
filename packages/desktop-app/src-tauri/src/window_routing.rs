//! Window <-> database routing (issue #2033).
//!
//! A window opened deliberately for a specific, already-known database
//! (any future "open in a new window" trigger) gets that database's id as
//! its literal Tauri window label — straightforward, since the id is known
//! before the window is built. The one exception is the app's bootstrap
//! window: at a cold launch with no `--database` pick (the common case —
//! Dock/Spotlight, not the tray's per-database submenu), which database it
//! will show is resolved *asynchronously* by the frontend (a remembered
//! selection in `localStorage`, or the registry default fetched over gRPC
//! once the daemon answers) — nothing on the Rust side can know it
//! synchronously at window-creation time, before the webview has even
//! loaded. That window keeps its static `tauri.conf.json` label ("main") but
//! is tracked here exactly like any other window once the frontend calls
//! [`pin_window_database`] to declare which database it settled on — so
//! routing correctness never actually depends on the literal label string,
//! only on this registry being kept current.
//!
//! [`WindowDatabaseRegistry`] is the live label -> database id map every
//! routing decision below reads. [`emit_routed`] replaces every previous
//! `app.get_webview_window("main")` + `.emit(...)` call (and the
//! database-scoped broadcasts in `watcher.rs`, which reached every open
//! window regardless of which database they belonged to): an event carrying
//! a non-empty `database_id` reaches only the window(s) currently pinned to
//! that database; an event with no id (or the daemon's empty-string
//! "not database-scoped" convention) reaches the focused window, falling
//! back to a deterministic default when nothing is focused — the common case
//! for a moment at startup, before the OS has assigned focus to the
//! just-created window.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, WebviewWindow};

/// Tauri-managed state: maps an open window's label to the database id it is
/// currently pinned to. See the module doc for why this is the source of
/// truth for routing rather than the literal window label.
#[derive(Default)]
pub struct WindowDatabaseRegistry {
    inner: Mutex<HashMap<String, String>>,
}

impl WindowDatabaseRegistry {
    /// Pin `window_label` to `database_id`, returning the id it was
    /// previously pinned to (if any) so callers can detect an actual change —
    /// [`pin_window_database`] uses this to restore saved window geometry
    /// only when a window's database genuinely changed, not on every
    /// re-affirming call (e.g. a background registry refresh).
    pub fn pin(&self, window_label: &str, database_id: &str) -> Option<String> {
        self.inner
            .lock()
            .unwrap()
            .insert(window_label.to_string(), database_id.to_string())
    }

    /// Drop `window_label`'s pin. Called once the window is destroyed so a
    /// stale entry can never route a later event to a label nothing is
    /// listening on, or — worse — to a *different*, newer window that
    /// happens to reuse the same label.
    pub fn unpin(&self, window_label: &str) {
        self.inner.lock().unwrap().remove(window_label);
    }

    /// The database `window_label` is currently pinned to, if any.
    pub fn database_for_window(&self, window_label: &str) -> Option<String> {
        self.inner.lock().unwrap().get(window_label).cloned()
    }

    /// A point-in-time copy of the whole label -> database id map, for the
    /// pure [`resolve_targets`] resolution below (kept free of Tauri types so
    /// it can be unit-tested directly).
    fn snapshot(&self) -> HashMap<String, String> {
        self.inner.lock().unwrap().clone()
    }
}

/// Pure routing decision — no Tauri types involved, so this is fast and
/// exhaustively unit-testable without a mock app or a real window.
///
/// - `database_id` present and non-empty: every currently-open window pinned
///   to that id. Normally 0 or 1, but nothing here assumes uniqueness.
/// - `database_id` absent (or empty — the daemon's convention for an event
///   that is not database-scoped, e.g. a Pro daemon with no registry): the
///   focused window if one is both focused and still open; otherwise the
///   lexicographically-first open label, for a deterministic answer instead
///   of whatever a `HashMap`'s iteration order happens to produce. Empty when
///   no window is open at all.
pub fn resolve_targets(
    pins: &HashMap<String, String>,
    open_labels: &[String],
    focused_label: Option<&str>,
    database_id: Option<&str>,
) -> Vec<String> {
    match database_id.filter(|id| !id.is_empty()) {
        Some(id) => open_labels
            .iter()
            .filter(|label| pins.get(label.as_str()).map(String::as_str) == Some(id))
            .cloned()
            .collect(),
        None => {
            if let Some(focused) = focused_label {
                if open_labels.iter().any(|l| l == focused) {
                    return vec![focused.to_string()];
                }
            }
            open_labels.iter().min().cloned().into_iter().collect()
        }
    }
}

/// [`resolve_targets`] wired up against the real Tauri window set for `app` —
/// gathers the live open-label/focused-label/pin inputs and returns just the
/// target label(s), not window objects. `Emitter::emit` on a `WebviewWindow`
/// is NOT scoped to that window (confirmed against the `tauri` 2.11 source —
/// every `Emitter` implementor's `emit` shares one default body that always
/// calls the manager's unscoped broadcast, `self.manager().emit(...)`,
/// regardless of which object it was called on); [`emit_routed`] below
/// therefore emits via `Emitter::emit_to(label, ...)` against these labels
/// rather than calling `.emit()` on a resolved window — a mistake real
/// enough that the very first test written against this module caught it
/// (see `emit_routed_reaches_only_the_window_pinned_to_the_database`).
fn resolve_target_labels<R: Runtime>(app: &AppHandle<R>, database_id: Option<&str>) -> Vec<String> {
    let windows = app.webview_windows();
    let open_labels: Vec<String> = windows.keys().cloned().collect();
    let focused_label = windows
        .iter()
        .find(|(_, w)| w.is_focused().unwrap_or(false))
        .map(|(label, _)| label.clone());
    let pins = app
        .try_state::<WindowDatabaseRegistry>()
        .map(|r| r.snapshot())
        .unwrap_or_default();

    resolve_targets(&pins, &open_labels, focused_label.as_deref(), database_id)
}

/// Replacement for every previous `app.get_webview_window("main")` +
/// `.emit(...)` call site, plus the database-scoped broadcasts in
/// `watcher.rs` that used to reach every open window regardless of which
/// database they belonged to. Routes to the window(s) pinned to
/// `database_id`, or the focused window when `database_id` is `None` (or
/// empty).
///
/// When NO window exists in the whole app yet, there is nothing to scope
/// to — this falls back to the same unscoped broadcast every emit used
/// before this module existed, rather than silently dropping the event.
/// This matters in production for the narrow window between process start
/// and the first window materializing, and it is *load-bearing* for the
/// `tests/*.rs` real-daemon integration suite (`optimistic_echo_race_test.rs`
/// et al.), which drives `watcher::run` against a bare `AppHandle` with no
/// window at all and asserts on events via `AppHandle::listen` — exactly the
/// regression the first version of this function shipped with, caught by
/// running that pre-existing suite rather than only this module's own tests.
///
/// Once at least one window exists, a `database_id` naming a database no
/// open window is pinned to is not an error — there is nowhere for that
/// specific event to go, so it is dropped (logged at debug) rather than
/// broadcast to windows showing a different database.
pub fn emit_routed<R: Runtime, S: Serialize + Clone>(
    app: &AppHandle<R>,
    event: &str,
    payload: S,
    database_id: Option<&str>,
) {
    if app.webview_windows().is_empty() {
        if let Err(e) = app.emit(event, payload) {
            tracing::warn!(event, error = %e, "failed to broadcast event (no windows open)");
        }
        return;
    }

    let targets = resolve_target_labels(app, database_id);
    if targets.is_empty() {
        tracing::debug!(
            event,
            ?database_id,
            "emit_routed: no target window open; dropping event"
        );
        return;
    }
    for label in targets {
        if let Err(e) = app.emit_to(label.as_str(), event, payload.clone()) {
            tracing::warn!(event, %label, error = %e, "failed to emit routed event");
        }
    }
}

/// Focus-resolution entry point for the single-instance relaunch handler and
/// the app menu's Quit item, both of which need "the one *window object* to
/// act on" (to call `unminimize`/`show`/`set_focus`/`close`) rather than an
/// event's target label(s) — same underlying resolution (`database_id:
/// None`), resolved to at most one real window.
pub fn resolve_focus_window<R: Runtime>(app: &AppHandle<R>) -> Option<WebviewWindow<R>> {
    let label = resolve_target_labels(app, None).into_iter().next()?;
    app.get_webview_window(&label)
}

/// Declare (or update) which database `window` is showing. Called by the
/// frontend once `databaseStore` resolves its active database — on initial
/// load and on every `switchTo` — so [`WindowDatabaseRegistry`] reflects
/// reality even for the bootstrap window, whose database isn't known at
/// window-creation time (see the module doc).
///
/// Best-effort restores that database's last saved size/position (issue
/// #2033's window-state-persistence scope item) the first time this window
/// pins to it — not on every call, so re-affirming the same pin (e.g. a
/// background registry reload) never yanks a window the user has since
/// resized back to an old saved geometry.
#[tauri::command]
pub async fn pin_window_database(
    window: tauri::WebviewWindow,
    app: tauri::AppHandle,
    id: String,
) -> Result<(), String> {
    let registry = app
        .try_state::<WindowDatabaseRegistry>()
        .ok_or_else(|| "WindowDatabaseRegistry not managed".to_string())?;
    let previous = registry.pin(window.label(), &id);

    if previous.as_deref() != Some(id.as_str()) {
        if let Some(geometry) = crate::window_state::load_geometry(&app, &id).await {
            if let Err(e) =
                window.set_size(tauri::PhysicalSize::new(geometry.width, geometry.height))
            {
                tracing::warn!(error = %e, "failed to restore saved window size");
            }
            if let Err(e) =
                window.set_position(tauri::PhysicalPosition::new(geometry.x, geometry.y))
            {
                tracing::warn!(error = %e, "failed to restore saved window position");
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn pins(pairs: &[(&str, &str)]) -> HashMap<String, String> {
        pairs
            .iter()
            .map(|(l, d)| (l.to_string(), d.to_string()))
            .collect()
    }

    #[test]
    fn database_id_routes_to_the_window_pinned_to_it() {
        let pins = pins(&[("win-a", "db-1"), ("win-b", "db-2")]);
        let open = labels(&["win-a", "win-b"]);
        assert_eq!(
            resolve_targets(&pins, &open, None, Some("db-2")),
            vec!["win-b".to_string()]
        );
    }

    #[test]
    fn database_id_with_no_matching_open_window_yields_no_targets() {
        let pins = pins(&[("win-a", "db-1")]);
        let open = labels(&["win-a"]);
        // db-3 isn't open in any window — nowhere for the event to go.
        assert!(resolve_targets(&pins, &open, None, Some("db-3")).is_empty());
    }

    #[test]
    fn database_id_matching_a_closed_window_is_excluded_even_if_the_registry_is_stale() {
        // win-b's registry entry wasn't cleaned up (e.g. Destroyed fired
        // before this snapshot), but it is no longer in open_labels.
        let pins = pins(&[("win-a", "db-1"), ("win-b", "db-2")]);
        let open = labels(&["win-a"]);
        assert!(resolve_targets(&pins, &open, None, Some("db-2")).is_empty());
    }

    #[test]
    fn database_id_matching_multiple_windows_returns_all_of_them() {
        let pins = pins(&[("win-a", "db-1"), ("win-b", "db-1")]);
        let open = labels(&["win-a", "win-b"]);
        let mut targets = resolve_targets(&pins, &open, None, Some("db-1"));
        targets.sort();
        assert_eq!(targets, vec!["win-a".to_string(), "win-b".to_string()]);
    }

    #[test]
    fn no_database_id_routes_to_the_focused_window() {
        let pins = pins(&[("win-a", "db-1"), ("win-b", "db-2")]);
        let open = labels(&["win-a", "win-b"]);
        assert_eq!(
            resolve_targets(&pins, &open, Some("win-b"), None),
            vec!["win-b".to_string()]
        );
    }

    #[test]
    fn empty_database_id_is_treated_the_same_as_no_id() {
        let pins = pins(&[("win-a", "db-1")]);
        let open = labels(&["win-a"]);
        assert_eq!(
            resolve_targets(&pins, &open, Some("win-a"), Some("")),
            vec!["win-a".to_string()]
        );
    }

    #[test]
    fn no_database_id_and_no_focus_falls_back_to_the_first_label_deterministically() {
        let pins = pins(&[]);
        let open = labels(&["win-c", "win-a", "win-b"]);
        assert_eq!(
            resolve_targets(&pins, &open, None, None),
            vec!["win-a".to_string()]
        );
    }

    #[test]
    fn a_focused_label_that_is_no_longer_open_falls_back_like_no_focus_at_all() {
        let pins = pins(&[]);
        let open = labels(&["win-a", "win-b"]);
        assert_eq!(
            resolve_targets(&pins, &open, Some("win-gone"), None),
            vec!["win-a".to_string()]
        );
    }

    #[test]
    fn no_windows_open_at_all_yields_no_targets() {
        let pins = pins(&[]);
        let open: Vec<String> = vec![];
        assert!(resolve_targets(&pins, &open, None, None).is_empty());
        assert!(resolve_targets(&pins, &open, None, Some("db-1")).is_empty());
    }

    #[test]
    fn registry_pin_returns_the_previous_value_and_unpin_removes_it() {
        let registry = WindowDatabaseRegistry::default();
        assert_eq!(registry.pin("win-a", "db-1"), None);
        assert_eq!(
            registry.database_for_window("win-a"),
            Some("db-1".to_string())
        );
        assert_eq!(registry.pin("win-a", "db-2"), Some("db-1".to_string()));
        assert_eq!(
            registry.database_for_window("win-a"),
            Some("db-2".to_string())
        );
        registry.unpin("win-a");
        assert_eq!(registry.database_for_window("win-a"), None);
    }

    /// Real Tauri windows via `tauri::test::mock_app()` (mirrors
    /// `relaunch_tests`/`shutdown_tests` in `lib.rs`) — proves
    /// `emit_routed`/`resolve_focus_window` correctly wire the pure
    /// resolution above to the real window/event APIs. `MockRuntime` reports
    /// every window as unfocused (confirmed against the `tauri` 2.11 source —
    /// its `WindowDispatch::is_focused` always returns `Ok(false)`), so this
    /// exercises the "no focus" fallback path; the focused-window path itself
    /// is already covered above by the pure unit tests.
    #[test]
    fn emit_routed_reaches_only_the_window_pinned_to_the_database() {
        use std::sync::{Arc, Mutex as StdMutex};
        use tauri::Listener;

        let app = tauri::test::mock_app();
        app.manage(WindowDatabaseRegistry::default());
        let handle = app.handle().clone();

        let win_a = tauri::WebviewWindowBuilder::new(&app, "win-a", Default::default())
            .build()
            .expect("failed to build mock window a");
        let win_b = tauri::WebviewWindowBuilder::new(&app, "win-b", Default::default())
            .build()
            .expect("failed to build mock window b");

        let registry = handle.state::<WindowDatabaseRegistry>();
        registry.pin("win-a", "db-1");
        registry.pin("win-b", "db-2");

        let received_a: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let received_b: Arc<StdMutex<Vec<String>>> = Arc::new(StdMutex::new(Vec::new()));
        let ra = received_a.clone();
        let rb = received_b.clone();
        win_a.listen("test-event", move |event| {
            ra.lock().unwrap().push(event.payload().to_string());
        });
        win_b.listen("test-event", move |event| {
            rb.lock().unwrap().push(event.payload().to_string());
        });

        emit_routed(&handle, "test-event", "hello-db-1", Some("db-1"));

        assert_eq!(
            received_a.lock().unwrap().len(),
            1,
            "db-1's window must receive the event"
        );
        assert!(
            received_b.lock().unwrap().is_empty(),
            "db-2's window must NOT receive an event routed to db-1 — no cross-talk"
        );
    }

    #[test]
    fn resolve_focus_window_falls_back_deterministically_when_nothing_is_focused() {
        let app = tauri::test::mock_app();
        app.manage(WindowDatabaseRegistry::default());
        let handle = app.handle().clone();

        tauri::WebviewWindowBuilder::new(&app, "zzz-later", Default::default())
            .build()
            .expect("failed to build mock window");
        tauri::WebviewWindowBuilder::new(&app, "aaa-first", Default::default())
            .build()
            .expect("failed to build mock window");

        let target = resolve_focus_window(&handle).expect("expected a fallback window");
        assert_eq!(target.label(), "aaa-first");
    }

    #[test]
    fn emit_routed_with_no_matching_window_does_not_panic() {
        let app = tauri::test::mock_app();
        app.manage(WindowDatabaseRegistry::default());
        let handle = app.handle().clone();
        // No windows exist at all — must be a harmless no-op, not a panic.
        emit_routed(&handle, "test-event", "payload", Some("db-nowhere"));
        emit_routed(&handle, "test-event", "payload", None);
    }
}
