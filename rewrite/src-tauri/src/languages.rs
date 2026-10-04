use crate::desktop::Desktop;
use product_core::{
    languages::{self as core, stable_id, Catalog, LanguageSnapshot},
    AppError, Locale, Result,
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Mutex,
    time::Duration,
};
use tauri::{Emitter, Manager, State};
#[derive(Default)]
pub struct Languages {
    busy: tauri::async_runtime::Mutex<()>,
    downloading: Mutex<BTreeSet<String>>,
    restore_lock: tauri::async_runtime::Mutex<()>,
    restore_attempts: Mutex<BTreeSet<String>>,
    errors: Mutex<BTreeMap<String, String>>,
    stopped: std::sync::atomic::AtomicBool,
}
pub struct LanguageMenus(
    pub Vec<tauri::menu::MenuItem<tauri::Wry>>,
    pub Vec<tauri::menu::MenuItem<tauri::Wry>>,
    #[cfg(target_os = "macos")] pub tauri::menu::PredefinedMenuItem<tauri::Wry>,
);
struct DownloadFlag<'a> {
    values: &'a Mutex<BTreeSet<String>>,
    code: String,
}
impl<'a> DownloadFlag<'a> {
    fn begin(values: &'a Mutex<BTreeSet<String>>, code: &str) -> Result<Self> {
        if !values.lock().map_err(error)?.insert(code.into()) {
            return Err(AppError::new("language-downloading", "该语言包正在下载"));
        }
        Ok(Self {
            values,
            code: code.into(),
        })
    }
}
impl Drop for DownloadFlag<'_> {
    fn drop(&mut self) {
        if let Ok(mut values) = self.values.lock() {
            values.remove(&self.code);
        }
    }
}
pub fn shutdown(app: &tauri::AppHandle) {
    if let Some(state) = app.try_state::<Languages>() {
        state
            .stopped
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
}
fn running(state: &Languages) -> Result<()> {
    if state.stopped.load(std::sync::atomic::Ordering::SeqCst) {
        Err(AppError::new(
            "language-stopped",
            "应用正在退出，语言包操作已停止",
        ))
    } else {
        Ok(())
    }
}
fn error(message: impl ToString) -> AppError {
    AppError::new("language-install", message)
}
pub(crate) fn snapshot(app: &tauri::AppHandle, state: &Languages) -> Result<LanguageSnapshot> {
    let catalog = Catalog::embedded()?;
    let desktop = app.state::<Desktop>();
    let locale = desktop.store.configuration()?.locale;
    let mut options = core::options();
    let mut web_messages = BTreeMap::new();
    let mut native_messages = BTreeMap::new();
    let downloading = state.downloading.lock().map_err(error)?.clone();
    let errors = state.errors.lock().map_err(error)?;
    for option in options.iter_mut().filter(|o| !o.built_in) {
        match desktop.store.legacy_language_pack(&catalog, &option.code) {
            Ok(Some(sections)) => {
                option.installed = true;
                option.state = "installed".into();
                if option.code == locale.as_str() {
                    web_messages = sections["web"].clone();
                    native_messages = sections["native"].clone();
                }
            }
            Ok(None) => {}
            Err(e) => {
                option.state = "failed".into();
                option.error = Some(e.message);
            }
        }
        if downloading.contains(&option.code) {
            option.state = "downloading".into();
        } else if !option.installed {
            if let Some(message) = errors.get(&option.code) {
                option.state = "failed".into();
                option.error = Some(message.clone());
            }
        }
    }
    Ok(LanguageSnapshot {
        locale,
        options,
        web_messages,
        native_messages,
    })
}
fn presentation_text(
    snapshot: &LanguageSnapshot,
    chinese: &str,
    english: &str,
    messages: &BTreeMap<String, String>,
) -> Result<String> {
    match snapshot.locale {
        Locale::Simplified => Ok(chinese.into()),
        Locale::Traditional => {
            let baseline: serde_json::Value =
                serde_json::from_str(include_str!("../../src/assets/baseline-languages.json"))?;
            let pairs: Vec<(String, String)> =
                serde_json::from_value(baseline["native_traditional"].clone())?;
            Ok(pairs
                .into_iter()
                .fold(chinese.to_owned(), |text, (from, to)| {
                    text.replace(&from, &to)
                }))
        }
        _ => Ok(messages
            .get(&stable_id(english))
            .cloned()
            .unwrap_or_else(|| english.into())),
    }
}
pub(crate) fn native_message(
    snapshot: &LanguageSnapshot,
    chinese: &str,
    english: &str,
) -> Result<String> {
    presentation_text(snapshot, chinese, english, &snapshot.native_messages)
}
pub fn synchronize_menus(app: &tauri::AppHandle, snapshot: &LanguageSnapshot) -> Result<()> {
    let Some(menus) = app.try_state::<LanguageMenus>() else {
        return Ok(());
    };
    let labels = menu_labels(snapshot);
    for item in &menus.0 {
        item.set_text(&labels.0).map_err(error)?;
    }
    for item in &menus.1 {
        item.set_text(&labels.1).map_err(error)?;
    }
    #[cfg(target_os = "macos")]
    menus
        .2
        .set_text(native_message(
            snapshot,
            "关于 IMDb Tech Manager",
            "About IMDb Tech Manager",
        )?)
        .map_err(error)?;
    Ok(())
}
fn menu_labels(snapshot: &LanguageSnapshot) -> (String, String) {
    let quit = native_message(snapshot, "退出 IMDb Tech Manager", "Quit IMDb Tech Manager")
        .unwrap_or_else(|_| "Quit IMDb Tech Manager".into());
    let show = match snapshot.locale {
        Locale::Simplified => "显示窗口",
        Locale::Traditional => "顯示視窗",
        _ => "Show",
    }
    .into();
    (show, quit)
}

#[tauri::command]
pub async fn language_status(app: tauri::AppHandle) -> Result<LanguageSnapshot> {
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<Languages>();
        let result = snapshot(&app, &state)?;
        synchronize_menus(&app, &result)?;
        Ok(result)
    })
    .await
    .map_err(error)?
}
#[tauri::command]
pub async fn choose_language(
    locale: Locale,
    app: tauri::AppHandle,
    state: State<'_, Languages>,
) -> Result<LanguageSnapshot> {
    running(&state)?;
    let _guard = state
        .busy
        .try_lock()
        .map_err(|_| error("该语言包正在下载"))?;
    let catalog = Catalog::embedded()?;
    let code = locale.as_str();
    let external = catalog.languages.contains_key(code);
    if external {
        let desktop = app.state::<Desktop>();
        ensure_pack(
            &desktop.store,
            &catalog,
            code,
            &state,
            download(&catalog, code),
            || app.emit("language-state-changed", ()).map_err(error),
        )
        .await?;
    }
    running(&state)?;
    let config = app
        .state::<Desktop>()
        .store
        .select_ui_language(&catalog, locale)?;
    let result = snapshot(&app, &state)?;
    synchronize_menus(&app, &result)?;
    app.emit("configuration-changed", config).map_err(error)?;
    Ok(result)
}

