use crate::{AppError, MediaItem, Result, Space};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, Default)]
#[serde(rename_all = "kebab-case")]
pub enum Sort {
    #[default]
    Title,
    Year,
    Path,
    Status,
    AddedDate,
    SpecsStatus,
    TagsStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum CatalogFilter {
    All,
    NonOptimal,
    Ai,
    Local,
    Ready,
    Nospec,
    Error,
    MissingImdb,
    XmlError,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "lowercase")]
pub enum MediaLevel {
    All,
    Tvshow,
    Episode,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, Default)]
pub struct LibraryView {
    #[serde(default)]
    pub lifecycle: String,
    #[serde(default)]
    pub issues: bool,
    pub search: String,
    pub errors: bool,
    pub roots: Vec<String>,
    pub selected: Vec<String>,
    pub expanded: Vec<String>,
    pub offset: u32,
    pub sort: Sort,
    pub descending: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalog_filter: Option<CatalogFilter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub media_level: Option<MediaLevel>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord, TS)]
#[serde(rename_all = "snake_case")]
pub enum CatalogColumn {
    Title,
    Year,
    AddedDate,
    SpecStatus,
    TagStatus,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct CatalogColumns {
    pub order: Vec<CatalogColumn>,
    pub visible: Vec<CatalogColumn>,
    pub widths: std::collections::BTreeMap<CatalogColumn, u16>,
    pub compact: bool,
}
impl Default for CatalogColumns {
    fn default() -> Self {
        let columns = vec![
            CatalogColumn::Title,
            CatalogColumn::Year,
            CatalogColumn::AddedDate,
            CatalogColumn::SpecStatus,
            CatalogColumn::TagStatus,
        ];
        Self {
            order: columns.clone(),
            visible: columns,
            widths: Default::default(),
            compact: false,
        }
    }
}
impl CatalogColumns {
    fn validate(&self) -> Result<()> {
        let order: std::collections::BTreeSet<_> = self.order.iter().collect();
        let visible: std::collections::BTreeSet<_> = self.visible.iter().collect();
        if self.order.len() != 5
            || order.len() != 5
            || self.order.first() != Some(&CatalogColumn::Title)
            || !visible.contains(&CatalogColumn::Title)
            || visible.len() != self.visible.len()
            || self
                .widths
                .values()
                .any(|width| !(1..=4000).contains(width))
        {
            return Err(AppError::new(
                "view-column-layout",
                "Invalid catalog column layout",
            ));
        }
        Ok(())
    }
}
/// Product-window preferences. An absent value preserves the serialized request
/// identity of the earlier rewrite UI and does not invalidate operation replays.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct PresentationState {
    #[serde(with = "layout_number")]
    #[ts(type = "number")]
    pub split_basis_points: f64,
    #[serde(with = "layout_number")]
    #[ts(type = "number")]
    pub task_height: f64,
    pub task_open: bool,
    pub task_history: bool,
    pub inspector_tab: InspectorTab,
    pub current_movie: Option<String>,
    pub current_tv: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub movie_columns: Option<CatalogColumns>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tv_columns: Option<CatalogColumns>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS, Default)]
