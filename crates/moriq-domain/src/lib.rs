//! Pure library identities and UI projections. No I/O or renderer dependencies.
mod ai;
mod jobs;
pub use ai::*;
pub use jobs::{JobKind, JobState, ModelId, TaskError, TaskResult};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! identity {
    ($name:ident) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        pub struct $name(pub Uuid);
        impl Default for $name {
            /// 为领域身份生成新的随机 UUID，避免新对象复用已有身份。
            fn default() -> Self {
                Self(Uuid::new_v4())
            }
        }
        impl std::fmt::Display for $name {
            /// 将领域身份格式化为其 UUID 文本。
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
    };
}
identity!(LibraryId);
identity!(AssetId);
identity!(AssetVersionId);
identity!(ProjectId);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ProjectTemplate {
    #[default]
    Default,
    Portrait,
    Product,
    Travel,
    Existing(ProjectId),
}

#[derive(Debug, Clone)]
pub struct LibraryInfo {
    pub id: LibraryId,
    pub name: String,
    pub count: u64,
    pub favorites: u64,
    pub bytes: u64,
    pub created_at: u64,
    pub updated_at: u64,
    pub projects: Vec<Project>,
}
#[derive(Debug, Clone)]
pub struct Project {
    pub visited_at: u64,
    pub id: ProjectId,
    pub parent: Option<ProjectId>,
    pub name: String,
    pub count: u64,
}

#[derive(Debug, Clone)]
pub struct Asset {
    pub id: AssetId,
    pub version_id: AssetVersionId,
    pub name: String,
    pub format: String,
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub favorite: bool,
    pub rating: u8,
    pub tags: Vec<String>,
    pub imported_at: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Query {
    pub legacy_category: String,
    pub medium: String,
    pub themes: Vec<String>,
    pub metadata: Vec<MetadataPredicate>,
    pub folder: Option<String>,
    pub category: String,
    pub person: Option<String>,
    pub semantic_ids: Vec<AssetId>,
    pub semantic_active: bool,
    pub rule: Option<Rule>,
    pub media_kind: MediaKind,
    pub text: String,
    pub favorites_only: bool,
    pub project: Option<ProjectId>,
    pub project_exact: bool,
    pub format: String,
    pub storage: u8,
    pub min_rating: u8,
    pub min_bytes: u64,
    pub max_bytes: u64,
    pub since: u64,
    pub before: u64,
    pub edited_since: u64,
    pub min_width: u32,
    pub orientation: u8,
    pub untagged: bool,
    pub unassigned: bool,
    pub trash: bool,
    pub tag: String,
    pub color: String,
    pub sort: SortOrder,
}

/// Bounded, typed conditions. SQL and filesystem paths never enter this tree.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct MetadataPredicate {
    pub field: String,
    pub contains: String,
    pub min: Option<i64>,
    pub max: Option<i64>,
}

