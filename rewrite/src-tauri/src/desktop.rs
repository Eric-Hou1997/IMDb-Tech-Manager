use product_core::{
    store::{StartupSources, Store},
    *,
};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
    time::Duration,
};
use tauri::{Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
fn startup_sources(os: &str, identifier: &str, home: &std::path::Path) -> Option<StartupSources> {
    (os == "macos" && !identifier.to_ascii_lowercase().contains("validation")).then(|| {
        StartupSources {
            manager: home.join("Library/Application Support/IMDb Tech Manager"),
            engine: home.join("Library/Application Support/tmm-imdb-tech"),
        }
    })
}
pub(crate) fn startup_command_allowed(ready: bool, stopping: bool, command: &str) -> bool {
    matches!(command, "runtime_probe" | "quit_probe")
        || !stopping && (ready || command == "configuration")
}
#[derive(Default)]
struct ReconcileSchedule {
    launch_finished: bool,
    saved_revision: Option<u32>,
}
impl ReconcileSchedule {
    fn saved(&mut self, before: &Configuration, after: &Configuration, confirmed: bool) {
        if !confirmed || before.roots != after.roots {
            self.saved_revision = (!after.roots.is_empty()).then_some(after.revision);
        }
    }
    fn poll(&mut self, store: &Store, launch_id: &str, elapsed: Duration) -> Result<Vec<Task>> {
        let request = if let Some(revision) = self.saved_revision {
            // A confirmed save must still load its roots after the bounded
            // startup wait. The normal worker waits for active writes/tasks.
            format!("roots-{revision}")
        } else {
            if self.launch_finished || elapsed < Duration::from_millis(1500) {
                return Ok(vec![]);
            }
            if elapsed >= Duration::from_millis(31500) {
                self.launch_finished = true;
                return Ok(vec![]);
            }
            launch_id.to_owned()
        };
        match store.reconcile_on_launch(&request) {
            Ok(None) => Ok(vec![]),
            result => {
                self.launch_finished = true;
                self.saved_revision = None;
                result.map(|tasks| tasks.unwrap_or_default())
            }
        }
    }
}
pub struct Desktop {
    pub store: Arc<Store>,
    pub(crate) write_gate: Mutex<()>,
    startup_sources: Option<StartupSources>,
    startup_gate: Mutex<()>,
    startup_ready: AtomicBool,
    stop: Arc<AtomicBool>,
    reconcile: Arc<Mutex<ReconcileSchedule>>,
    launch_id: String,
    worker: Mutex<Option<JoinHandle<()>>>,
}
impl Desktop {
    pub fn start(app: &tauri::AppHandle) -> Result<Self> {
        let path = app
            .path()
            .app_data_dir()
            .map_err(|e| AppError::new("data-directory", e))?;
        std::fs::create_dir_all(&path).map_err(|e| AppError::new("data-directory", e))?;
        let store = Arc::new(Store::open(&path.join("workspace.sqlite"))?);
        let home = app
            .path()
            .home_dir()
            .map_err(|e| AppError::new("data-directory", e))?;
        let startup_sources =
            startup_sources(std::env::consts::OS, &app.config().identifier, &home);
        let stop = Arc::new(AtomicBool::new(false));
        let desktop = Self {
            store,
            write_gate: Mutex::new(()),
            startup_sources,
            startup_gate: Mutex::new(()),
            startup_ready: AtomicBool::new(false),
            stop,
            reconcile: Arc::new(Mutex::new(ReconcileSchedule::default())),
            launch_id: format!(
                "launch-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_err(|e| AppError::new("startup-clock", e))?
                    .as_nanos()
            ),
            worker: Mutex::new(None),
        };
        Ok(desktop)
    }
    pub fn ready(&self) -> bool {
        self.startup_ready.load(Ordering::SeqCst)
    }
    fn ensure_startup(&self, app: &tauri::AppHandle) -> Result<()> {
        let _owner = self
            .startup_gate
            .lock()
            .map_err(|e| AppError::new("startup-state", e))?;
        if self.stopping() {
            return Err(AppError::new("startup-cancelled", "Application is closing"));
        }
        if self.ready() {
            return Ok(());
        }
        if let Some(sources) = &self.startup_sources {
            Self::ensure_legacy_idle(sources)?;
            self.store
                .restore_legacy_on_start(sources, &|| self.stopping())?;
            Self::ensure_legacy_idle(sources)?;
        }
        if self.stopping() {
            return Err(AppError::new("startup-cancelled", "Application is closing"));
        }
        crate::lifecycle::initialize(app)?;
        if self.stopping() {
            return Err(AppError::new("startup-cancelled", "Application is closing"));
        }
        self.store.automatic_startup(unix_now())?;
        self.resume(app)?;
        self.startup_ready.store(true, Ordering::SeqCst);
        Ok(())
    }
    pub(crate) fn ensure_legacy_idle(sources: &StartupSources) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            let read = |fields: &str| -> Result<String> {
                let output = std::process::Command::new("/bin/ps")
                    .args(["-wwaxo", fields])
                    .output()
                    .map_err(|e| AppError::new("legacy-runtime-check", e))?;
                if !output.status.success() || output.stdout.len() > (8 << 20) {
                    return Err(AppError::new(
                        "legacy-runtime-check",
                        "无法确认旧后台是否已退出，未启动新的写入后台",
                    ));
                }
                String::from_utf8(output.stdout)
                    .map_err(|e| AppError::new("legacy-runtime-check", e))
            };
            let mut listing = read("pid=,comm=")?;
            // Verify executable identity separately: a shell's quoted diagnostic
            // text must never be mistaken for an old application process.
            let python = listing
                .lines()
                .filter_map(|line| line.trim().split_once(char::is_whitespace))
                .filter(|(_, exe)| {
                    std::path::Path::new(exe.trim())
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with("python"))
                })
                .map(|(pid, _)| pid.to_owned())
                .collect::<std::collections::BTreeSet<_>>();
            let arguments = read("pid=,command=")?;
            for line in arguments.lines().filter(|line| {
                line.trim()
                    .split_once(char::is_whitespace)
                    .is_some_and(|(pid, _)| python.contains(pid))
            }) {
                listing.push('\n');
                listing.push_str(line);
            }
            product_core::legacy_startup::ensure_idle(
                &listing,
                &sources.manager,
                std::process::id(),
            )?;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = sources;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    pub(crate) fn legacy_sources(&self) -> Option<&StartupSources> {
        self.startup_sources.as_ref()
    }
    pub fn resume(&self, app: &tauri::AppHandle) -> Result<()> {
        let mut worker = self
            .worker
            .lock()
            .map_err(|e| AppError::new("worker-state", e))?;
        if worker.is_some() {
            return Ok(());
        }
        if self.stopping() {
            return Err(AppError::new("startup-cancelled", "Application is closing"));
        }
        let worker_store = self.store.clone();
        let worker_stop = self.stop.clone();
        let reconcile = self.reconcile.clone();
        let launch_id = self.launch_id.clone();
        let handle = app.clone();
        *worker = Some(
            std::thread::Builder::new()
                .name("library-worker".into())
                .spawn(move || {
                    if let Err(error) = worker_store.maintain_imdb_cache_automatically() {
                        let _ = handle.emit("cache-maintenance-failed", &error);
                    }
                    let mut auto_check = std::time::Instant::now() - Duration::from_secs(1);
                    let launched = std::time::Instant::now();
                    let mut reconcile_check = launched;
                    while !worker_stop.load(Ordering::SeqCst) {
                        // One worker owns startup and explicit roots-save scans,
                        // including idle waiting and synchronous shutdown.
                        if reconcile_check.elapsed() >= Duration::from_millis(500) {
                            reconcile_check = std::time::Instant::now();
                            let result = reconcile
                                .lock()
                                .map_err(|e| AppError::new("reconcile-state", e))
                                .and_then(|mut schedule| {
                                    schedule.poll(&worker_store, &launch_id, launched.elapsed())
                                });
                            match result {
                                Ok(tasks) => {
                                    for task in tasks {
                                        let _ = handle.emit("task-changed", task);
                                    }
                                }
                                Err(error) => {
                                    let _ = handle.emit("worker-failed", &error);
                                }
                            }
                        }
                        if auto_check.elapsed() >= Duration::from_secs(1) {
                            auto_check = std::time::Instant::now();
                            match worker_store.automatic_tick(unix_now()) {
                                Ok(tasks) => {
                                    for task in tasks {
                                        let _ = handle.emit("task-changed", task);
                                    }
                                }
                                Err(error) => {
                                    let _ = handle.emit("worker-failed", &error);
                                }
                            }
                        }
                        match worker_store.run_batch_next(
                            || worker_stop.load(Ordering::SeqCst),
                            |task| {
                                let _ = handle.emit("task-changed", task);
                            },
                            |task, row| crate::batches::execute(&handle, task, row),
                        ) {
                            Ok(Some(_)) => continue,
                            Ok(None) => {}
                            Err(error) => {
                                let _ = handle.emit("worker-failed", &error);
                                break;
                            }
                        }
                        match worker_store.run_next(
                            || worker_stop.load(Ordering::SeqCst),
                            |task| {
                                if let Err(e) = handle.emit("task-changed", task) {
                                    eprintln!("task-event: {e}");
                                }
                            },
                        ) {
                            Ok(Some(_)) => {}
                            Ok(None) => std::thread::sleep(Duration::from_millis(100)),
                            Err(error) => {
                                let _ = handle.emit("worker-failed", &error);
                                eprintln!("library-worker: {error}");
                                break;
                            }
                        }
                    }
                })
                .map_err(|e| AppError::new("worker-start", e))?,
        );
        Ok(())
    }
    pub fn stopping(&self) -> bool {
        self.stop.load(Ordering::SeqCst)
    }
    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Ok(mut worker) = self.worker.lock() {
            if let Some(worker) = worker.take() {
                if worker.join().is_err() {
                    eprintln!("library-worker panicked during shutdown");
                }
            }
        }
        let _startup = self.startup_gate.lock();
        let _writes = self.write_gate.lock();
    }
}
#[tauri::command]
pub async fn automatic_status(app: tauri::AppHandle) -> Result<automatic::Status> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.automatic_status())
        .await
        .map_err(|e| AppError::new("automatic-worker", e))?
}
#[tauri::command]
pub async fn automatic_apply(
    id: String,
    settings: automatic::Settings,
    enabled: bool,
    expected: automatic::Expected,
    app: tauri::AppHandle,
) -> Result<automatic::Status> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.set_automatic_checked(
            &id,
            settings,
            enabled,
            unix_now(),
            expected,
        )
    })
    .await
    .map_err(|e| AppError::new("automatic-worker", e))?
}
impl Drop for Desktop {
    fn drop(&mut self) {
        self.shutdown();
    }
}
#[tauri::command]
pub async fn configuration(app: tauri::AppHandle) -> Result<Configuration> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Desktop>();
        state.ensure_startup(&app)?;
        state.store.configuration()
    })
    .await
    .map_err(|e| AppError::new("startup-worker", e))?
}
#[tauri::command]
pub async fn onboarding_info(app: tauri::AppHandle) -> Result<product_core::onboarding::Info> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.onboarding_info(|| {
            if std::env::consts::OS == "macos"
                && !app
                    .config()
                    .identifier
                    .to_ascii_lowercase()
                    .contains("validation")
            {
                app.path()
                    .home_dir()
                    .ok()
                    .map(|home| home.join("Library/Application Support/tinyMediaManager/data"))
            } else {
                None
            }
        })
    })
    .await
    .map_err(|e| AppError::new("onboarding-worker", e))?
}
#[tauri::command]
pub fn pending_legacy_roots(
    state: State<'_, Desktop>,
) -> Result<Vec<product_core::migration::LegacyRoot>> {
    state.store.pending_legacy_roots()
}
#[tauri::command]
pub fn operation_result(id: String, state: State<'_, Desktop>) -> Result<OperationResult> {
    state.store.operation_result(&id)
}
/// Folder selection is a draft input; only save_library_roots persists it.
#[tauri::command]
pub async fn choose_library_root(
    space: Option<Space>,
    app: tauri::AppHandle,
) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let title = match space {
            Some(Space::Movie) => "请选择电影资料库",
            Some(Space::Tv) => "请选择电视剧资料库",
            None => "请选择资料库",
        };
        let Some(selected) = app.dialog().file().set_title(title).blocking_pick_folder() else {
            return Ok(None);
        };
        let selected = selected
            .into_path()
            .map_err(|e| AppError::new("invalid-path", e))?;
        let real = product_core::paths::checked(&selected)?;
        if !real.is_dir() {
            return Err(
                AppError::new("invalid-root", "Library root must be a directory")
                    .at(real.display()),
            );
        }
        let value = real
            .to_str()
            .ok_or_else(|| AppError::new("invalid-encoding", "Path must be Unicode"))?;
        Ok(Some(value.to_owned()))
    })
    .await
    .map_err(|e| AppError::new("worker-failed", e))?
}
#[tauri::command]
pub async fn save_library_roots(
    id: String,
    configuration: Configuration,
    app: tauri::AppHandle,
) -> Result<Configuration> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Desktop>();
        let before = state.store.configuration()?;
        let confirmed = state.store.library_roots_confirmed()?;
        state.store.confirm_library_roots(&id, configuration)?;
        let saved = state.store.configuration()?;
        state
            .reconcile
            .lock()
            .map_err(|e| AppError::new("reconcile-state", e))?
            .saved(&before, &saved, confirmed);
        app.emit("configuration-changed", &saved)
            .map_err(|e| AppError::new("configuration-event", e))?;
        Ok(saved)
    })
    .await
    .map_err(|e| AppError::new("worker-failed", e))?
}
#[tauri::command]
pub async fn test_library_root(path: String, app: tauri::AppHandle) -> Result<LibraryRootAccess> {
    tauri::async_runtime::spawn_blocking(move || {
        app.state::<Desktop>().store.test_library_root(&path)
    })
    .await
    .map_err(|e| AppError::new("worker-failed", e))?
}
#[tauri::command]
pub async fn add_library_root(
    app: tauri::AppHandle,
    space: Space,
    operation_id: String,
) -> Result<Option<Configuration>> {
    tauri::async_runtime::spawn_blocking(move || {
        match app.state::<Desktop>().store.operation_result(&operation_id) {
            Ok(OperationResult::Configuration(value)) => return Ok(Some(value)),
            Ok(_) => {
                return Err(AppError::new(
                    "operation-conflict",
                    "Operation ID belongs to another action",
                ))
            }
            Err(e) if e.code == "operation-not-found" => {}
            Err(e) => return Err(e),
        }
        let Some(selected) = app
            .dialog()
            .file()
            .set_title("选择媒体根目录 / Select media root")
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = selected
            .into_path()
            .map_err(|e| AppError::new("invalid-path", e))?;
        let real = product_core::paths::checked(&path)?;
        let path = real
            .to_str()
            .ok_or_else(|| AppError::new("invalid-encoding", "Path must be Unicode"))?
            .to_owned();
        let state = app.state::<Desktop>();
        let mut config = state.store.configuration()?;
        config.roots.push(LibraryRoot {
            id: product_core::hash(
                format!(
                    "{}:{path}",
                    if space == Space::Movie { "movie" } else { "tv" }
                )
                .as_bytes(),
            ),
            space,
            path,
        });
        state.store.configure(&operation_id, config).map(Some)
    })
    .await
    .map_err(|e| AppError::new("worker-failed", e))?
}
#[tauri::command]
pub fn scan_library(request: ScanRequest, state: State<'_, Desktop>) -> Result<Task> {
    state.store.submit(request)
}
#[tauri::command]
pub fn task_control(request: TaskControl, state: State<'_, Desktop>) -> Result<Task> {
    state.store.control(request)
}
#[tauri::command]
pub fn task_history(state: State<'_, Desktop>) -> Result<Vec<Task>> {
    state.store.tasks()
}
#[tauri::command]
pub fn task_job(id: String, state: State<'_, Desktop>) -> Result<product_core::task_log::TaskJob> {
    state.store.task_job(&id)
}
#[tauri::command]
pub fn job_history(state: State<'_, Desktop>) -> Result<Vec<product_core::task_log::TaskJob>> {
    state.store.job_history()
}
#[tauri::command]
pub fn task_result(id: String, state: State<'_, Desktop>) -> Result<Task> {
    state.store.task(&id)
}
#[tauri::command]
pub fn catalog(query: CatalogQuery, state: State<'_, Desktop>) -> Result<CatalogPage> {
    state.store.query(query)
}
#[tauri::command]
pub async fn inspector(id: String, app: tauri::AppHandle) -> Result<MediaItem> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.inspect_item(&id))
        .await
        .map_err(|e| AppError::new("inspector-worker", e))?
}
#[tauri::command]
pub async fn annotate_item(
    request: product_core::inspector::AnnotationRequest,
    app: tauri::AppHandle,
) -> Result<product_core::inspector::Annotation> {
    tauri::async_runtime::spawn_blocking(move || app.state::<Desktop>().store.annotate(request))
        .await
        .map_err(|e| AppError::new("inspector-worker", e))?
}
#[tauri::command]
pub async fn copy_text(value: String, app: tauri::AppHandle) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let (sender, receiver) = std::sync::mpsc::channel();
        app.run_on_main_thread(move || {
            use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
            use objc2_foundation::NSString;
            let pasteboard = NSPasteboard::generalPasteboard();
            pasteboard.clearContents();
            let copied = unsafe {
                pasteboard.setString_forType(&NSString::from_str(&value), NSPasteboardTypeString)
            };
            let _ = sender.send(copied);
        })
        .map_err(|e| AppError::new("clipboard-failed", e))?;
        let copied = tauri::async_runtime::spawn_blocking(move || receiver.recv())
            .await
            .map_err(|e| AppError::new("clipboard-worker", e))?
            .map_err(|e| AppError::new("clipboard-failed", e))?;
        if !copied {
            return Err(AppError::new(
                "clipboard-failed",
                "System clipboard did not accept text",
            ));
        }
        Ok(())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (value, app);
        Err(AppError::new(
            "clipboard-unavailable",
            "This platform uses its WebView clipboard",
        ))
    }
}
#[tauri::command]
pub async fn reveal_item(id: String, app: tauri::AppHandle) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Desktop>();
        let item = state.store.item(&id)?;
        let config = state.store.configuration()?;
        let root = config
            .roots
            .iter()
            .find(|r| r.id == item.root_id)
            .ok_or_else(|| AppError::new("invalid-root", "Root no longer configured"))?;
        let path = product_core::paths::within(
            std::path::Path::new(&root.path),
            std::path::Path::new(&item.path),
        )?;
        reveal(&path)
    })
    .await
    .map_err(|e| AppError::new("worker-failed", e))?
}
fn reveal(path: &std::path::Path) -> Result<()> {
    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status();
    #[cfg(target_os = "windows")]
    let status = std::process::Command::new("explorer.exe")
        .arg(format!("/select,{}", path.display()))
        .status();
    #[cfg(target_os = "linux")]
    let status = std::process::Command::new("xdg-open")
        .arg(path.parent().unwrap_or(path))
        .status();
    if !status
        .map_err(|e| AppError::new("reveal-failed", e).at(path.display()))?
        .success()
    {
        return Err(
            AppError::new("reveal-failed", "System file manager did not open").at(path.display()),
        );
    }
    Ok(())
}

