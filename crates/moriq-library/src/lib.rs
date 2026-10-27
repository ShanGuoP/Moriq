//! Blocking infrastructure. Call only from the application's dedicated worker.
use fs2::FileExt;
use image::{GenericImageView, ImageDecoder, ImageFormat, ImageReader};
use moriq_domain::*;
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};
use uuid::Uuid;

const SCHEMA: u32 = 26;
// Library
#[path = "library/core.rs"]
mod library_core;
#[path = "library/library_cover.rs"]
pub mod library_cover;
#[path = "library/library_removal.rs"]
pub mod library_removal;
#[path = "library/lifecycle.rs"]
pub mod lifecycle;
#[path = "library/settings.rs"]
pub mod settings;

// Catalog
#[path = "catalog/batch.rs"]
mod batch;
#[path = "catalog/catalog.rs"]
mod catalog;
#[path = "catalog/edits.rs"]
mod catalog_edits;
#[path = "catalog/query.rs"]
mod catalog_query;
#[path = "catalog/folders.rs"]
mod folders;
#[path = "catalog/location.rs"]
mod location;
#[path = "catalog/organize.rs"]
mod organize;
#[path = "catalog/overview.rs"]
pub mod overview;
#[path = "catalog/project_deletion.rs"]
mod project_deletion;
#[path = "catalog/project_isolation.rs"]
mod project_isolation;
#[path = "catalog/rules.rs"]
mod rules;
#[path = "catalog/workspace.rs"]
mod workspace;

// Media
#[path = "media/cache.rs"]
pub mod cache;
#[path = "media/color.rs"]
mod color;
#[path = "media/media.rs"]
pub mod media;
#[path = "media/access.rs"]
mod media_access;
#[path = "media/metadata.rs"]
pub mod metadata;
#[path = "media/native.rs"]
pub mod native;
#[cfg(target_os = "macos")]
#[path = "media/raw_portable.rs"]
mod raw_portable;
#[path = "media/region.rs"]
pub mod region;
#[path = "media/resize.rs"]
mod resize;
#[path = "media/vector.rs"]
mod vector;
#[path = "media/video.rs"]
pub mod video;

// AI
#[path = "ai/ai_content.rs"]
mod ai_content;
#[path = "ai/ai_profile.rs"]
pub mod ai_profile;
#[path = "ai/analysis.rs"]
pub mod analysis;
#[path = "ai/portraits.rs"]
pub mod portraits;
#[path = "ai/qwen_faces.rs"]
pub mod qwen_faces;
#[path = "ai/similarity.rs"]
pub mod similarity;

// Transfer
#[path = "transfer/export.rs"]
pub mod export;
#[path = "transfer/export_metadata.rs"]
mod export_metadata;
#[path = "transfer/import.rs"]
pub mod import;
#[path = "transfer/commit.rs"]
mod import_commit;
#[path = "transfer/ingest.rs"]
pub mod ingest;
#[path = "transfer/synco.rs"]
mod synco;

// Tasks
#[path = "tasks/cancel.rs"]
mod cancel;
#[path = "tasks/jobs.rs"]
pub mod jobs;

// Maintenance
#[path = "maintenance/backup.rs"]
pub mod backup;
#[path = "maintenance/maintenance.rs"]
pub mod maintenance;
#[path = "maintenance/orphans.rs"]
pub mod orphans;
#[path = "maintenance/purge.rs"]
mod purge;
#[path = "maintenance/storage.rs"]
pub mod storage;

// Database
#[path = "database/journal.rs"]
mod journal;
#[path = "database/read_pool.rs"]
mod read_pool;

// Tests
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/ai_content_tests.rs"]
mod ai_content_tests;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/ai_profile_tests.rs"]
mod ai_profile_tests;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/project_isolation_tests.rs"]
mod project_isolation_tests;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/storage_regression_tests.rs"]
mod storage_regression_tests;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/library.rs"]
mod tests;
#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[path = "tests/workspace_tests.rs"]
mod workspace_tests;

