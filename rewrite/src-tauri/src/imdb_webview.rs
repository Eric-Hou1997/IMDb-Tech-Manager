//! An isolated, temporary system WebView. It has no capability grant and cannot
//! invoke manager commands. Rust reads the page through the native eval callback.
use crate::desktop::Desktop;
use product_core::{specs::SourceSpecs, AppError, Result};
use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};
struct PageOwner {
    window: WebviewWindow,
    destroyed: Arc<AtomicBool>,
}
impl PageOwner {
    fn close(&self) -> Result<()> {
        if !self.destroyed.load(Ordering::SeqCst) {
            self.window
                .destroy()
                .map_err(|e| AppError::new("imdb-window-cleanup", e))?;
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while !self.destroyed.load(Ordering::SeqCst) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        if self.destroyed.load(Ordering::SeqCst) {
            Ok(())
        } else {
            Err(AppError::new(
                "imdb-window-cleanup",
                "IMDb window destruction was not confirmed",
            ))
        }
    }
}
impl Drop for PageOwner {
    fn drop(&mut self) {
        if !self.destroyed.load(Ordering::SeqCst) {
            if let Err(error) = self.window.destroy() {
                eprintln!("imdb-window-cleanup: {error}");
            }
        }
    }
}
pub fn fetch(app: &tauri::AppHandle, id: &str, imdb: &str) -> Result<SourceSpecs> {
    let url = product_core::specs::imdb_url(imdb)?;
    let title_path = format!("/title/{imdb}/");
    let label = format!("imdb-{}", product_core::hash(id.as_bytes()));
    let window = WebviewWindowBuilder::new(
        app,
        &label,
        WebviewUrl::External(
            url.parse()
                .map_err(|e| AppError::new("invalid-imdb-url", e))?,
        ),
    )
    .title(format!("IMDb · {imdb}"))
    .inner_size(900.0, 700.0)
    .incognito(true)
    .on_navigation(move |url| {
        url.scheme() == "https"
            && matches!(url.host_str(), Some("www.imdb.com" | "m.imdb.com"))
            && url.path().starts_with(&title_path)
    })
    .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
    .on_download(|_, _| false)
    .build()
    .map_err(|e| AppError::new("imdb-webview-unavailable", e))?;
    let destroyed = Arc::new(AtomicBool::new(false));
    let observer = destroyed.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            observer.store(true, Ordering::SeqCst);
        }
    });
    let owner = PageOwner { window, destroyed };
    let result = (|| {
        let desktop = app.state::<Desktop>();
        let deadline = Instant::now() + Duration::from_secs(90);
        let mut last_error = AppError::new(
            "imdb-webview-timeout",
            "IMDb page did not provide technical data within 90 seconds",
        );
        while Instant::now() < deadline {
            if desktop.stopping()
                || desktop.store.fetch_record(id)?.phase != "requested"
                || owner.destroyed.load(Ordering::SeqCst)
            {
                return Err(AppError::new(
                    "cancelled",
                    "IMDb page was closed or the request was cancelled",
                ));
            }
            let (tx, rx) = mpsc::sync_channel(1);
            owner.window.eval_with_callback("(() => { const s=document.querySelector('script#__NEXT_DATA__'); return s && s.textContent.length <= 8388608 ? '<script id=\"__NEXT_DATA__\">'+s.textContent+'</script>' : ''; })()",move |value|{let _=tx.try_send(value);}).map_err(|e|AppError::new("imdb-page-read",e))?;
            let poll_deadline = Instant::now() + Duration::from_millis(700);
            while Instant::now() < poll_deadline {
                if desktop.stopping()
                    || desktop.store.fetch_record(id)?.phase != "requested"
                    || owner.destroyed.load(Ordering::SeqCst)
                {
                    return Err(AppError::new("cancelled", "IMDb request cancelled"));
                }
                match rx.recv_timeout(Duration::from_millis(50)) {
                    Ok(value) => {
                        let page: String = serde_json::from_str(&value)
                            .map_err(|e| AppError::new("imdb-page-read", e))?;
                        if !page.is_empty() {
                            match product_core::specs::parse_page(imdb, &page) {
                                Ok(source) => return Ok(source),
                                Err(error)
                                    if error.code == "imdb-no-tech"
                                        || error.code == "imdb-title-mismatch" =>
                                {
                                    return Err(error)
                                }
                                Err(error) => last_error = error,
                            }
                        }
                        break;
                    }
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    Err(mpsc::RecvTimeoutError::Disconnected) => break,
                }
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(last_error)
    })();
    owner.close()?;
    result
}
