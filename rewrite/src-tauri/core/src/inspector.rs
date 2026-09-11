//! NFO-derived status and hash-bound presentation annotations.
//! An annotation never changes media bytes, ownership, or generation eligibility.
use crate::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use ts_rs::TS;
pub const OVERRIDES: &[&str] = &[
    "ai-complete",
    "local-complete",
    "spec-ready",
    "spec-missing",
    "spec-empty",
    "no-tags",
    "stale",
    "review",
    "tag-missing",
    "legacy",
    "manual-spec",
];
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(default)]
pub struct Inspection {
    pub xml_valid: bool,
    pub bom: bool,
    pub newline: String,
    pub tag_status: String,
    pub lifecycle: String,
    pub status_override: String,
    pub issues: Vec<String>,
    pub ignored_issues: Vec<String>,
    pub tag_engine: String,
    pub tag_model: String,
    pub tag_prompt_hash: String,
    pub fetched_at: String,
    pub source_fetched_at: String,
    pub source_specs: Specs,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Annotation {
    pub source_hash: String,
    #[serde(default)]
    pub issue_hash: String,
    #[serde(default)]
    pub status_hash: String,
    pub ignored_kinds: Vec<String>,
    pub status_override: String,
    pub updated_at: String,
}
impl Annotation {
    pub fn validate(&self) -> Result<()> {
        if [&self.issue_hash, &self.status_hash]
            .iter()
            .any(|h| !h.is_empty() && (h.len() != 64 || !h.bytes().all(|c| c.is_ascii_hexdigit())))
            || self.source_hash.len() != 64
            || !self.source_hash.bytes().all(|c| c.is_ascii_hexdigit())
            || (!self.status_override.is_empty()
                && !OVERRIDES.contains(&self.status_override.as_str()))
            || self.ignored_kinds.len() > 100
            || self.ignored_kinds.iter().any(|k| {
                protected(k)
                    || k.is_empty()
                    || k.len() > 100
                    || !k
                        .bytes()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
            })
        {
            return Err(AppError::new(
                "invalid-annotation",
                "Invalid hash-bound Inspector annotation",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum AnnotationAction {
    Ignore { issue: String },
    RestoreIssues,
    SetStatus { value: String },
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct AnnotationRequest {
    pub operation_id: String,
    pub item_id: String,
    pub expected_hash: String,
    pub action: AnnotationAction,
}
pub fn protected(kind: &str) -> bool {
    ["xml-error", "read-error", "library-type-mismatch"].contains(&kind)
}
pub fn type_matches(item: &MediaItem) -> bool {
    match item.space {
        Space::Movie => item.kind == "Movie",
        Space::Tv => ["Series", "Episode", "Season"].contains(&item.kind.as_str()),
    }
}
pub fn generation_allowed(item: &MediaItem) -> Result<()> {
    if !type_matches(item) {
        return Err(AppError::new(
            "library-type-mismatch",
            "Media type does not match its configured library space",
        )
        .at(&item.path));
    }
    if item.kind == "Season" {
        return Err(
            AppError::new("not-applicable", "Season NFO is not a generation target").at(&item.path),
        );
    }
    Ok(())
}
pub fn lifecycle(item: &MediaItem) -> String {
    let i = &item.inspection;
    if !i.xml_valid {
        return "xml-error".into();
    }
    if !["Movie", "Series", "Episode"].contains(&item.kind.as_str()) {
        return "not-applicable".into();
    }
    if !i.status_override.is_empty() {
        return i.status_override.clone();
    }
    match item.spec_status.as_str() {
        "missing" => "spec-missing".into(),
        "empty" => "spec-empty".into(),
        _ => match i.tag_status.as_str() {
            "ai-current" => "ai-complete",
            "local-current" => "local-complete",
            "current" => "legacy",
            "stale" => "stale",
            "review" => "review",
            "tag-missing" => "tag-missing",
            "legacy" => "legacy",
            _ => "no-tags",
        }
        .into(),
    }
}
fn text(n: roxmltree::Node<'_, '_>) -> String {
    n.descendants()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}
pub fn derive(
    item: &mut MediaItem,
    tech: Option<roxmltree::Node<'_, '_>>,
    raw: &[u8],
) -> Result<()> {
    let spec_hash = specs::fingerprint(&item.specs)?;
    let mut i = Inspection {
        xml_valid: true,
        bom: raw.starts_with(b"\xef\xbb\xbf"),
        newline: if raw.windows(2).any(|s| s == b"\r\n") {
            "CRLF"
        } else {
            "LF"
        }
        .into(),
        tag_status: "none".into(),
        ..Default::default()
    };
    if item.kind == "Season" {
        item.spec_status = "not-applicable".into();
        i.tag_status = "not-applicable".into();
    } else {
        if item.imdb.is_empty() {
            i.issues.push("missing-imdb".into());
        }
        if item.spec_status == "missing" {
            i.issues.push("spec-missing".into());
        } else if let Some(tech) = tech {
            i.fetched_at = tech.attribute("fetched").unwrap_or("").into();
            i.source_specs = item.specs.clone();
            if let Some(snapshot) = tech.children().find(|n| n.has_tag_name("sourcesnapshot")) {
                i.source_fetched_at = snapshot.attribute("fetched").unwrap_or("").into();
                let mut source = Specs::new();
                for section in snapshot.children().filter(|n| n.has_tag_name("section")) {
                    crate::collect_specs(
                        &mut source,
                        section.attribute("name").unwrap_or(""),
                        section
                            .children()
                            .filter(|n| n.has_tag_name("item"))
                            .map(text)
                            .collect(),
                    );
                }
                if !source.is_empty() {
                    i.source_specs = source;
                }
            }
            if let Some(manifest) = tech.children().rfind(|n| n.has_tag_name("generatedtags")) {
                i.tag_engine = manifest.attribute("engine").unwrap_or("").into();
                i.tag_model = manifest.attribute("model").unwrap_or("").into();
                i.tag_prompt_hash = manifest.attribute("promptHash").unwrap_or("").into();
                if !i.tag_engine.is_empty() {
                    let have: BTreeSet<_> = item
                        .tags
                        .iter()
                        .map(|t| library::canonical_tag(&t.value))
                        .collect();
                    i.tag_status = if manifest
                        .children()
                        .filter(|n| n.has_tag_name("tag"))
                        .any(|n| !have.contains(&library::canonical_tag(&text(n))))
                    {
                        "tag-missing"
                    } else if manifest
                        .attribute("specHash")
                        .is_some_and(|h| !h.is_empty() && h != spec_hash)
                    {
                        "stale"
                    } else if manifest.attribute("state") == Some("review") {
                        "review"
                    } else if i.tag_engine == "ai" {
                        "ai-current"
                    } else if i.tag_engine == "local-rules" {
                        "local-current"
                    } else {
                        "current"
                    }
                    .into();
                    if ["stale", "tag-missing", "review"].contains(&i.tag_status.as_str()) {
                        i.issues.push(i.tag_status.clone());
                    }
                }
            }
            if i.tag_engine.is_empty() {
                let have: BTreeSet<_> = item.tags.iter().map(|t| ownership_key(&t.value)).collect();
                if rules::entries(&item.specs)
                    .iter()
                    .any(|e| have.contains(&ownership_key(&e.value)))
                {
                    i.tag_status = "legacy".into();
                }
            }
        }
        let mut seen = BTreeSet::new();
        if item
            .tags
            .iter()
            .any(|t| !seen.insert(ownership_key(&t.value)))
        {
            i.issues.push("duplicate-tag".into());
        }
        if !type_matches(item) {
            i.issues.push("library-type-mismatch".into());
        }
    }
    *item.inspection = i;
    item.inspection.lifecycle = lifecycle(item);
    Ok(())
}
pub fn apply(item: &mut MediaItem, annotation: Option<&Annotation>) {
    if item.parser_revision < library::PARSER_REVISION {
        item.inspection.lifecycle = "index-refresh-required".into();
        item.inspection.issues = vec!["index-refresh-required".into()];
        return;
    }
    // Start with the cached static layer on every read; never store this overlay in the index.
    if let Some(value) = annotation.filter(|a| a.validate().is_ok()) {
        let status_hash = if value.status_hash.is_empty() {
            &value.source_hash
        } else {
            &value.status_hash
        };
        let issue_hash = if value.issue_hash.is_empty() {
            &value.source_hash
        } else {
            &value.issue_hash
        };
        if status_hash == &item.source_hash {
            item.inspection.status_override = value.status_override.clone();
        }
        let mut active = vec![];
        let mut ignored = vec![];
        for issue in item.inspection.issues.drain(..) {
            if issue_hash == &item.source_hash
                && value.ignored_kinds.contains(&issue)
                && !protected(&issue)
            {
                ignored.push(issue)
            } else {
                active.push(issue)
            }
        }
        item.inspection.issues = active;
        item.inspection.ignored_issues = ignored;
    }
    if let Some(error) = &item.error {
        item.inspection.xml_valid = false;
        let issue = if ["invalid-xml", "invalid-encoding"].contains(&error.code.as_str()) {
            "xml-error"
        } else {
            "read-error"
        };
        if !item.inspection.issues.iter().any(|i| i == issue) {
            item.inspection.issues.push(issue.into());
        }
    }
    item.inspection.lifecycle = lifecycle(item);
}
