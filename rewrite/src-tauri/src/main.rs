#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod acquisition;
mod ai_jobs;
mod batches;
#[cfg(target_os = "macos")]
mod credential_process;
mod credentials;
mod desktop;
mod imdb_webview;
mod languages;
mod lifecycle;
mod native_dialogs;
mod public_links;
mod update;
mod writing;
use product_core::services::CredentialStore;
use serde_json::{json, Value};
use std::{
    io::{Read, Write},
    time::Duration,
};
use tauri::{
    menu::{Menu, MenuItem, Submenu},
    tray::TrayIconBuilder,
    Manager,
};
use tauri_plugin_dialog::DialogExt;
const PRODUCT: &str = "ITM";

#[tauri::command]
fn runtime_probe(nonce: String) -> Result<Value, String> {
    if nonce.len() > 128 {
        return Err("invalid-nonce".into());
    }
    Ok(
        json!({"product": PRODUCT, "os": std::env::consts::OS, "arch": std::env::consts::ARCH, "echo": nonce}),
    )
}
fn report_event(event: &str) -> Result<(), String> {
    if let Some(path) = std::env::var_os("REWRITE_PROBE_REPORT") {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        writeln!(file, "{}", json!({"product":PRODUCT,"event":event,"os":std::env::consts::OS,"arch":std::env::consts::ARCH})).map_err(|e|e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
    }
    Ok(())
}
#[tauri::command]
fn frontend_ready(app: tauri::AppHandle) -> Result<(), String> {
    report_event("frontend-mounted-ipc-roundtrip")?;
    update::frontend_healthy(&app).map_err(|e| e.to_string())?;
    if std::env::var_os("REWRITE_PROBE_AUTOCLOSE").is_some() {
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_secs(2));
            app.exit(0);
        });
    }
    Ok(())
}
#[tauri::command]
async fn directory_probe(app: tauri::AppHandle) -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(selected) = app.dialog().file().set_title("只读检查目录 / Read-only directory check").blocking_pick_folder() else { return Ok(json!({"status":"cancelled"})); };
        let path=selected.into_path().map_err(|e|e.to_string())?;
        let mut items=std::fs::read_dir(&path).map_err(|e|format!("directory-read: {e}"))?;
        let count=items.by_ref().take(1000).collect::<Result<Vec<_>,_>>().map_err(|e|e.to_string())?.len();
        Ok(json!({"status":"directory-readable","entries_sampled":count,"limit":1000,"modified":false,"emby_write_permission":"unverified"}))
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
fn storage_probe(app: tauri::AppHandle) -> Result<Value, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let work = tempfile::tempdir_in(dir).map_err(|e| e.to_string())?;
    let mut candidate = tempfile::NamedTempFile::new_in(work.path()).map_err(|e| e.to_string())?;
    candidate
        .write_all(b"validation-only")
        .map_err(|e| e.to_string())?;
    candidate.as_file().sync_all().map_err(|e| e.to_string())?;
    let destination = work.path().join("probe.txt");
    candidate.persist(&destination).map_err(|e| e.to_string())?;
    let mut data = String::new();
    std::fs::File::open(destination)
        .map_err(|e| e.to_string())?
        .read_to_string(&mut data)
        .map_err(|e| e.to_string())?;
    if data != "validation-only" {
        return Err("readback-mismatch".into());
    }
    work.close().map_err(|e| format!("cleanup-failed: {e}"))?;
    Ok(
        json!({"status":"write-sync-read-cleanup-passed","scope":"validation-app-data-only","nfo_transaction":"not-tested"}),
    )
}
#[tauri::command]
async fn credential_probe() -> Result<Value, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let service=format!("io.github.eric-hou1997.{}.validation",PRODUCT.to_lowercase());
        let account=format!("probe-{}-{}",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(|e|e.to_string())?.as_nanos());
        let entry=credentials::NativeCredentials::new(service);
        entry.put(&account,"non-secret-validation-sentinel").map_err(|e|format!("credential-write: {e}"))?;
        let read=entry.get(&account);
        let cleanup=entry.delete(&account);
        cleanup.map_err(|e|format!("credential-cleanup-failed: {e}"))?;
        if read.map_err(|e|format!("credential-read: {e}"))?.as_deref() != Some("non-secret-validation-sentinel") {return Err("credential-roundtrip-mismatch".into());}
        if !matches!(entry.get(&account),Ok(None)) {return Err("credential-delete-unverified".into());}
        Ok(json!({"status":"native-store-roundtrip-and-delete-passed","production_credentials_accessed":false}))
    }).await.map_err(|e|e.to_string())?
}
#[tauri::command]
async fn network_probe() -> Result<Value, String> {
    let url = if PRODUCT == "ITM" {
        "https://www.imdb.com/title/tt0064757/technical/"
    } else {
        "https://tauri.app/"
    };
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("network-request: {e}"))?;
    Ok(
        json!({"status":response.status().as_u16(),"http_received":true,"imdb_acquisition_verified":false,"note":"HTTP response alone does not prove usable Technical Specs"}),
    )
}
#[tauri::command]
fn quit_probe(app: tauri::AppHandle) {
    app.exit(0);
}
fn restore(app: &tauri::AppHandle) {
    if let Some(w) = app.get_webview_window("main") {
        for result in [w.show(), w.unminimize(), w.set_focus()] {
            if let Err(e) = result {
                eprintln!("window-restore: {e}");
            }
        }
    }
}
fn main() {
    #[cfg(target_os = "macos")]
    if let Some(code) = credential_process::entry() {
        std::process::exit(code);
    }
    let commands: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
        languages::language_status,
        languages::choose_language,
        languages::restore_language_packs,
        native_dialogs::native_dialog,
        acquisition::fetch_specs,
        acquisition::fetch_record,
        acquisition::fetch_history,
        acquisition::imdb_cache_status,
        acquisition::maintain_imdb_cache,
        acquisition::cancel_fetch,
        acquisition::preview_source,
        writing::preview_specs,
        writing::preview_restore_specs,
        writing::preview_tags,
        writing::preview_rules,
        ai_jobs::ai_settings,
        ai_jobs::save_ai_settings,
        ai_jobs::ai_settings_receipt,
        ai_jobs::generate_ai,
        ai_jobs::ai_record,
        ai_jobs::cancel_ai,
        ai_jobs::ai_history,
        ai_jobs::ai_runtime,
        ai_jobs::ai_failure_items,
        ai_jobs::resume_ai_runtime,
        ai_jobs::test_ai_connection,
        ai_jobs::preview_ai,
        writing::apply_specs,
        writing::preview_undo,
        writing::write_history,
        writing::legacy_undo_entries,
        writing::preview_legacy_undo,
        runtime_probe,
        lifecycle::lifecycle_status,
        lifecycle::lifecycle_apply,
        lifecycle::background_window,
        frontend_ready,
        directory_probe,
        storage_probe,
        credential_probe,
        network_probe,
        public_links::open_product_link,
        quit_probe,
        update::update_identity,
        update::update_status,
        update::update_check,
        update::update_install,
        update::update_cancel,
        desktop::configuration,
        desktop::pending_legacy_roots,
        desktop::onboarding_info,
        desktop::automatic_status,
        desktop::automatic_apply,
        desktop::operation_result,
        desktop::add_library_root,
        desktop::choose_library_root,
        desktop::save_library_roots,
        desktop::test_library_root,
        batches::plan_batch,
        batches::adopt_preview,
        batches::preflight_items,
        batches::batch_detail,
        batches::apply_batch_item,
        batches::approve_batch_scope,
        desktop::scan_library,
        desktop::task_control,
        desktop::task_history,
        desktop::task_job,
        desktop::job_history,
        desktop::task_result,
        desktop::catalog,
        desktop::ui_state,
        desktop::save_ui_state,
        desktop::browse,
        desktop::catalog_members,
        desktop::tv_catalog,
        desktop::tv_members,
        desktop::inspector,
        desktop::annotate_item,
        desktop::copy_text,
        desktop::reveal_item
    ];
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            restore(app);
            if let Err(e) = report_event("second-launch-forwarded") {
                eprintln!("{e}");
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .on_window_event(|window, event| {
            if window.label() == "main" {
                if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    lifecycle::close_requested(window.app_handle());
                }
            }
        })
        .plugin(tauri_plugin_updater::Builder::new().build())
        .invoke_handler(move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
            let app = invoke.message.webview().app_handle().clone();
            let state = app.state::<desktop::Desktop>();
            if !desktop::startup_command_allowed(
                state.ready(),
                state.stopping(),
                invoke.message.command(),
            ) {
                invoke.resolver.reject(product_core::AppError::new(
                    "startup-pending",
                    "启动未完成，请重新连接；尚未启动后台任务",
                ));
                return true;
            }
            commands(invoke)
        })
        .setup(|app| {
            app.manage(desktop::Desktop::start(app.handle())?);
            app.manage(lifecycle::Lifecycle::default());
            app.manage(update::Updates::default());
            app.manage(languages::Languages::default());
            let show = MenuItem::with_id(app, "show", "显示窗口 / Show", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "退出 / Quit", true, Some("CmdOrCtrl+Q"))?;
            // macOS menu bars require top-level submenus. A flat tray menu
            // cannot also serve as the menu bar: its accelerators stay inactive.
            #[cfg(target_os = "macos")]
            let about = tauri::menu::PredefinedMenuItem::about(
                app,
                Some("关于 IMDb Tech Manager"),
                Some(tauri::menu::AboutMetadata {
                    name: Some("IMDb Tech Manager".into()),
                    version: Some(env!("CARGO_PKG_VERSION").into()),
                    authors: Some(vec!["侯雁泽".into()]),
                    license: Some("Apache License 2.0".into()),
                    ..Default::default()
                }),
            )?;
            #[cfg(target_os = "macos")]
            let separator = tauri::menu::PredefinedMenuItem::separator(app)?;
            #[cfg(target_os = "macos")]
            let application =
                Submenu::with_items(app, "IMDb Tech Manager", true, &[&about, &separator, &quit])?;
            #[cfg(not(target_os = "macos"))]
            let application = Submenu::with_items(app, "IMDb Tech Manager", true, &[&show, &quit])?;
            app.set_menu(Menu::with_items(app, &[&application])?)?;
            let tray_show = MenuItem::with_id(app, "show", "显示窗口 / Show", true, None::<&str>)?;
            let tray_quit = MenuItem::with_id(app, "quit", "退出 / Quit", true, None::<&str>)?;
            let tray_menu = Menu::with_items(app, &[&tray_show, &tray_quit])?;
            app.manage(languages::LanguageMenus(
                vec![show, tray_show],
                vec![quit, tray_quit],
                #[cfg(target_os = "macos")]
                about,
            ));
            let language = languages::snapshot(app.handle(), &app.state::<languages::Languages>())?;
            languages::synchronize_menus(app.handle(), &language)?;
            app.on_menu_event(|app, event| match event.id().as_ref() {
                "show" => restore(app),
                "quit" => app.exit(0),
                _ => {}
            });
            let mut tray = TrayIconBuilder::new()
                .menu(&tray_menu)
                .tooltip("IMDb Tech Manager");
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            match tray.build(app) {
                Ok(_) => app
                    .state::<lifecycle::Lifecycle>()
                    .tray
                    .store(true, std::sync::atomic::Ordering::SeqCst),
                Err(error) => eprintln!("tray-unavailable: {error}"),
            }
            lifecycle::first_window(app.handle())?;
            report_event("native-setup-complete")?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("validation application setup failed");
    app.run(|handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = &event {
            if !handle
                .state::<lifecycle::Lifecycle>()
                .allow_exit
                .load(std::sync::atomic::Ordering::SeqCst)
            {
                api.prevent_exit();
                lifecycle::request_exit(handle);
            }
        }
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = &event {
            restore(handle);
        }
        if matches!(event, tauri::RunEvent::Exit) {
            languages::shutdown(handle);
            handle.state::<desktop::Desktop>().shutdown();
            if let Err(e) = report_event("process-exit") {
                eprintln!("{e}");
            }
        }
    });
}

fn prepare_update_exit(app: &tauri::AppHandle) -> product_core::Result<()> {
    languages::shutdown(app);
    app.state::<desktop::Desktop>().shutdown();
    app.state::<lifecycle::Lifecycle>()
        .allow_exit
        .store(true, std::sync::atomic::Ordering::SeqCst);
    Ok(())
}
