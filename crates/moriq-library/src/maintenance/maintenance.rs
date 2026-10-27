//! Bounded media checking outside the SQLite writer. Only verified cache results cross back.
use super::*;
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct Control {
    pub cancel: Arc<AtomicBool>,
    pub finished: Arc<AtomicUsize>,
    pub total: Arc<AtomicUsize>,
}

pub struct Plan {
    readers: Arc<read_pool::ReadPool>,
    root: PathBuf,
    _lease: Arc<File>,
}

pub struct Repair {
    version: String,
    hash: String,
    source: PathBuf,
    thumbnail: Option<tempfile::NamedTempFile>,
    color: Option<String>,
}

pub enum Outcome {
    Missing(String),
    Damaged(String),
    Warning(String),
    Valid(Option<Repair>),
}

impl Outcome {
    /// Shared accounting for synchronous tools and the asynchronous coordinator.
    /// Repairs remain version-checked mutations on the library writer.
    ///
    /// 把单个检查结果计入报告，并返回仍需写入端复核的修复候选。
    pub fn record(self, report: &mut MaintenanceReport) -> Option<Repair> {
        report.checked += 1;
        let warning = match self {
            Self::Missing(version) => {
                report.missing += 1;
                Some(format!("版本 {version} 文件缺失"))
            }
            Self::Damaged(version) => {
                report.damaged += 1;
                Some(format!("版本 {version} 文件校验不符"))
            }
            Self::Warning(warning) => Some(warning),
            Self::Valid(repair) => return repair,
        };
        if report.warnings.len() < 50
            && let Some(warning) = warning
        {
            report.warnings.push(warning);
        }
        None
    }
}

impl Library {
    /// 冻结检查所需的目录、读取连接池和资源库租约。
    pub fn maintenance_plan(&self) -> Plan {
        Plan {
            readers: self.readers.clone(),
            root: self.root.clone(),
            _lease: self._lock.clone(),
        }
    }

    /// 复核版本校验和与来源路径后提交缓存修复；过期修复返回未应用。
    pub fn apply_cache_repair(&mut self, repair: Repair) -> Result<bool> {
        let hash: Option<String> = self
            .db
            .query_row(
                "SELECT sha256 FROM asset_versions WHERE id=?1",
                [&repair.version],
                |r| r.get(0),
            )
            .optional()?;
        if hash.as_deref() != Some(&repair.hash)
            || self.version_path(&repair.version, false)? != repair.source
        {
            return Ok(false);
        }
        if let Some(thumbnail) = repair.thumbnail {
            let destination = safe_path(
                &self.root,
                &format!("thumbnails/256/{}.png", repair.version),
            )?;
            thumbnail
                .persist(destination)
                .map_err(|e| Error::Io(e.error))?;
        }
        if let Some(color) = repair.color {
            self.db.execute(
                "UPDATE asset_versions SET dominant_color=?1 WHERE id=?2",
                params![color, repair.version],
            )?;
            self.snapshot = None;
        }
        Ok(true)
    }
}

