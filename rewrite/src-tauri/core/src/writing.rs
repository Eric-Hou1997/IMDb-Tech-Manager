use crate::Specs;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct SpecsEdit {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub specs: Specs,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct WritePreview {
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
}
