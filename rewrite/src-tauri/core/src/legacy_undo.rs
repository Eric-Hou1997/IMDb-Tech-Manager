use crate::*;
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
pub(crate) const PATH_EXPR:&str="json_extract(CASE WHEN json_valid(CAST(body AS TEXT)) THEN CAST(body AS TEXT) ELSE '{}' END,'$.path')";
pub(crate) const HASH_EXPR:&str="json_extract(CASE WHEN json_valid(CAST(body AS TEXT)) THEN CAST(body AS TEXT) ELSE '{}' END,'$.after_hash')";
#[derive(Debug, Clone, Deserialize)]
pub struct Journal {
    pub schema: u32,
    pub path: String,
    pub operation: String,
    pub created_at: String,
    pub expires_at: String,
    pub before: String,
    pub before_hash: String,
    pub after_hash: String,
}
impl Journal {
    pub fn validate(&self, source: &str) -> Result<Vec<u8>> {
        if self.schema != 1
            || source.rsplit('/').next()
                != Some(format!("{}.json", hash(self.path.as_bytes())).as_str())
        {
            return Err(AppError::new(
                "legacy-undo-identity",
                "Historical undo schema or filename does not match its source path",
            ));
        }
        if self.before.len() > (library::MAX_NFO_BYTES.div_ceil(3) * 4) as usize {
            return Err(AppError::new(
                "legacy-undo-size",
                "Historical backup exceeds the NFO size limit",
            ));
        }
        let raw = STANDARD
            .decode(&self.before)
            .map_err(|e| AppError::new("legacy-undo-invalid", e))?;
        if raw.len() as u64 > library::MAX_NFO_BYTES
            || hash(&raw) != self.before_hash
            || self.after_hash.len() != 64
            || !self.after_hash.bytes().all(|b| b.is_ascii_hexdigit())
        {
            return Err(AppError::new(
                "legacy-undo-hash",
                "Historical backup does not match its recorded checksum",
            ));
        }
        let text =
            std::str::from_utf8(&raw).map_err(|e| AppError::new("legacy-undo-encoding", e))?;
        let document = roxmltree::Document::parse(text.trim_start_matches('\u{feff}'))
            .map_err(|e| AppError::new("legacy-undo-xml", e))?;
        if !["movie", "tvshow", "episodedetails"]
            .contains(&document.root_element().tag_name().name())
        {
            return Err(AppError::new(
                "legacy-undo-media",
                "Historical backup is not a supported editable NFO",
            ));
        }
        let created = chrono::DateTime::parse_from_rfc3339(&self.created_at)
            .map_err(|e| AppError::new("legacy-undo-time", e))?;
        let expires = chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|e| AppError::new("legacy-undo-time", e))?;
        if expires < created || expires - created > chrono::Duration::seconds(1801) {
            return Err(AppError::new(
                "legacy-undo-time",
                "Historical undo lifetime exceeds the original 30-minute contract",
            ));
        }
        Ok(raw)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct LegacyUndoProof {
    pub import_id: String,
    pub source: String,
    pub archive_hash: String,
    pub before_hash: String,
    pub after_hash: String,
    pub expires_at: String,
    pub destination: String,
}
impl LegacyUndoProof {
    pub fn validate_candidate(
        &self,
        path: &std::path::Path,
        expected: &str,
        candidate: &[u8],
    ) -> Result<()> {
        if path.to_str() != Some(self.destination.as_str())
            || expected != self.after_hash
            || hash(candidate) != self.before_hash
        {
            return Err(AppError::new(
                "unsafe-legacy-undo",
                "Restore must match both the reviewed historical backup and current file",
            ));
        }
        if chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .map_err(|e| AppError::new("legacy-undo-time", e))?
            < chrono::Utc::now()
        {
            return Err(AppError::new(
                "legacy-undo-expired",
                "Historical undo expired; import does not extend its lifetime",
            ));
        }
        let text =
            std::str::from_utf8(candidate).map_err(|e| AppError::new("legacy-undo-encoding", e))?;
        roxmltree::Document::parse(text.trim_start_matches('\u{feff}'))
            .map_err(|e| AppError::new("legacy-undo-xml", e))?;
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LegacyUndoEntry {
    pub import_id: String,
    pub source: String,
    pub old_path: String,
    pub operation: String,
    pub expires_at: String,
    pub state: String,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LegacyUndoPage {
    pub total: u32,
    pub entries: Vec<LegacyUndoEntry>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LegacyUndoRequest {
    pub operation_id: String,
    pub item_id: String,
    pub import_id: String,
    pub source: String,
    pub expected_hash: String,
    pub confirm_path_mapping: bool,
}