/// Bounded, typed conditions. SQL and filesystem paths never enter this tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", content = "value", rename_all = "snake_case")]
pub enum Rule {
    All(Vec<Rule>),
    Any(Vec<Rule>),
    Not(Box<Rule>),
    Name(String),
    Notes(String),
    Tag(String),
    Format(String),
    Rating(u8),
    Favorite,
    Raw,
    HasLocation,
}
impl Rule {
    /// 校验规则树的深度、节点及字段预算，拒绝无界或无效筛选。
    pub fn validate(&self) -> Result<(), &'static str> {
        /// 递归检查规则节点并累计预算，供规则树完整校验使用。
        fn visit(rule: &Rule, depth: usize, count: &mut usize) -> Result<(), &'static str> {
            *count += 1;
            if depth > 8 || *count > 64 {
                return Err("组合规则最多 8 层、64 个条件及分组");
            }
            match rule {
                Rule::All(items) | Rule::Any(items) => {
                    if items.is_empty() || items.len() > 64 {
                        return Err("分组需要 1～64 个条件");
                    }
                    for item in items {
                        visit(item, depth + 1, count)?;
                    }
                }
                Rule::Not(item) => visit(item, depth + 1, count)?,
                Rule::Name(s) | Rule::Notes(s) | Rule::Tag(s) | Rule::Format(s) => {
                    if s.trim().is_empty() || s.len() > 1024 {
                        return Err("规则文字需要 1～1024 字节");
                    }
                }
                Rule::Rating(value) if *value > 5 => return Err("评分范围为 0～5"),
                _ => {}
            }
            Ok(())
        }
        visit(self, 0, &mut 0)
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    #[default]
    All,
    Raw,
    Video,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortOrder {
    #[default]
    Newest,
    Oldest,
    Name,
    Largest,
    Rating,
}
#[derive(Debug, Clone, Default)]
pub struct AssetDetails {
    pub project_copies: Vec<ProjectId>,
    pub medium: String,
    pub themes: Vec<String>,
    pub ai_tags: Vec<String>,
    pub manual_tags: Vec<String>,
    pub manual_medium: bool,
    pub manual_themes: bool,
    pub manual_description: bool,
    pub ai_caption: String,
    pub ocr: String,
    pub category: String,
    pub faces: Vec<Face>,
    pub manual_location: Option<AssetLocation>,
    pub exif_location: Option<AssetLocation>,
    pub notes: String,
    pub projects: Vec<ProjectId>,
    pub exif: Vec<(String, String)>,
    pub referenced: bool,
    pub versions: Vec<AssetVersion>,
}
#[derive(Debug, Clone)]
pub struct AssetVersion {
    pub id: AssetVersionId,
    pub origin: String,
    pub parent: Option<AssetVersionId>,
}
#[derive(Debug, Clone)]
pub struct SyncoTask {
    pub id: String,
    pub name: String,
    pub state: String,
    pub error: String,
    pub request: String,
    pub ack: Option<String>,
}
#[derive(Debug, Clone)]
pub struct Collection {
    pub id: String,
    pub name: String,
    pub query: Query,
}
#[derive(Debug, Clone, Default)]
pub struct Organizer {
    pub ai_profile: AiProfile,
    pub ai_groups: Vec<AiGroup>,
    pub ai_outdated: u64,
    pub errors: Vec<(OrganizerSection, String)>,
    pub quarantined_files: u64,
    pub quarantined_bytes: u64,
    pub folders: Vec<VirtualFolder>,
    pub ai_categories: Vec<(String, u64)>,
    pub people: Vec<Person>,
    pub video_count: u64,
    pub jobs: Vec<BackgroundJob>,
    pub raw_count: u64,
    pub tags: Vec<(String, u64)>,
    pub collections: Vec<Collection>,
    pub trash_count: u64,
    pub recent_count: u64,
    pub untagged_count: u64,
    pub unassigned_count: u64,
    pub high_rating_count: u64,
    pub edited_count: u64,
    pub interrupted: u64,
    pub description: String,
    pub synco: Vec<SyncoTask>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrganizerSection {
    Folders,
    Jobs,
    People,
    Categories,
    Tags,
    Collections,
    Statistics,
    Synco,
    Profile,
}
#[derive(Debug, Clone)]
pub struct BackgroundJob {
    pub id: String,
    pub kind: JobKind,
    pub state: JobState,
    pub priority: i32,
    pub completed: u64,
    pub total: u64,
    pub failed: u64,
    pub error: String,
}
#[derive(Debug, Clone, Copy)]
pub enum JobCommand {
    RetryFailed,
    Pause,
    Resume,
    Cancel,
    Priority(i32),
}
#[derive(Debug, Clone)]
pub struct JobFailure {
    pub asset: Option<AssetId>,
    pub name: String,
    pub message: String,
}
#[derive(Debug, Clone)]
pub struct SimilarAsset {
    pub id: AssetId,
    pub name: String,
    pub distance: u32,
}
#[derive(Debug, Clone)]
pub struct Person {
    pub id: String,
    pub name: String,
    pub count: u64,
}
#[derive(Debug, Clone)]
pub struct VirtualFolder {
    pub id: String,
    pub name: String,
    pub favorite_group: bool,
    pub cover: Option<AssetId>,
    pub count: u64,
}
#[derive(Debug, Clone)]
pub enum FolderEdit {
    Create(String, bool),
    Rename(String, String),
    Delete(String),
    Members(String, Vec<AssetId>, bool),
    Cover(String, AssetId),
}
#[derive(Debug, Clone)]
pub struct Face {
    pub id: String,
    pub version: AssetVersionId,
    pub person: Option<String>,
    pub name: String,
    pub bounds: [f32; 4],
}

#[derive(Debug, Clone, Default)]
pub struct UserSettings {
    pub category_template: Option<AiProfile>,
    pub synco: SyncoPreferences,
    pub ai: AiPreferences,
    pub recent: Vec<(String, std::path::PathBuf)>,
    pub import: ImportPreferences,
    pub diagnostic_logging: bool,
    pub reduced_motion: bool,
    pub language: Language,
    pub appearance: Appearance,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Language {
    #[default]
    SimplifiedChinese,
    English,
}
impl std::fmt::Display for Language {
    /// 把语言枚举格式化为用户可读的语言名称。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::SimplifiedChinese => "简体中文",
            Self::English => "English",
        })
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Appearance {
    #[default]
    Light,
    Dark,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncoPreferences {
    pub executable: Option<std::path::PathBuf>,
    pub auto_start: bool,
}
impl Default for SyncoPreferences {
    /// 构造默认 Synco 偏好，允许自动启动且不预设可执行文件路径。
    fn default() -> Self {
        Self {
            executable: None,
            auto_start: true,
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AiPreferences {
    pub on_demand: bool,
    pub retain_indexes: bool,
    pub use_mirror: bool,
}
impl Default for AiPreferences {
    /// 构造按需加载、保留索引并使用镜像的默认 AI 偏好。
    fn default() -> Self {
        Self {
            on_demand: true,
            retain_indexes: true,
            use_mirror: true,
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedVisit {
    pub library_picker: bool,
    pub screen: u8,
    pub query: Query,
    pub scroll_y: f32,
    pub selected: Option<AssetId>,
    pub list: bool,
    pub masonry: bool,
    pub card_width: f32,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedNavigation {
    pub current: SavedVisit,
    pub back: Vec<SavedVisit>,
    pub forward: Vec<SavedVisit>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ImportPreferences {
    /// Zero selects the hardware-based default. Explicit limits are 1..=32.
    pub workers: usize,
    pub memory_mib: u32,
}
impl Default for ImportPreferences {
    /// 构造自动选择并发数的导入偏好，并提供缺省内存预算。
    fn default() -> Self {
        Self {
            workers: 0,
            memory_mib: 2048,
        }
    }
}
impl ImportPreferences {
    /// 结合处理器数量和用户偏好计算实际导入并发，保留界面处理余量。
    pub fn effective_workers(self, logical_cpus: usize) -> usize {
        if self.workers == 0 {
            (logical_cpus / 2).clamp(2, 16)
        } else {
            self.workers.clamp(1, 32)
        }
    }
    /// 校验并规范化导入并发与内存设置。
    pub fn validated(self) -> Option<Self> {
        (self.workers <= 32 && (256..=4096).contains(&self.memory_mib)).then_some(self)
    }
}

#[derive(Debug, Clone)]
pub enum Edit {
    Trash(bool),
    Favorite(bool),
    Rating(u8),
    AddTags(Vec<String>),
    RemoveTags(Vec<String>),
    Project(ProjectId, bool),
    Notes(String),
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchPatch {
    pub favorite: Option<bool>,
    pub rating: Option<u8>,
    pub notes: Option<String>,
    pub category: Option<String>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub project: Option<ProjectId>,
}
#[derive(Debug, Clone)]
pub enum BatchScope {
    Assets(Vec<AssetId>),
    Query(Box<Query>),
}
#[derive(Debug, Clone, Default)]
pub struct PurgeReport {
    pub deleted: usize,
    pub cleanup_pending: bool,
}
#[derive(Debug, Clone, Default)]
pub struct CacheReport {
    pub removed_files: u64,
    pub removed_bytes: u64,
    pub remaining_bytes: u64,
    pub skipped: u64,
}
#[derive(Debug, Clone, Default)]
pub struct OrphanReport {
    pub moved: u64,
    pub bytes: u64,
    pub skipped: u64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ImageRegion {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub level: u32,
}

/// Frozen, bounded deletion scope shown to the user and revalidated before file moves.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectDeletionPlan {
    pub root: ProjectId,
    pub name: String,
    pub projects: Vec<ProjectId>,
    pub assets: Vec<AssetId>,
    pub shared: Vec<AssetId>,
    pub versions: Vec<AssetVersionId>,
    pub aliases: Vec<String>,
    pub referenced: u64,
}
#[derive(Debug, Clone, Default)]
pub struct MaintenanceReport {
    pub cancelled: bool,
    pub checked: u64,
    pub missing: u64,
    pub damaged: u64,
    pub repaired: u64,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Page {
    pub total: u64,
    pub offset: usize,
    pub assets: Vec<Asset>,
}

#[derive(Debug, Clone)]
pub struct Neighborhood {
    pub selected: AssetId,
    pub position: usize,
    pub page: Page,
}

#[derive(Debug, Clone, Default)]
pub struct ImportReport {
    pub imported: usize,
    pub duplicates: usize,
    pub failed: usize,
    pub failures: Vec<String>,
    pub cancelled: bool,
}
#[derive(Debug, Clone)]
pub struct ImportFile {
    pub path: std::path::PathBuf,
    pub relative: std::path::PathBuf,
    pub project_name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ExportFormat {
    #[default]
    Original,
    Jpeg,
    Png,
    Webp,
    Tiff,
    Bmp,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Resampling {
    #[default]
    Area,
    Lanczos3,
    Bicubic,
    Nearest,
}
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ExportLayout {
    #[default]
    Flat,
    Format,
    ImportDate,
}
#[derive(Debug, Clone)]
pub struct ExportOptions {
    pub directory_template: String,
    pub preserve_exif: bool,
    pub format: ExportFormat,
    pub max_edge: u32,
    pub quality: u8,
    pub rotate: u16,
    pub resampling: Resampling,
    pub layout: ExportLayout,
    pub metadata_sidecar: bool,
}
impl Default for ExportOptions {
    /// 构造保留原文件格式的默认导出选项及其他转换缺省值。
    fn default() -> Self {
        Self {
            directory_template: String::new(),
            preserve_exif: false,
            format: ExportFormat::Original,
            max_edge: 0,
            quality: 90,
            rotate: 0,
            resampling: Resampling::Area,
            layout: ExportLayout::Flat,
            metadata_sidecar: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    /// 验证自动导入并发保留 CPU 余量，并校验用户覆盖值。
    fn automatic_import_limits_leave_cpu_headroom_and_validate_overrides() {
        let auto = ImportPreferences::default();
        assert_eq!(auto.effective_workers(20), 10);
        assert_eq!(auto.effective_workers(8), 4);
        assert_eq!(auto.effective_workers(1), 2);
        assert_eq!(
            ImportPreferences {
                workers: 24,
                ..auto
            }
            .effective_workers(20),
            24
        );
        assert!(
            ImportPreferences {
                workers: 33,
                ..auto
            }
            .validated()
            .is_none()
        );
        assert!(
            ImportPreferences {
                memory_mib: 0,
                ..auto
            }
            .validated()
            .is_none()
        );
    }
}

#[derive(Debug, Clone)]
pub struct LibraryEvent {
    pub id: i64,
    pub at: u64,
    pub kind: String,
    pub detail: String,
    pub asset: Option<AssetId>,
    pub project: Option<ProjectId>,
    pub unread: bool,
    pub retry: String,
}
#[derive(Debug, Clone, Default)]
pub struct EventPage {
    pub items: Vec<LibraryEvent>,
    pub unread: u64,
    pub more: bool,
}
#[derive(Debug, Clone)]
pub struct Notice {
    pub title: String,
    pub detail: String,
    pub level: u8,
    pub asset: Option<AssetId>,
    pub retry: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AssetLocation {
    pub latitude: f64,
    pub longitude: f64,
    pub label: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageStats {
    pub allocated: u64,
    pub unique_file_bytes: u64,
    pub categories: Vec<(String, u64)>,
    pub managed_registered: u64,
    pub referenced_registered: u64,
    pub volume_total: u64,
    pub volume_available: u64,
    pub files: u64,
    pub linked_paths: u64,
    pub warnings: u64,
    pub cancelled: bool,
    pub measured_at: u64,
}
impl AssetLocation {
    /// 检查地点经纬度及相关字段是否落在有效范围。
    pub fn valid(&self) -> bool {
        self.latitude.is_finite()
            && self.longitude.is_finite()
            && (-90.0..=90.0).contains(&self.latitude)
            && (-180.0..=180.0).contains(&self.longitude)
            && self.label.len() <= 1024
            && self.label.chars().count() <= 256
    }
    /// 根据已校验经纬度生成地图查看链接。
    pub fn map_url(&self) -> Option<String> {
        self.valid().then(|| {
            format!(
                "https://www.openstreetmap.org/?mlat={}&mlon={}#map=14/{}/{}",
                self.latitude, self.longitude, self.latitude, self.longitude
            )
        })
    }
}
