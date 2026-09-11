use crate::*;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum BatchEngine {
    Rules,
    Ai,
    Specs,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum BatchMode {
    Preview,
    Generate,
    Rebuild,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BatchRequest {
    pub operation_id: String,
    pub space: Space,
    pub item_ids: Vec<String>,
    #[serde(default)]
    pub root_ids: Vec<String>,
    #[serde(default)]
    pub retry_failed: bool,
    pub engine: BatchEngine,
    pub mode: BatchMode,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct BatchItem {
    pub item: MediaItem,
    pub request_id: String,
    pub write_id: String,
    pub phase: String,
    pub error: Option<AppError>,
    pub candidate_hash: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Batch {
    pub engine: BatchEngine,
    pub mode: BatchMode,
    pub plan_hash: String,
    pub approved: bool,
    pub retry_failed: bool,
    pub pause_requested: bool,
    pub cancel_requested: bool,
    pub settings: serde_json::Value,
    #[serde(default)]
    pub total: u32,
    #[serde(default)]
    pub items: Vec<BatchItem>,
}
#[derive(Debug, Clone)]
pub struct Outcome {
    pub phase: String,
    pub error: Option<AppError>,
    pub candidate_hash: Option<String>,
}
impl Outcome {
    pub fn failed(error: AppError) -> Self {
        Self {
            phase: "failed".into(),
            error: Some(error),
            candidate_hash: None,
        }
    }
}
pub fn child_id(task: &str, index: usize, kind: &str) -> String {
    format!(
        "batch-{}",
        crate::hash(format!("{task}:{index}:{kind}").as_bytes())
    )
}

pub fn skip_reason(task: &Task, row: &BatchItem) -> Result<Option<String>> {
    let batch = task
        .batch
        .as_ref()
        .ok_or_else(|| AppError::new("invalid-task", "Not a batch"))?;
    let root = task
        .roots
        .iter()
        .find(|r| r.id == row.item.root_id)
        .ok_or_else(|| AppError::new("invalid-root", "Missing planned root"))?;
    let (_, raw) = library::read_bytes(root, std::path::Path::new(&row.item.path))?;
    if hash(&raw) != row.item.source_hash {
        return Err(AppError::new(
            "source-conflict",
            "NFO changed after the batch scope was reviewed",
        ));
    }
    let text = std::str::from_utf8(&raw)
        .map_err(|e| AppError::new("invalid-encoding", e))?
        .trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(text).map_err(|e| AppError::new("invalid-xml", e))?;
    let tech = doc
        .root_element()
        .children()
        .find(|n| n.has_tag_name("technicalspecs"));
    if batch.engine != BatchEngine::Specs {
        if !row
            .item
            .specs
            .values()
            .flatten()
            .any(|v| !v.trim().is_empty())
        {
            return Ok(Some("skipped-empty-specs".into()));
        }
        if batch.mode == BatchMode::Generate {
            if let Some(generated) =
                tech.and_then(|n| n.children().find(|c| c.has_tag_name("generatedtags")))
            {
                let current = generated.attribute("owner") == Some("IMDb Tech Manager")
                    && generated.attribute("state") == Some("current")
                    && generated.attribute("specHash")
                        == Some(specs::fingerprint(&row.item.specs)?.as_str());
                let engine = generated.attribute("engine").unwrap_or("");
                if current
                    && (engine == "ai"
                        || batch.engine == BatchEngine::Rules && engine == "local-rules")
                {
                    return Ok(Some("skipped-current".into()));
                }
            }
        }
    } else if batch.mode == BatchMode::Generate
        && tech.is_some_and(|n| {
            n.attribute("source") == Some("IMDb")
                && n.attribute("imdbid") == Some(row.item.imdb.as_str())
                && [Some("ok"), Some("empty")].contains(&n.attribute("status"))
        })
    {
        return Ok(Some("skipped-spec-ready".into()));
    }
    Ok(None)
}
