use crate::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(rename = "AutomaticSettings")]
pub struct Settings {
    pub interval_seconds: u32,
    pub on_app_start: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            interval_seconds: 60,
            on_app_start: false,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<()> {
        if !(30..=86400).contains(&self.interval_seconds) {
            return Err(AppError::new(
                "invalid-interval",
                "Automatic interval must be between 30 and 86400 seconds",
            ));
        }
        Ok(())
    }
}
pub fn legacy_settings(value: &serde_json::Value) -> Result<Option<Settings>> {
    if !["interval_seconds", "auto_mode_on_app_start", "auto_start"]
        .iter()
        .any(|k| value.get(k).is_some())
    {
        return Ok(None);
    }
    let flag = |name: &str| -> Result<bool> {
        value
            .get(name)
            .map(|v| {
                v.as_bool().ok_or_else(|| {
                    AppError::new(
                        "legacy-automatic-settings",
                        format!("Expected boolean {name}"),
                    )
                })
            })
            .unwrap_or(Ok(false))
    };
    let interval = value
        .get("interval_seconds")
        .map(|v| {
            v.as_i64().ok_or_else(|| {
                AppError::new("legacy-automatic-settings", "Expected integer interval")
            })
        })
        .transpose()?
        .unwrap_or(60);
    let settings = Settings {
        interval_seconds: if interval < 30 {
            60
        } else {
            interval
                .try_into()
                .map_err(|_| AppError::new("legacy-automatic-settings", "Invalid interval"))?
        },
        on_app_start: if flag("auto_mode_on_app_start_configured")? {
            flag("auto_mode_on_app_start")?
        } else if flag("auto_start_configured")? {
            flag("auto_start")?
        } else {
            false
        },
    };
    settings.validate()?;
    Ok(Some(settings))
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[ts(rename = "AutomaticStatus")]
pub struct Status {
    pub settings: Settings,
    pub enabled: bool,
    #[ts(type = "number")]
    pub cycle: u64,
    #[ts(type = "number")]
    pub next_due: i64,
    pub task_ids: Vec<String>,
}
/// The old resident engine prioritizes stable recent files, then attempts one
/// missing older item. It never discovers roots or walks directories per cycle.
pub fn candidates(
    items: &[MediaItem],
    now: i64,
    cooling: impl Fn(&MediaItem) -> bool,
) -> Vec<String> {
    let eligible = |i: &&MediaItem| {
        i.error.is_none()
            && crate::inspector::type_matches(i)
            && ["Movie", "Series", "Episode"].contains(&i.kind.as_str())
            && !i.imdb.is_empty()
            && i.spec_status == "missing"
    };
    let mut result: Vec<String> = items
        .iter()
        .filter(eligible)
        .filter(|i| (90..=900).contains(&now.saturating_sub(i.modified_at)))
        .map(|i| i.id.clone())
        .collect();
    if let Some(item) = items
        .iter()
        .filter(eligible)
        .find(|i| !result.contains(&i.id) && !cooling(i))
    {
        result.push(item.id.clone());
    }
    result
}