#[serde(rename_all = "kebab-case")]
pub enum InspectorTab {
    #[default]
    Overview,
    Specs,
    Tags,
}
impl Default for PresentationState {
    fn default() -> Self {
        Self {
            split_basis_points: 4200.0,
            task_height: 240.0,
            task_open: false,
            task_history: false,
            inspector_tab: InspectorTab::Overview,
            current_movie: None,
            current_tv: None,
            movie_columns: None,
            tv_columns: None,
        }
    }
}
// Integer layouts from earlier rewrite operations retain their exact JSON
// request fingerprint. Fractional original preferences keep their subpixels.
mod layout_number {
    pub fn deserialize<'de, D: serde::Deserializer<'de>>(
        deserializer: D,
    ) -> std::result::Result<f64, D::Error> {
        serde::Deserialize::deserialize(deserializer)
    }
    pub fn serialize<S: serde::Serializer>(
        value: &f64,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        if value.is_finite() && value.fract() == 0.0 && *value >= 0.0 && *value <= u32::MAX as f64 {
            serializer.serialize_u32(*value as u32)
        } else {
            serializer.serialize_f64(*value)
        }
    }
}
impl PresentationState {
    fn validate(&self) -> Result<()> {
        if !self.split_basis_points.is_finite()
            || !(100.0..=9900.0).contains(&self.split_basis_points)
            || !self.task_height.is_finite()
            || !(190.0..=4000.0).contains(&self.task_height)
        {
            return Err(AppError::new(
                "view-layout-bounds",
                "Invalid window layout preference",
            ));
        }
        for columns in [&self.movie_columns, &self.tv_columns]
            .into_iter()
            .flatten()
        {
            columns.validate()?;
        }
        for id in [&self.current_movie, &self.current_tv]
            .into_iter()
            .flatten()
        {
            if id.is_empty() || id.len() > 256 || id.chars().any(char::is_control) {
                return Err(AppError::new(
                    "view-state-id",
                    "Invalid current item identifier",
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, TS)]
pub struct UiState {
    pub revision: u32,
    pub active_space: Space,
    pub movie: LibraryView,
    pub tv: LibraryView,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub presentation: Option<PresentationState>,
}
impl Default for UiState {
    fn default() -> Self {
        Self {
            revision: 0,
            active_space: Space::Movie,
            movie: LibraryView::default(),
            tv: LibraryView::default(),
            presentation: None,
        }
    }
}
impl UiState {
    pub fn validate(&self) -> Result<()> {
        if let Some(presentation) = &self.presentation {
            presentation.validate()?;
        }
        for view in [&self.movie, &self.tv] {
            if view.lifecycle.len() > 64
                || view.search.len() > 4096
                || view.roots.len() > 1000
                || view.selected.len() > 100_000
                || view.expanded.len() > 100_000
                || view.offset > 10_000_000
            {
                return Err(AppError::new(
                    "view-state-limit",
                    "View state exceeds supported size",
                ));
            }
            for values in [&view.roots, &view.selected, &view.expanded] {
                let mut ids = std::collections::HashSet::new();
                for id in values {
                    if id.is_empty() || id.len() > 256 || !ids.insert(id) {
                        return Err(AppError::new(
                            "view-state-id",
                            "Invalid or repeated view identifier",
                        ));
                    }
                }
            }
        }
        Ok(())
    }
}
fn catalog_bucket(item: &MediaItem) -> CatalogFilter {
    match item.inspection.lifecycle.as_str() {
        "xml-error" | "index-refresh-required" => CatalogFilter::Error,
        "not-applicable" => CatalogFilter::All,
        "spec-missing" | "spec-empty" => CatalogFilter::Nospec,
        "ai-complete" => CatalogFilter::Ai,
        "local-complete" => CatalogFilter::Local,
        _ if item.error.is_some()
            || item.inspection.issues.iter().any(|issue| {
                [
                    "xml-error",
                    "read-error",
                    "ownership-mismatch",
                    "library-type-mismatch",
                ]
                .contains(&issue.as_str())
            }) =>
        {
            CatalogFilter::Error
        }
        _ => CatalogFilter::Ready,
    }
}
fn catalog_matches(item: &MediaItem, view: &LibraryView) -> bool {
    let status = match &view.catalog_filter {
        None | Some(CatalogFilter::All) => true,
        Some(CatalogFilter::NonOptimal) => catalog_bucket(item) != CatalogFilter::Ai,
        Some(CatalogFilter::MissingImdb) => item.imdb.is_empty(),
        Some(CatalogFilter::XmlError) => {
            catalog_bucket(item) == CatalogFilter::Error && !item.inspection.xml_valid
        }
        Some(filter) => *filter == catalog_bucket(item),
    };
    status
        && (item.space != Space::Tv
            || match view.media_level {
                None | Some(MediaLevel::All) => true,
                Some(MediaLevel::Tvshow) => item.kind == "Series",
                Some(MediaLevel::Episode) => item.kind == "Episode",
            })
}
pub fn matches(item: &MediaItem, space: &Space, view: &LibraryView) -> bool {
    item.space == *space
        && catalog_matches(item, view)
        && (view.roots.is_empty() || view.roots.contains(&item.root_id))
        && (view.lifecycle.is_empty() || item.inspection.lifecycle == view.lifecycle)
        && (!view.issues || !item.inspection.issues.is_empty() || item.error.is_some())
        && (!view.errors || item.error.is_some())
        && (view.search.trim().is_empty()
            || format!(
                "{} {} {} {} {}",
                item.title, item.year, item.imdb, item.path, item.series_key
            )
            .to_lowercase()
            .contains(&view.search.trim().to_lowercase()))
}
pub(crate) fn spec_rank(value: &str) -> u8 {
    match value {
        "missing" => 0,
        "empty" => 1,
        "manual" => 2,
        "ready" => 3,
        "not-applicable" => 4,
        _ => 0,
    }
}
pub(crate) fn tag_rank(value: &str) -> u8 {
    match value {
        "none" => 0,
        "tag-missing" => 1,
        "stale" => 2,
        "review" => 3,
        "legacy" => 4,
        "local-current" => 5,
        "ai-current" => 6,
        "not-applicable" => 7,
        _ => 0,
    }
}
pub fn sort(items: &mut [MediaItem], view: &LibraryView) {
    items.sort_by(|a, b| {
        let order = match view.sort {
            Sort::Title => a.title.to_lowercase().cmp(&b.title.to_lowercase()),
            Sort::Year => a.year.cmp(&b.year),
            Sort::Path => a.path.cmp(&b.path),
            Sort::Status => a.inspection.lifecycle.cmp(&b.inspection.lifecycle),
            Sort::AddedDate => a.added_date.cmp(&b.added_date),
            Sort::SpecsStatus => spec_rank(&a.spec_status).cmp(&spec_rank(&b.spec_status)),
            Sort::TagsStatus => {
                tag_rank(&a.inspection.tag_status).cmp(&tag_rank(&b.inspection.tag_status))
            }
        };
        let order = order
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.id.cmp(&b.id));
        if view.descending {
            order.reverse()
        } else {
            order
        }
    });
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct UiReceipt {
    pub revision: u32,
}
