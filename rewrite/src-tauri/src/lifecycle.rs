use crate::desktop::Desktop;
use product_core::{
    lifecycle::{self, Autostart, CloseAction, Settings, SettingsOperation},
    *,
};
use serde::Serialize;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Mutex,
};
use tauri::{Emitter, Manager};
#[cfg(windows)]
use tauri_plugin_autostart::ManagerExt;
struct Native<'a>(&'a tauri::AppHandle);
#[cfg(not(windows))]
impl Native<'_> {
    fn file(&self) -> Result<product_core::startup_file::StartupFile> {
        #[cfg(target_os = "macos")]
        let directory = self
            .0
            .path()
            .home_dir()
            .map_err(|e| AppError::new("autostart-directory", e))?
            .join("Library/LaunchAgents");
        #[cfg(target_os = "linux")]
        let directory = self
            .0
            .path()
            .config_dir()
            .map_err(|e| AppError::new("autostart-directory", e))?
            .join("autostart");
        let mut exe = std::env::current_exe().map_err(|e| AppError::new("autostart-path", e))?;
        #[cfg(target_os = "linux")]
        if let Some(path) = self.0.env().appimage {
            exe = path.into();
        }
        #[cfg(not(target_os = "linux"))]
        let _ = &mut exe;
        product_core::startup_file::StartupFile::new(
            std::env::consts::OS,
            &directory,
            &self.0.config().identifier,
            &exe,
            &self.0.package_info().name,
        )
    }
}
impl Autostart for Native<'_> {
    fn enabled(&self) -> Result<bool> {
        #[cfg(windows)]
        {
            self.0
                .autolaunch()
                .is_enabled()
                .map_err(|e| AppError::new("autostart-read", e))
        }
        #[cfg(not(windows))]
        {
            self.file()?.enabled()
        }
    }
    fn set(&self, enabled: bool) -> Result<()> {
        #[cfg(windows)]
        {
            let manager = self.0.autolaunch();
            if enabled {
                manager.enable()
            } else {
                manager.disable()
            }
            .map_err(|e| AppError::new("autostart-change", e))
        }
        #[cfg(not(windows))]
        {
            self.file()?.set(enabled)
        }
    }
}
#[derive(Default)]
pub struct Lifecycle {
    gate: Mutex<()>,
    pub allow_exit: AtomicBool,
    closing: AtomicBool,
    pub tray: AtomicBool,
    error: Mutex<Option<AppError>>,
}
#[derive(Serialize)]
pub struct LifecycleStatus {
    settings: Settings,
    native_autostart: Option<bool>,
    background_mode: String,
    tray_available: bool,
    closing: bool,
    error: Option<AppError>,
}
pub fn initialize(app: &tauri::AppHandle) -> Result<()> {
    let result: Result<()> = (|| {
        lifecycle::reconcile(&app.state::<Desktop>().store, &Native(app))?;
        #[cfg(target_os = "macos")]
        handoff_legacy_login(app)?;
        Ok(())
    })();
    if let Err(error) = result {
        set_error(app, error.clone());
        return Err(error);
    }
    *app.state::<Lifecycle>()
        .error
        .lock()
        .map_err(|e| AppError::new("lifecycle-state", e))? = None;
    Ok(())
}
#[cfg(target_os = "macos")]
fn handoff_legacy_login(app: &tauri::AppHandle) -> Result<()> {
    let desktop = app.state::<Desktop>();
    let Some(sources) = desktop.legacy_sources() else {
        return Ok(());
    };
    if desktop.stopping() {
        return Err(AppError::new("startup-cancelled", "Application is closing"));
    }
    Desktop::ensure_legacy_idle(sources)?;
    let directory = app
        .path()
        .home_dir()
        .map_err(|e| AppError::new("legacy-login-directory", e))?
        .join("Library/LaunchAgents");
    let mut registrations = vec![];
    for label in [
        "com.local.imdb-tech-manager",
        "com.local.imdb-tech-manager.app",
    ] {
        if let Some(value) = product_core::legacy_startup::Registration::read(
            &directory.join(format!("{label}.plist")),
            label,
        )? {
            registrations.push(value);
        }
    }
    let pending = desktop
        .store
        .preferences("legacy-login-handoff-pending")?
        .as_bool()
        .unwrap_or(false);
    if registrations.is_empty() && !pending {
        return Ok(());
    }
    // Use the imported explicit preference; an old Agent setting never enables
    // App login startup. Unbundled development identities do not reach here.
    let mut desired = desktop.store.lifecycle_settings()?;
    if desktop
        .store
        .preferences("lifecycle-settings")?
        .get("revision")
        .is_none()
        && registrations
            .iter()
            .any(|r| r.label == "com.local.imdb-tech-manager.app")
    {
        desired.launch_at_login = true;
        desktop
            .store
            .save_preference("lifecycle-settings", &serde_json::to_value(&desired)?)?;
    }
    Native(app).enabled()?;
    let uid = std::process::Command::new("/usr/bin/id")
        .arg("-u")
        .output()
        .map_err(|e| AppError::new("legacy-login-user", e))?;
    let uid = String::from_utf8(uid.stdout).map_err(|e| AppError::new("legacy-login-user", e))?;
    let uid = uid
        .trim()
        .parse::<u32>()
        .map_err(|e| AppError::new("legacy-login-user", e))?;
    let archive = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::new("legacy-login-archive", e))?
        .join("legacy-login-items");
    // Durable pending intent survives a crash after old items are retired but
    // before the new registration exists. Never leave two login owners enabled.
    desktop
        .store
        .save_preference("legacy-login-handoff-pending", &serde_json::json!(true))?;
    for registration in registrations {
        if desktop.stopping() {
            return Err(AppError::new("startup-cancelled", "Application is closing"));
        }
        Desktop::ensure_legacy_idle(sources)?;
        registration.unchanged()?;
        registration.backup(&archive)?;
        let service = format!("gui/{uid}/{}", registration.label);
        let output = std::process::Command::new("/bin/launchctl")
            .args(["print", &service])
            .output()
            .map_err(|e| AppError::new("legacy-login-state", e))?;
        let exists = output.status.success();
        if exists
            && !String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim().strip_prefix("path = ") == registration.path.to_str())
        {
            return Err(AppError::new(
                "legacy-login-owner",
                "已加载的旧登录服务与文件归属不一致，未撤除",
            ));
        }
        if exists
            && String::from_utf8_lossy(&output.stdout)
                .lines()
                .any(|line| line.trim().starts_with("pid = "))
        {
            return Err(AppError::new(
                "legacy-runtime-active",
                "旧登录服务仍在运行，请从旧版正常退出后重试",
            ));
        }
        if !exists && !String::from_utf8_lossy(&output.stderr).contains("Could not find service") {
            return Err(AppError::new(
                "legacy-login-state",
                "无法确认旧登录服务状态，未撤除旧登录项",
            ));
        }
        let output = std::process::Command::new("/bin/launchctl")
            .args(["disable", &service])
            .output()
            .map_err(|e| AppError::new("legacy-login-disable", e))?;
        if !output.status.success() {
            return Err(AppError::new(
                "legacy-login-disable",
                "旧登录项停用失败，请重试",
            ));
        }
        if exists {
            let output = std::process::Command::new("/bin/launchctl")
                .args(["bootout", &service])
                .output()
                .map_err(|e| AppError::new("legacy-login-stop", e))?;
            if !output.status.success() {
                return Err(AppError::new(
                    "legacy-login-stop",
                    "旧登录服务撤除未确认，请重试",
                ));
            }
        }
        registration.archive(&archive)?;
    }
    Desktop::ensure_legacy_idle(sources)?;
    if desktop.stopping() {
        return Err(AppError::new("startup-cancelled", "Application is closing"));
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| AppError::new("legacy-login-clock", e))?
        .as_nanos();
    let id = format!("legacy-login-{nonce}");
    lifecycle::apply(&desktop.store, &id, desired, &Native(app))?;
    desktop
        .store
        .save_preference("legacy-login-handoff-pending", &serde_json::json!(false))?;
    Ok(())
}
fn set_error(app: &tauri::AppHandle, error: AppError) {
    if let Ok(mut current) = app.state::<Lifecycle>().error.lock() {
        *current = Some(error.clone());
    }
    let _ = app.emit("lifecycle-error", error);
}
#[tauri::command]
pub fn lifecycle_status(app: tauri::AppHandle) -> Result<LifecycleStatus> {
    let state = app.state::<Lifecycle>();
    let settings = app.state::<Desktop>().store.lifecycle_settings()?;
    let native = Native(&app).enabled();
    let mut error = state
        .error
        .lock()
        .map_err(|e| AppError::new("lifecycle-state", e))?
        .clone();
    if let Err(e) = &native {
        error = Some(e.clone());
    }
    Ok(LifecycleStatus {
        settings,
        native_autostart: native.ok(),
        background_mode: if cfg!(target_os = "linux") {
            "minimize"
        } else {
            "hide"
        }
        .into(),
        tray_available: state.tray.load(Ordering::SeqCst),
        closing: state.closing.load(Ordering::SeqCst),
        error,
    })
}
#[tauri::command]
pub async fn lifecycle_apply(
    id: String,
    settings: Settings,
    app: tauri::AppHandle,
) -> Result<SettingsOperation> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Lifecycle>();
        let _guard = state
            .gate
            .lock()
            .map_err(|e| AppError::new("lifecycle-state", e))?;
        lifecycle::reconcile(&app.state::<Desktop>().store, &Native(&app))?;
        let value = lifecycle::apply(&app.state::<Desktop>().store, &id, settings, &Native(&app))?;
        *state
            .error
            .lock()
            .map_err(|e| AppError::new("lifecycle-state", e))? = None;
        Ok(value)
    })
    .await
    .map_err(|e| AppError::new("lifecycle-worker", e))?
}
#[tauri::command]
pub fn background_window(app: tauri::AppHandle) -> Result<()> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::new("window-unavailable", "Main window is unavailable"))?;
    // Linux desktops can accept a tray item without displaying it. Keep a taskbar
    // window as the guaranteed equivalent restore entry on every Linux desktop.
    if cfg!(target_os = "linux") {
        window.minimize()
    } else {
        window.hide()
    }
    .map_err(|e| AppError::new("window-background", e))
}
pub fn first_window(app: &tauri::AppHandle) -> Result<()> {
    let settings = app.state::<Desktop>().store.lifecycle_settings()?;
    if settings.start_hidden && std::env::args().any(|a| a == "--background") {
        background_window(app.clone())?;
    }
    Ok(())
}
pub fn close_requested(app: &tauri::AppHandle) {
    crate::native_dialogs::cancel(app);
    match app.state::<Desktop>().store.lifecycle_settings() {
        Ok(settings) if settings.close_action == CloseAction::Background => {
            if let Err(error) = background_window(app.clone()) {
                set_error(app, error);
                crate::restore(app);
            }
        }
        Ok(_) => request_exit(app),
        Err(error) => {
            set_error(app, error);
            crate::restore(app);
        }
    }
}
pub fn request_exit(app: &tauri::AppHandle) {
    crate::native_dialogs::cancel(app);
    let state = app.state::<Lifecycle>();
    if state.closing.swap(true, Ordering::SeqCst) {
        return;
    }
    let handle = app.clone();
    let _ = app.emit("lifecycle-closing", true);
    tauri::async_runtime::spawn_blocking(move || match crate::prepare_update_exit(&handle) {
        Ok(()) => {
            handle
                .state::<Lifecycle>()
                .allow_exit
                .store(true, Ordering::SeqCst);
            handle.exit(0);
        }
        Err(error) => {
            let state = handle.state::<Lifecycle>();
            state.closing.store(false, Ordering::SeqCst);
            state.allow_exit.store(false, Ordering::SeqCst);
            set_error(&handle, error);
            crate::restore(&handle);
        }
    });
}