#[tauri::command]
pub fn ui_state(state: State<'_, Desktop>) -> Result<product_core::ui::UiState> {
    state.store.ui_state()
}
#[tauri::command]
pub fn save_ui_state(
    id: String,
    value: product_core::ui::UiState,
    state: State<'_, Desktop>,
) -> Result<product_core::ui::UiReceipt> {
    state.store.save_ui_state(&id, value)
}
#[tauri::command]
pub fn browse(
    space: Space,
    view: product_core::ui::LibraryView,
    complete: Option<bool>,
    state: State<'_, Desktop>,
) -> Result<CatalogPage> {
    if complete.unwrap_or(false) {
        state.store.browse_complete(space, view)
    } else {
        state.store.browse(space, view)
    }
}

#[tauri::command]
pub fn tv_catalog(
    view: product_core::ui::LibraryView,
    state: State<'_, Desktop>,
) -> Result<product_core::tv::TvPage> {
    Ok(product_core::tv::page(&state.store.all_items()?, &view))
}
#[tauri::command]
pub fn tv_members(id: String, state: State<'_, Desktop>) -> Result<Vec<String>> {
    product_core::tv::members(&state.store.all_items()?, &id)
}

#[tauri::command]
pub fn catalog_members(
    space: Space,
    view: product_core::ui::LibraryView,
    state: State<'_, Desktop>,
) -> Result<Vec<String>> {
    state.store.catalog_members(space, view)
}