async fn ensure_pack(
    store: &product_core::store::Store,
    catalog: &Catalog,
    code: &str,
    state: &Languages,
    transfer: impl std::future::Future<Output = Result<Vec<u8>>>,
    notify: impl Fn() -> Result<()>,
) -> Result<()> {
    if store
        .current_language_pack(catalog, code)
        .is_ok_and(|p| p.is_some())
    {
        return Ok(());
    }
    running(state)?;
    let flag = DownloadFlag::begin(&state.downloading, code)?;
    notify()?;
    let mut transfer = std::pin::pin!(transfer);
    let mut tick = tokio::time::interval(Duration::from_millis(100));
    let result = std::future::poll_fn(|cx| {
        if let Err(failure) = running(state) {
            return std::task::Poll::Ready(Err(failure));
        }
        while tick.poll_tick(cx).is_ready() {}
        transfer.as_mut().poll(cx)
    })
    .await
    .and_then(|bytes| {
        running(state)?;
        store
            .install_legacy_language_pack(catalog, code, &bytes)
            .map(|_| ())
    });
    drop(flag);
    match &result {
        Ok(()) => {
            state.errors.lock().map_err(error)?.remove(code);
        }
        Err(failure) => {
            state
                .errors
                .lock()
                .map_err(error)?
                .insert(code.into(), failure.message.clone());
        }
    }
    notify()?;
    result
}
#[tauri::command]
pub async fn restore_language_packs(
    app: tauri::AppHandle,
    state: State<'_, Languages>,
) -> Result<LanguageSnapshot> {
    let Ok(_guard) = state.restore_lock.try_lock() else {
        return language_status(app).await;
    };
    let catalog = Catalog::embedded()?;
    let store = app.state::<Desktop>().store.clone();
    for code in store.language_restore_candidates(&catalog)? {
        running(&state)?;
        if state.downloading.lock().map_err(error)?.contains(&code) {
            continue;
        }
        let key = format!("{code}:{}", catalog.descriptor(&code)?.sha256);
        if !state
            .restore_attempts
            .lock()
            .map_err(error)?
            .insert(key.clone())
        {
            continue;
        }
        if let Err(failure) = ensure_pack(
            &store,
            &catalog,
            &code,
            &state,
            download(&catalog, &code),
            || app.emit("language-state-changed", ()).map_err(error),
        )
        .await
        {
            if failure.code == "language-downloading" {
                state.restore_attempts.lock().map_err(error)?.remove(&key);
            } else {
                eprintln!("language-pack-restore: {code}: {failure}");
            }
        }
    }
    language_status(app).await
}

