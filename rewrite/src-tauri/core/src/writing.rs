use crate::Specs;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum WriteIntent {
    #[default]
    Specs,
    Tags {
        plan: crate::tags::Plan,
    },
    Undo {
        original_id: String,
    },
    LegacyUndo {
        proof: Box<crate::legacy_undo::LegacyUndoProof>,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct TagEdit {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub action: crate::tags::Action,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SpecsEdit {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub specs: Specs,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WritePreview {
    #[serde(default)]
    pub intent: WriteIntent,
    pub operation_id: String,
    pub item_id: String,
    pub path: String,
    pub title: String,
    pub year: String,
    pub imdb: String,
    pub media_kind: String,
    pub before_hash: String,
    pub after_hash: String,
    pub before_specs: Specs,
    pub after_specs: Specs,
    pub phase: String,
    pub undo_of: Option<String>,
    pub error: Option<crate::AppError>,
    #[serde(default)]
    pub before_xml: String,
    #[serde(default)]
    pub after_xml: String,
    #[serde(default)]
    pub before_tags: Vec<crate::Tag>,
    #[serde(default)]
    pub after_tags: Vec<crate::Tag>,
}