#[cfg(test)]
mod startup_tests {
    use super::*;
    #[test]
    fn late_root_confirmation_indexes_read_only_once_without_enabling_background_generation() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let movies = root.join("Movies");
        std::fs::create_dir(&movies).unwrap();
        let nfo = movies.join("sample.nfo");
        let bytes = b"\xef\xbb\xbf<movie>\r\n<title>Sample</title><tag>External</tag>\r\n</movie>";
        std::fs::write(&nfo, bytes).unwrap();
        let modified = std::fs::metadata(&nfo).unwrap().modified().unwrap();
        let store = Store::open(&root.join("workspace.sqlite")).unwrap();
        let mut schedule = ReconcileSchedule::default();
        assert!(schedule
            .poll(&store, "launch", Duration::from_secs(60))
            .unwrap()
            .is_empty());
        assert!(store.tasks().unwrap().is_empty());
        let before = store.configuration().unwrap();
        let confirmed = store.library_roots_confirmed().unwrap();
        let saved = store
            .confirm_library_roots(
                "confirm",
                Configuration {
                    roots: vec![LibraryRoot {
                        id: "movies".into(),
                        space: Space::Movie,
                        path: movies.to_str().unwrap().into(),
                    }],
                    ..before.clone()
                },
            )
            .unwrap();
        schedule.saved(&before, &saved, confirmed);
        let scans = schedule
            .poll(&store, "launch", Duration::from_secs(61))
            .unwrap();
        assert_eq!(scans.len(), 1);
        assert!(!scans[0].automatic && scans[0].batch.is_none());
        store.run_next(|| false, |_| {}).unwrap();
        assert_eq!(store.all_items().unwrap().len(), 1);
        assert_eq!(std::fs::read(&nfo).unwrap(), bytes);
        assert_eq!(
            std::fs::metadata(&nfo).unwrap().modified().unwrap(),
            modified
        );
        assert!(!store.automatic_status().unwrap().enabled);
        assert!(schedule
            .poll(&store, "launch", Duration::from_secs(62))
            .unwrap()
            .is_empty());
        // Replayed saves and locale-only revisions must not rescan unchanged roots.
        schedule.saved(&saved, &saved, true);
        let localized = store
            .confirm_library_roots(
                "language",
                Configuration {
                    locale: Locale::English,
                    ..saved.clone()
                },
            )
            .unwrap();
        schedule.saved(&saved, &localized, true);
        assert!(schedule
            .poll(&store, "launch", Duration::from_secs(63))
            .unwrap()
            .is_empty());
        assert_eq!(store.tasks().unwrap().len(), 1);
    }
    #[test]
    fn root_changes_wait_for_existing_work_and_only_scan_the_latest_confirmed_roots() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().canonicalize().unwrap();
        let store = Store::open(&root.join("workspace.sqlite")).unwrap();
        let mut before = store.configuration().unwrap();
        let mut schedule = ReconcileSchedule::default();
        for name in ["first", "second", "last"] {
            let path = root.join(name);
            std::fs::create_dir(&path).unwrap();
            std::fs::write(
                path.join("sample.nfo"),
                format!("<movie><title>{name}</title></movie>"),
            )
            .unwrap();
            let saved = store
                .confirm_library_roots(
                    name,
                    Configuration {
                        roots: vec![LibraryRoot {
                            id: name.into(),
                            space: Space::Movie,
                            path: path.to_str().unwrap().into(),
                        }],
                        ..before.clone()
                    },
                )
                .unwrap();
            schedule.saved(&before, &saved, name != "first");
            if name == "first" {
                store
                    .submit(ScanRequest {
                        operation_id: "existing".into(),
                        space: Space::Movie,
                        root_ids: vec!["first".into()],
                    })
                    .unwrap();
                store
                    .control(TaskControl {
                        operation_id: "pause".into(),
                        task_id: "existing".into(),
                        state: TaskState::Paused,
                    })
                    .unwrap();
                assert!(schedule
                    .poll(&store, "launch", Duration::from_secs(500))
                    .unwrap()
                    .is_empty());
                assert_eq!(
                    store
                        .confirm_library_roots(
                            "blocked-change",
                            Configuration {
                                roots: vec![],
                                ..saved.clone()
                            }
                        )
                        .unwrap_err()
                        .code,
                    "active-task"
                );
                store
                    .control(TaskControl {
                        operation_id: "cancel".into(),
                        task_id: "existing".into(),
                        state: TaskState::Cancelled,
                    })
                    .unwrap();
            }
            before = saved;
        }
        let scans = schedule
            .poll(&store, "launch", Duration::from_secs(501))
            .unwrap();
        assert_eq!(scans.len(), 1);
        assert_eq!(scans[0].roots[0].id, "last");
        store.run_next(|| false, |_| {}).unwrap();
        assert_eq!(store.all_items().unwrap()[0].title, "last");
        assert_eq!(store.tasks().unwrap().len(), 2);
        let cleared = store
            .confirm_library_roots(
                "clear",
                Configuration {
                    roots: vec![],
                    ..before.clone()
                },
            )
            .unwrap();
        schedule.saved(&before, &cleared, true);
        assert!(schedule
            .poll(&store, "launch", Duration::from_secs(502))
            .unwrap()
            .is_empty());
        assert!(store.all_items().unwrap().is_empty());
        assert_eq!(store.tasks().unwrap().len(), 2);
    }
    #[test]
    fn validation_and_new_platforms_never_discover_production_legacy_data() {
        let home = std::path::Path::new("/fixture-home");
        for (os, id) in [
            ("macos", "io.github.eric-hou1997.itm.validation"),
            ("windows", "itm"),
            ("linux", "itm"),
        ] {
            assert!(startup_sources(os, id, home).is_none());
        }
        let source = startup_sources("macos", "io.github.eric-hou1997.itm", home).unwrap();
        assert_eq!(
            source.manager,
            home.join("Library/Application Support/IMDb Tech Manager")
        );
        assert_eq!(
            source.engine,
            home.join("Library/Application Support/tmm-imdb-tech")
        );
    }
    #[test]
    fn failure_and_shutdown_gate_all_product_operations_but_keep_retry_and_exit() {
        for command in [
            "save_library_roots",
            "save_ai_settings",
            "automatic_apply",
            "scan_library",
            "generate_ai",
            "apply_specs",
            "save_ui_state",
            "update_install",
            "restore_language_packs",
        ] {
            assert!(!startup_command_allowed(false, false, command));
            assert!(startup_command_allowed(true, false, command));
            assert!(!startup_command_allowed(true, true, command));
        }
        assert!(startup_command_allowed(false, false, "configuration"));
        assert!(!startup_command_allowed(false, true, "configuration"));
        assert!(startup_command_allowed(false, true, "quit_probe"));
    }
}