async fn download(catalog: &Catalog, locale: &str) -> Result<Vec<u8>> {
    let client = reqwest::Client::builder()
        .https_only(true)
        .timeout(Duration::from_secs(90))
        .user_agent(concat!("IMDb-Tech-Manager/", env!("CARGO_PKG_VERSION")))
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if attempt.previous().len() >= 10 || !core::official_redirect(attempt.url()) {
                attempt.error("语言包下载重定向到非官方地址或次数过多，已拒绝")
            } else {
                attempt.follow()
            }
        }))
        .build()
        .map_err(error)?;
    let mut response = client
        .get(catalog.url(locale)?)
        .send()
        .await
        .map_err(|e| error(format!("语言包下载失败：{e}")))?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(error(format!(
            "语言包下载失败（HTTP {}）",
            response.status().as_u16()
        )));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(error)? {
        if bytes.len() + chunk.len() > core::MAX_BYTES {
            return Err(error("语言包过大，已拒绝安装"));
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_download_releases_its_owner_and_cannot_install_or_change_locale() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let store =
                product_core::store::Store::open(&temp.path().join("state.sqlite")).unwrap();
            let catalog = Catalog::embedded().unwrap();
            let state = Languages::default();
            let mut transfer = std::pin::pin!(ensure_pack(
                &store,
                &catalog,
                "fr-FR",
                &state,
                std::future::pending(),
                || Ok(())
            ));
            let mut cx = std::task::Context::from_waker(std::task::Waker::noop());
            assert!(std::future::Future::poll(transfer.as_mut(), &mut cx).is_pending());
            assert!(state.downloading.lock().unwrap().contains("fr-FR"));
            state
                .stopped
                .store(true, std::sync::atomic::Ordering::SeqCst);
            assert_eq!(transfer.await.unwrap_err().code, "language-stopped");
            assert!(state.downloading.lock().unwrap().is_empty());
            assert_eq!(store.configuration().unwrap().locale, Locale::Simplified);
            assert!(store
                .legacy_language_pack(&catalog, "fr-FR")
                .unwrap()
                .is_none());
        });
    }
    #[test]
    fn failed_download_preserves_language_and_clears_the_spinner() {
        tauri::async_runtime::block_on(async {
            let temp = tempfile::tempdir().unwrap();
            let store =
                product_core::store::Store::open(&temp.path().join("state.sqlite")).unwrap();
            let catalog = Catalog::embedded().unwrap();
            let state = Languages::default();
            assert!(ensure_pack(
                &store,
                &catalog,
                "fr-FR",
                &state,
                async { Err(error("offline fixture")) },
                || Ok(())
            )
            .await
            .is_err());
            assert!(state.downloading.lock().unwrap().is_empty());
            assert_eq!(state.errors.lock().unwrap()["fr-FR"], "offline fixture");
            assert_eq!(store.configuration().unwrap().locale, Locale::Simplified);
        });
    }
    #[test]
    fn original_native_labels_follow_all_verified_locale_dictionaries() {
        for (locale, source) in [
            (
                Locale::French,
                include_str!("../../../language-packs/fr-FR/r1/translations.json"),
            ),
            (
                Locale::Russian,
                include_str!("../../../language-packs/ru-RU/r1/translations.json"),
            ),
            (
                Locale::Japanese,
                include_str!("../../../language-packs/ja-JP/r1/translations.json"),
            ),
            (
                Locale::Spanish,
                include_str!("../../../language-packs/es-ES/r1/translations.json"),
            ),
            (
                Locale::Thai,
                include_str!("../../../language-packs/th-TH/r1/translations.json"),
            ),
        ] {
            let dictionary: serde_json::Value = serde_json::from_str(source).unwrap();
            let snapshot = LanguageSnapshot {
                locale,
                options: core::options(),
                web_messages: BTreeMap::new(),
                native_messages: dictionary["native"]
                    .as_object()
                    .unwrap()
                    .iter()
                    .map(|(key, text)| (stable_id(key), text.as_str().unwrap().into()))
                    .collect(),
            };
            for (cn, en) in [
                ("确认", "Confirm"),
                ("取消", "Cancel"),
                ("退出 IMDb Tech Manager", "Quit IMDb Tech Manager"),
            ] {
                assert_eq!(
                    native_message(&snapshot, cn, en).unwrap(),
                    dictionary["native"][en].as_str().unwrap()
                );
            }
        }
    }
}
