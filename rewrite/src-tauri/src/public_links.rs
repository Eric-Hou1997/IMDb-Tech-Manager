use product_core::{AppError, Locale, Result};

#[derive(Clone, Copy, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProductLink {
    Privacy,
    Terms,
    License,
    Repository,
}

fn url(kind: ProductLink, locale: &Locale) -> String {
    let base = "https://github.com/Eric-Hou1997/IMDb-Tech-Manager";
    let document = match kind {
        ProductLink::Privacy => "PRIVACY",
        ProductLink::Terms => "TERMS",
        ProductLink::License => return format!("{base}/blob/main/LICENSE"),
        ProductLink::Repository => return base.into(),
    };
    let suffix = match locale {
        Locale::Simplified => return format!("{base}/blob/main/{document}.md"),
        Locale::Traditional => "zh-Hant",
        Locale::English => "en",
        Locale::French => "fr",
        Locale::Russian => "ru",
        Locale::Japanese => "ja",
        Locale::Spanish => "es",
        Locale::Thai => "th",
    };
    format!("{base}/blob/main/docs/legal/{document}.{suffix}.md")
}

#[tauri::command]
pub async fn open_product_link(kind: ProductLink, locale: Locale) -> Result<()> {
    tauri::async_runtime::spawn_blocking(move || {
        let url = url(kind, &locale);
        #[cfg(target_os = "macos")]
        let status = std::process::Command::new("/usr/bin/open")
            .arg(url)
            .status();
        #[cfg(windows)]
        let status = std::process::Command::new("rundll32.exe")
            .arg("url.dll,FileProtocolHandler")
            .arg(url)
            .status();
        #[cfg(target_os = "linux")]
        let status = std::process::Command::new("xdg-open").arg(url).status();
        match status {
            Ok(status) if status.success() => Ok(()),
            Ok(status) => Err(AppError::new("browser-open", status)),
            Err(error) => Err(AppError::new("browser-open", error)),
        }
    })
    .await
    .map_err(|e| AppError::new("browser-worker", e))?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_product_links_keep_all_eight_legal_locales_and_reject_arbitrary_targets() {
        for (locale, suffix) in [
            (Locale::Simplified, ""),
            (Locale::Traditional, "zh-Hant"),
            (Locale::English, "en"),
            (Locale::French, "fr"),
            (Locale::Russian, "ru"),
            (Locale::Japanese, "ja"),
            (Locale::Spanish, "es"),
            (Locale::Thai, "th"),
        ] {
            for kind in [ProductLink::Privacy, ProductLink::Terms] {
                let address = url(kind, &locale);
                assert!(address
                    .starts_with("https://github.com/Eric-Hou1997/IMDb-Tech-Manager/blob/main/"));
                assert!(address.ends_with(
                    if suffix.is_empty() {
                        ".md".into()
                    } else {
                        format!(".{suffix}.md")
                    }
                    .as_str()
                ));
            }
        }
        assert!(serde_json::from_str::<ProductLink>("\"https://example.org\"").is_err());
        assert_eq!(
            url(ProductLink::Repository, &Locale::Thai),
            "https://github.com/Eric-Hou1997/IMDb-Tech-Manager"
        );
    }
}