pub use catalog::validate_reference;
const MAX_BYTES: u64 = 256 * 1024 * 1024;
const DIRECTORIES: &[&str] = &[
    "assets",
    "versions",
    "projects",
    "thumbnails/256",
    "previews",
    "indexes/search",
    "indexes/layout",
    "indexes/ai",
    "metadata",
    "interop/outgoing",
    "staging",
    "trash",
    "recovery",
    "backups",
];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("文件读写失败，请检查目录权限和剩余空间：{0}")]
    Io(#[from] std::io::Error),
    #[error("资源库数据库操作失败：{0}")]
    Db(#[from] rusqlite::Error),
    #[error("图片无法解码，请检查文件是否损坏：{0}")]
    Image(#[from] image::ImageError),
    #[error("色彩配置无法转换：{0}")]
    Color(String),
    #[error("{0}")]
    Invalid(String),
    #[error("资源库正由另一个窗口或进程使用，请先关闭它")]
    Locked,
}
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    format: String,
    library_id: LibraryId,
    name: String,
    schema_version: u32,
    min_reader_version: u32,
    created_at_utc: u64,
}

pub struct Library {
    workspace: Option<ProjectId>,
    organizer_cache: std::cell::RefCell<std::collections::BTreeMap<String, (u64, u32, Organizer)>>,
    root: PathBuf,
    manifest: Manifest,
    db: Connection,
    _lock: std::sync::Arc<File>,
    readers: std::sync::Arc<read_pool::ReadPool>,
    snapshot: Option<(Query, u64)>,
    last_import: ImportMetrics,
}
#[derive(Debug, Clone, Default)]
pub struct ImportMetrics {
    pub copy_ms: f64,
    pub hash_ms: f64,
    pub decode_ms: f64,
    pub thumbnail_ms: f64,
    pub commit_ms: f64,
}
#[derive(Debug, Clone)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: bytes::Bytes,
}

/// 返回 Unix 纪元以来的毫秒数；系统时钟早于纪元时返回错误。
pub fn now_ms() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|v| v.as_millis() as u64)
        .map_err(|_| Error::Invalid("系统时间早于 UTC 1970，请校准时钟".into()))
}

/// Managed paths are generated by the application, never taken from filenames.
///
/// 将受管相对路径解析到资源库内，拒绝目录穿越、绝对路径和指向库外的链接。
pub fn safe_path(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = Path::new(relative);
    if path.as_os_str().is_empty()
        || relative.contains(['\\', ':'])
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(Error::Invalid("资源库中的文件路径无效，已停止访问".into()));
    }
    let canonical_root = root.canonicalize()?;
    let mut current = canonical_root.clone();
    for part in path.components() {
        current.push(part.as_os_str());
        match fs::symlink_metadata(&current) {
            Ok(_) => {
                if !current.canonicalize()?.starts_with(&canonical_root) {
                    return Err(Error::Invalid("资源库路径指向库外，已停止访问".into()));
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(current)
}

/// Explicit diagnostics output; never used for library content.
#[cfg(feature = "capture")]
/// 将指定尺寸的 RGBA 像素写为诊断用 PNG，不参与资源库素材存储。
pub fn save_capture(path: &Path, width: u32, height: u32, rgba: &[u8]) -> Result<()> {
    image::save_buffer_with_format(
        path,
        rgba,
        width,
        height,
        image::ColorType::Rgba8,
        ImageFormat::Png,
    )?;
    Ok(())
}
/// 在目标目录试建硬链接，提前确认该卷能够满足资源库的文件关联要求。
fn require_hardlinks(root: &Path) -> Result<()> {
    let probe = tempfile::NamedTempFile::new_in(root)?;
    let link = root.join(format!(".moriq-link-probe-{}", Uuid::new_v4()));
    fs::hard_link(probe.path(), &link).map_err(|error| Error::Invalid(format!(
        "此位置无法创建资源库所需的硬链接（{error}）。请将资源库放在支持硬链接的 NTFS、APFS 或 ext4 卷上。"
    )))?;
    fs::remove_file(link)?;
    Ok(())
}

/// 解析数据库中的 UUID；格式损坏时转换为可向用户报告的资源库错误。
fn parse_id(value: &str) -> Result<Uuid> {
    Uuid::parse_str(value).map_err(|_| Error::Invalid("资源库包含无效标识，请检查备份".into()))
}
/// 把解码图像转换为统一的 RGBA 像素缓冲及尺寸信息。
fn pixels(image: image::DynamicImage) -> Pixels {
    let (width, height) = image.dimensions();
    Pixels {
        width,
        height,
        rgba: image.into_rgba8().into_raw().into(),
    }
}
/// 构造通用解码尺寸和分配上限，限制损坏媒体引起的过量内存分配。
fn decode_limits() -> image::Limits {
    let mut limits = image::Limits::default();
    limits.max_image_width = None;
    limits.max_image_height = None;
    limits.max_alloc = Some(384 * 1024 * 1024);
    limits
}
