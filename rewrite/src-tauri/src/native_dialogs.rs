//! Original input/confirmation dialogs. Wry's macOS delegate does not supply
//! the JavaScript dialog callbacks that the v4 launcher implemented.
use product_core::{AppError, Result};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc,
};
use tauri::Manager;

static OPEN: AtomicBool = AtomicBool::new(false);
static CANCEL_EPOCH: AtomicU64 = AtomicU64::new(0);
static ACTIVE_DIALOG: AtomicU64 = AtomicU64::new(0);
struct DialogOwner;
impl Drop for DialogOwner {
    fn drop(&mut self) {
        OPEN.store(false, Ordering::SeqCst);
    }
}

#[tauri::command]
pub async fn native_dialog(
    message: String,
    default_value: Option<String>,
    confirm: String,
    cancel: String,
    app: tauri::AppHandle,
) -> Result<Option<String>> {
    if message.len() > 16384
        || default_value
            .as_ref()
            .is_some_and(|v| v.len() > 1024 * 1024)
        || confirm.is_empty()
        || cancel.is_empty()
        || confirm.len() > 128
        || cancel.len() > 128
    {
        return Err(AppError::new(
            "invalid-dialog",
            "Invalid native dialog text",
        ));
    }
    let epoch = CANCEL_EPOCH.load(Ordering::SeqCst);
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err(AppError::new(
            "dialog-busy",
            "A native dialog is already open",
        ));
    }
    let _owner = DialogOwner;
    let dialog = ACTIVE_DIALOG.fetch_add(1, Ordering::SeqCst) + 1;
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::new("dialog-window", "Main window is unavailable"))?;
    let (sender, receiver) = mpsc::channel();
    #[cfg(target_os = "macos")]
    app.run_on_main_thread(move || {
        if CANCEL_EPOCH.load(Ordering::SeqCst) != epoch {
            let _ = sender.send(None);
            return;
        }
        use block2::RcBlock;
        use objc2_app_kit::{NSAlert, NSAlertFirstButtonReturn, NSTextField, NSWindow};
        use objc2_foundation::{MainThreadMarker, NSPoint, NSRect, NSSize, NSString};
        use std::cell::RefCell;
        if ACTIVE_DIALOG.load(Ordering::SeqCst) != dialog {
            let _ = sender.send(None);
            return;
        }
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        let Ok(parent) = window.ns_window() else {
            return;
        };
        // Tauri owns this NSWindow; the pointer is obtained and used only on
        // its UI thread. The sheet and accessory have the parent window's life.
        let parent = unsafe { &*parent.cast::<NSWindow>() };
        let alert = NSAlert::new(mtm);
        alert.setMessageText(&NSString::from_str(&message));
        let field = default_value.map(|value| {
            let field = NSTextField::textFieldWithString(&NSString::from_str(&value), mtm);
            field.setFrame(NSRect::new(NSPoint::new(0., 0.), NSSize::new(360., 24.)));
            alert.setAccessoryView(Some(&field));
            field
        });
        alert.addButtonWithTitle(&NSString::from_str(&confirm));
        alert.addButtonWithTitle(&NSString::from_str(&cancel));
        let sender = RefCell::new(Some(sender));
        let handler = RcBlock::new(move |response| {
            let value = if response == NSAlertFirstButtonReturn {
                Some(
                    field
                        .as_ref()
                        .map(|f| f.stringValue().to_string())
                        .unwrap_or_default(),
                )
            } else {
                None
            };
            if let Some(sender) = sender.borrow_mut().take() {
                let _ = sender.send(value);
            }
        });
        alert.beginSheetModalForWindow_completionHandler(parent, Some(&handler));
    })
    .map_err(|e| AppError::new("dialog-window", e))?;
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (dialog, epoch);
        use tauri_plugin_dialog::{DialogExt, MessageDialogButtons};
        if default_value.is_some() {
            return Err(AppError::new(
                "dialog-input-unavailable",
                "This platform uses its WebView input dialog",
            ));
        }
        app.dialog()
            .message(message)
            .title("")
            .parent(&window)
            .buttons(MessageDialogButtons::OkCancelCustom(confirm, cancel))
            .show(move |accepted| {
                let _ = sender.send(accepted.then(String::new));
            });
    }
    tauri::async_runtime::spawn_blocking(move || receiver.recv())
        .await
        .map_err(|e| AppError::new("dialog-worker", e))?
        .map_err(|e| AppError::new("dialog-closed", e))
}

/// End an owned sheet before its parent is hidden or the app starts shutdown.
pub fn cancel(app: &tauri::AppHandle) {
    CANCEL_EPOCH.fetch_add(1, Ordering::SeqCst);
    let dialog = ACTIVE_DIALOG.load(Ordering::SeqCst);
    #[cfg(target_os = "macos")]
    if OPEN.load(Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = app.run_on_main_thread(move || {
                if ACTIVE_DIALOG.load(Ordering::SeqCst) != dialog || !OPEN.load(Ordering::SeqCst) {
                    return;
                }
                use objc2_app_kit::{NSAlertSecondButtonReturn, NSWindow};
                if let Ok(parent) = window.ns_window() {
                    let parent = unsafe { &*parent.cast::<NSWindow>() };
                    if let Some(sheet) = parent.attachedSheet() {
                        parent.endSheet_returnCode(&sheet, NSAlertSecondButtonReturn);
                    }
                }
            });
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, dialog);
}
