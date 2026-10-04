use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use ts_rs::TS;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Space {
    Movie,
    Tv,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub enum Locale {
    #[serde(rename = "zh-CN")]
    Simplified,
    #[serde(rename = "zh-Hant")]
    Traditional,
    #[serde(rename = "en-US")]
    English,
    #[serde(rename = "fr-FR")]
    French,
    #[serde(rename = "ru-RU")]
    Russian,
    #[serde(rename = "ja-JP")]
    Japanese,
    #[serde(rename = "es-ES")]
    Spanish,
    #[serde(rename = "th-TH")]
    Thai,
}
impl Locale {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Simplified => "zh-CN",
            Self::Traditional => "zh-Hant",
            Self::English => "en-US",
            Self::French => "fr-FR",
            Self::Russian => "ru-RU",
            Self::Japanese => "ja-JP",
            Self::Spanish => "es-ES",
            Self::Thai => "th-TH",
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct LibraryRoot {
    pub id: String,
    pub space: Space,
    pub path: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct LibraryRootAccess {
    pub path: String,
    pub online: bool,
    pub state: String,
    pub access_error: Option<String>,
    pub entry_count: Option<u32>,
    pub checked_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Configuration {
    pub revision: u32,
    pub locale: Locale,
    pub roots: Vec<LibraryRoot>,
}
impl Default for Configuration {
    fn default() -> Self {
        Self {
            revision: 0,
            locale: Locale::Simplified,
            roots: vec![],
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum TaskState {
    Requested,
    Running,
    Paused,
    Cancelled,
    Interrupted,
    Failed,
    Completed,
}
impl TaskState {
    pub fn terminal(&self) -> bool {
        matches!(self, Self::Cancelled | Self::Failed | Self::Completed)
    }
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct AppError {
    pub code: String,
    pub message: String,
    pub path: Option<String>,
    pub operation_id: Option<String>,
    pub retryable: bool,
}
impl AppError {
    pub fn new(code: &str, message: impl ToString) -> Self {
        Self {
            code: code.into(),
            message: message.to_string(),
            path: None,
            operation_id: None,
            retryable: false,
        }
    }
    pub fn at(mut self, path: impl ToString) -> Self {
        self.path = Some(path.to_string());
        self
    }
}
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.code, self.message)
    }
}
impl std::error::Error for AppError {}
impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::new("database", e)
    }
}
impl From<serde_json::Error> for AppError {
    fn from(e: serde_json::Error) -> Self {
        Self::new("invalid-data", e)
    }
}
pub type Result<T> = std::result::Result<T, AppError>;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct ScanRequest {
    pub operation_id: String,
    pub space: Space,
    pub root_ids: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct TaskControl {
    pub operation_id: String,
    pub task_id: String,
    pub state: TaskState,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Task {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub journal: Option<crate::task_log::TaskJournal>,
    #[serde(default)]
    pub automatic: bool,
    #[serde(default)]
    pub batch: Option<crate::batch::Batch>,
    pub id: String,
    pub state: TaskState,
    pub locale: Locale,
    pub space: Space,
    pub roots: Vec<LibraryRoot>,
    #[serde(default)]
    pub attempt: u32,
    pub processed: u32,
    pub errors: u32,
    pub current_path: Option<String>,
    pub failure: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
#[serde(rename_all = "kebab-case")]
pub enum Ownership {
    External,
    Generated,
    Manual,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct Tag {
    pub value: String,
    pub ownership: Ownership,
    pub engine: String,
    #[serde(default)]
    pub field: String,
}
pub type Specs = BTreeMap<String, Vec<String>>;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct MediaItem {
    #[serde(default)]
    pub inspection: Box<crate::inspector::Inspection>,
    #[serde(default)]
    #[ts(type = "number")]
    pub modified_at: i64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub added_date: String,
    #[serde(default)]
    pub spec_status: String,
    #[serde(default)]
    pub parser_revision: u32,
    pub id: String,
    pub root_id: String,
    pub space: Space,
    pub path: String,
    pub source_hash: String,
    pub title: String,
    #[serde(default)]
    pub series_key: String,
    pub year: String,
    pub imdb: String,
    pub kind: String,
    pub season: String,
    pub episode: String,
    pub specs: Specs,
    pub tags: Vec<Tag>,
    pub error: Option<AppError>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct CatalogQuery {
    pub space: Space,
    pub search: String,
    pub only_errors: bool,
    pub offset: u32,
    pub limit: u32,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, TS)]
pub struct CatalogPage {
    pub total: u32,
    pub items: Vec<MediaItem>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "result", rename_all = "kebab-case")]
pub enum OperationResult {
    Annotation(crate::inspector::Annotation),
    Automatic(crate::automatic::Status),
    Cache(crate::imdb_cache::CacheStatus),
    Fetch(crate::acquisition::FetchRecord),
    Write(crate::writing::WritePreview),
    Ai(Box<crate::ai::job::Record>),
    AiSettings(crate::ai::job::Settings),
    AiRuntime(crate::ai::runtime::State),
    Lifecycle(crate::lifecycle::SettingsOperation),
    Ui(crate::ui::UiReceipt),
    Configuration(Configuration),
    Update(crate::update::UpdateProgress),
    MigrationPlan(crate::migration::MigrationPlan),
    Migration(crate::migration::MigrationReceipt),
    Task(Task),
}

pub fn typescript() -> String {
    let declarations = [
        crate::presentation::Message::decl(),
        crate::presentation::Descriptor::decl(),
        crate::presentation::Catalog::decl(),
        crate::presentation::Pack::decl(),
        Space::decl(),
        Locale::decl(),
        LibraryRoot::decl(),
        LibraryRootAccess::decl(),
        crate::onboarding::RootCandidate::decl(),
        crate::onboarding::Info::decl(),
        Configuration::decl(),
        TaskState::decl(),
        AppError::decl(),
        ScanRequest::decl(),
        TaskControl::decl(),
        Task::decl(),
        crate::automatic::Settings::decl(),
        crate::automatic::Status::decl(),
        crate::automatic::Expected::decl(),
        crate::batch::BatchEngine::decl(),
        crate::batch::BatchMode::decl(),
        crate::batch::ReviewedCandidate::decl(),
        crate::batch::PreviewAdoption::decl(),
        crate::batch::BatchRequest::decl(),
        crate::batch::BatchItem::decl(),
        crate::batch::Batch::decl(),
        Ownership::decl(),
        Tag::decl(),
        crate::inspector::Inspection::decl(),
        crate::inspector::Annotation::decl(),
        crate::inspector::AnnotationAction::decl(),
        crate::inspector::AnnotationRequest::decl(),
        MediaItem::decl(),
        CatalogQuery::decl(),
        CatalogPage::decl(),
        crate::migration::AdapterPlan::decl(),
        crate::imdb_cache::CacheMigrationItem::decl(),
        crate::imdb_cache::CacheSettings::decl(),
        crate::imdb_cache::CacheRequest::decl(),
        crate::imdb_cache::CacheStatus::decl(),
        crate::history::HistoryArchive::decl(),
        crate::history::HistoryArchives::decl(),
        crate::history::HistoryPage::decl(),
        crate::task_log::TaskJournal::decl(),
        crate::task_log::TaskJob::decl(),
        crate::migration::LegacyFile::decl(),
        crate::migration::LegacyRoot::decl(),
        crate::migration::MigrationPlan::decl(),
        crate::migration::MigrationReceipt::decl(),
        crate::update::InstallationIdentity::decl(),
        crate::update::UpdateArtifact::decl(),
        crate::update::UpdateCatalog::decl(),
        crate::update::UpdateProgress::decl(),
        crate::languages::LanguageOption::decl(),
        crate::languages::LanguageSnapshot::decl(),
        crate::ui::Sort::decl(),
        crate::ui::CatalogFilter::decl(),
        crate::ui::MediaLevel::decl(),
        crate::ui::LibraryView::decl(),
        crate::ui::InspectorTab::decl(),
        crate::ui::CatalogColumn::decl(),
        crate::ui::CatalogColumns::decl(),
        crate::ui::PresentationState::decl(),
        crate::ui::UiState::decl(),
        crate::ui::UiReceipt::decl(),
        crate::tv::TvRow::decl(),
        crate::tv::TvPage::decl(),
        crate::lifecycle::CloseAction::decl(),
        crate::lifecycle::Settings::decl(),
        crate::lifecycle::SettingsOperation::decl(),
        crate::writing::SpecsEdit::decl(),
        crate::tags::GeneratedEntry::decl(),
        crate::tags::Action::decl(),
        crate::tags::Plan::decl(),
        crate::writing::TagEdit::decl(),
        crate::writing::WriteIntent::decl(),
        crate::legacy_undo::LegacyUndoProof::decl(),
        crate::legacy_undo::LegacyUndoEntry::decl(),
        crate::legacy_undo::LegacyUndoPage::decl(),
        crate::legacy_undo::LegacyUndoRequest::decl(),
        crate::writing::WritePreview::decl(),
        serde_json::Value::decl(),
        crate::ai::Protocol::decl(),
        crate::ai::Config::decl(),
        crate::ai::Usage::decl(),
        crate::ai::Meter::decl(),
        crate::ai::job::Settings::decl(),
        crate::ai::job::Profile::decl(),
        crate::ai::job::Request::decl(),
        crate::ai::job::Attempt::decl(),
        crate::ai::job::Record::decl(),
        crate::ai::job::Purpose::decl(),
        crate::ai::legacy_cache::Origin::decl(),
        crate::ai::legacy_failure::Failure::decl(),
        crate::ai::runtime::State::decl(),
        crate::specs::SourceStatus::decl(),
        crate::specs::SourceSpecs::decl(),
        crate::acquisition::FetchRequest::decl(),
        crate::acquisition::FetchAttempt::decl(),
        crate::acquisition::FetchRecord::decl(),
        OperationResult::decl(),
    ];
    format!(
        "// Generated from Rust contracts. Do not edit.\n{}\n",
        declarations
            .into_iter()
            .map(|s| format!("export {s}"))
            .collect::<Vec<_>>()
            .join("\n")
    )
}