impl Plan {
    /// A private, file-backed SQLite temp snapshot bounds Rust memory and releases the WAL read
    /// transaction before media I/O. Imports and edits can continue on the library writer.
    ///
    /// 在写入线程外检查来源和缓存，逐项报告结果并响应取消。
    pub fn run(
        self,
        repair: bool,
        control: &Control,
        mut emit: impl FnMut(Outcome) -> bool,
    ) -> Result<()> {
        if control.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        let connection = self.readers.checkout()?;
        let db = connection.connection()?;
        db.busy_timeout(std::time::Duration::from_secs(5))?;
        let integrity: String = db.query_row("PRAGMA integrity_check", [], |r| r.get(0))?;
        if integrity != "ok" {
            return Err(Error::Invalid(format!(
                "数据库完整性检查未通过：{integrity}"
            )));
        }
        if db.prepare("PRAGMA foreign_key_check")?.exists([])? {
            return Err(Error::Invalid(
                "数据库包含失效的引用，请保留原库并从备份恢复".into(),
            ));
        }
        if control.cancel.load(Ordering::Relaxed) {
            return Ok(());
        }
        db.execute_batch("PRAGMA temp_store=FILE; PRAGMA temp.cache_size=-2048; CREATE TEMP TABLE maintenance_snapshot AS \
                SELECT v.id,v.path,v.sha256,v.dominant_color,f.platform,f.native_path FROM asset_versions v LEFT \
                JOIN file_locations f ON f.version_id=v.id;")?;

        let total: usize = db.query_row("SELECT count(*) FROM maintenance_snapshot", [], |r| {
            r.get(0)
        })?;
        control.total.store(total, Ordering::Relaxed);
        let mut statement = db.prepare(
            "SELECT id,path,sha256,dominant_color,platform,native_path FROM maintenance_snapshot",
        )?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, Option<String>>(4)?,
                row.get::<_, Option<Vec<u8>>>(5)?,
            ))
        })?;
        for row in rows {
            if control.cancel.load(Ordering::Relaxed) {
                break;
            }
            let (version, relative, hash, color, platform, native) = row?;
            let outcome = (|| {
                let source = match (platform, native) {
                    (Some(platform), Some(bytes)) => {
                        let path = organize::decode_path(&platform, &bytes)?;
                        if !path.is_absolute() {
                            return Err(Error::Invalid("引用路径无效".into()));
                        }
                        path
                    }
                    (None, None) => safe_path(&self.root, &relative)?,
                    _ => return Err(Error::Invalid("引用路径记录不完整".into())),
                };
                if !source.is_file() {
                    return Ok(Outcome::Missing(version.clone()));
                }
                let thumbnail_path =
                    safe_path(&self.root, &format!("thumbnails/256/{version}.png"))?;
                let valid_thumbnail = !repair
                    || (|| -> Result<()> {
                        let mut reader =
                            ImageReader::open(&thumbnail_path)?.with_guessed_format()?;
                        let mut limits = image::Limits::default();
                        limits.max_image_width = Some(256);
                        limits.max_image_height = Some(256);
                        limits.max_alloc = Some(1024 * 1024);
                        reader.limits(limits);
                        reader.decode()?;
                        Ok(())
                    })()
                    .is_ok();
                // Freeze only files that need decoding. The decoded bytes are exactly those hashed,
                // even if an external editor changes a referenced source during this task.
                let mut frozen = if repair && (!valid_thumbnail || color.is_empty()) {
                    let required = fs::metadata(&source)?
                        .len()
                        .saturating_add(64 * 1024 * 1024);
                    if fs2::available_space(&self.root)? < required {
                        return Err(Error::Invalid(
                            "剩余空间不足，无法暂存源文件以修复缓存".into(),
                        ));
                    }
                    Some(tempfile::NamedTempFile::new_in(safe_path(
                        &self.root, "staging",
                    )?)?)
                } else {
                    None
                };
                let mut file = File::open(&source)?;
                let mut hasher = Sha256::new();
                let mut buffer = [0u8; 65536];
                let mut bytes = 0u64;
                loop {
                    if control.cancel.load(Ordering::Relaxed) {
                        return Ok(Outcome::Valid(None));
                    }
                    let length = file.read(&mut buffer)?;
                    if length == 0 {
                        break;
                    }
                    bytes += length as u64;
                    if bytes > 64 * 1024 * 1024 * 1024 {
                        return Ok(Outcome::Damaged(version.clone()));
                    }
                    hasher.update(&buffer[..length]);
                    if let Some(frozen) = &mut frozen {
                        frozen.write_all(&buffer[..length])?;
                    }
                }
                if format!("{:x}", hasher.finalize()) != hash {
                    return Ok(Outcome::Damaged(version.clone()));
                }
                let Some(frozen) = frozen else {
                    return Ok(Outcome::Valid(None));
                };
                let image = media::raster(File::open(frozen.path())?, &source, 256)?
                    .image
                    .ok_or_else(|| Error::Invalid("缓存修复解码无像素".into()))?;
                let thumbnail = if !valid_thumbnail {
                    let mut temp =
                        tempfile::NamedTempFile::new_in(safe_path(&self.root, "staging")?)?;
                    image.write_to(&mut temp, ImageFormat::Png)?;
                    temp.as_file().sync_all()?;
                    Some(temp)
                } else {
                    None
                };
                Ok(Outcome::Valid(Some(Repair {
                    version: version.clone(),
                    hash,
                    source,
                    thumbnail,
                    color: color
                        .is_empty()
                        .then(|| organize::dominant_color(&image).to_string()),
                })))
            })();
            if control.cancel.load(Ordering::Relaxed) {
                break;
            }
            let outcome = outcome.unwrap_or_else(|error: Error| {
                Outcome::Warning(format!("版本 {version}：{error}"))
            });
            if !emit(outcome) {
                break;
            }
            control.finished.fetch_add(1, Ordering::Relaxed);
        }
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    /// 准备当前用例的独立素材和测试资源，由临时目录句柄负责清理。
    fn fixture() -> (tempfile::TempDir, Library) {
        let dir = tempfile::tempdir().unwrap();
        let mut library = Library::create(dir.path(), "维护测试").unwrap();
        for n in 0..3u8 {
            let path = dir.path().join(format!("{n}.png"));
            image::RgbImage::from_pixel(32, 24, image::Rgb([n * 60, 40, 90]))
                .save(&path)
                .unwrap();
            library.import(&path, None).unwrap();
        }
        (dir, library)
    }
    #[test]
    /// 验证快照修复不阻塞编辑，也不会把之后导入混入检查范围。
    fn snapshot_repair_allows_writer_edits_and_does_not_include_new_imports() {
        let (dir, mut library) = fixture();
        let assets = library.page(&Query::default(), 0, 10).unwrap().assets;
        let target = assets[0].version_id;
        fs::write(
            library.root.join(format!("thumbnails/256/{target}.png")),
            b"broken",
        )
        .unwrap();
        let plan = library.maintenance_plan();
        let control = Control::default();
        let mut checked = 0;
        let mut repaired = 0;
        plan.run(true, &control, |outcome| {
            checked += 1;
            if checked == 1 {
                library
                    .edit(&[assets[0].id], &Edit::Notes("检查期间仍可编辑".into()))
                    .unwrap();
                let path = dir.path().join("新图片.png");
                image::RgbImage::from_pixel(32, 24, image::Rgb([250, 2, 3]))
                    .save(&path)
                    .unwrap();
                library.import(&path, None).unwrap();
            }
            match outcome {
                Outcome::Valid(Some(repair)) => {
                    repaired += usize::from(library.apply_cache_repair(repair).unwrap())
                }
                Outcome::Valid(None) => {}
                _ => panic!("unexpected maintenance outcome"),
            }
            true
        })
        .unwrap();
        assert_eq!((checked, repaired), (3, 1));
        assert_eq!(control.total.load(Ordering::Relaxed), 3);
        assert_eq!(control.finished.load(Ordering::Relaxed), 3);
        assert_eq!(library.info().unwrap().count, 4);
        assert!(library.thumbnail(target).unwrap().is_some());
        assert_eq!(
            library.details(assets[0].id).unwrap().notes,
            "检查期间仍可编辑"
        );
    }
    #[test]
    /// 验证取消维护丢弃待提交修复并释放读取租约。
    fn cancellation_drops_pending_repairs_and_releases_library_lease() {
        let (dir, library) = fixture();
        let root = library.root.clone();
        let plan = library.maintenance_plan();
        drop(library);
        assert!(matches!(Library::open(&root), Err(Error::Locked)));
        let control = Control::default();
        let mut count = 0;
        plan.run(true, &control, |_| {
            count += 1;
            control.cancel.store(true, Ordering::Relaxed);
            true
        })
        .unwrap();
        assert_eq!(count, 1);
        assert_eq!(control.finished.load(Ordering::Relaxed), 1);
        let reopened = Library::open(&root).unwrap();
        assert_eq!(reopened.info().unwrap().count, 3);
        assert_eq!(
            fs::read_dir(reopened.root.join("staging")).unwrap().count(),
            0
        );
        drop(dir);
    }
    #[test]
    /// 验证过期修复不会覆盖重定位后的外部引用缓存。
    fn stale_repair_does_not_overwrite_relocated_reference_cache() {
        let (dir, mut library) = fixture();
        let source = dir.path().join("外部.png");
        image::RgbImage::from_pixel(24, 16, image::Rgb([9, 8, 7]))
            .save(&source)
            .unwrap();
        library.import_referenced(&source, None).unwrap();
        let asset = library
            .page(
                &Query {
                    storage: 2,
                    ..Default::default()
                },
                0,
                10,
            )
            .unwrap()
            .assets
            .remove(0);
        let cache = library
            .root
            .join(format!("thumbnails/256/{}.png", asset.version_id));
        fs::remove_file(&cache).unwrap();
        let plan = library.maintenance_plan();
        let mut pending = None;
        plan.run(true, &Control::default(), |outcome| {
            if let Outcome::Valid(Some(repair)) = outcome {
                pending = Some(repair)
            }
            true
        })
        .unwrap();
        library
            .db
            .execute(
                "UPDATE asset_versions SET sha256=replace(hex(zeroblob(32)),'0','a') WHERE id=?1",
                [asset.version_id.to_string()],
            )
            .unwrap();
        assert!(!library.apply_cache_repair(pending.unwrap()).unwrap());
        assert!(!cache.exists());
        assert!(source.is_file());
    }
}
