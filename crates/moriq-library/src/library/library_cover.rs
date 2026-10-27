//! Prepare a cover off the writer, then commit only to the original live asset version.
use super::*;
use std::sync::Arc;

pub struct Plan {
    root: PathBuf,
    owner: LibraryId,
    asset: AssetId,
    version: AssetVersionId,
    source: media::Source,
    lease: Arc<File>,
}
pub struct Prepared {
    root: PathBuf,
    owner: LibraryId,
    asset: AssetId,
    version: AssetVersionId,
    file: tempfile::NamedTempFile,
    _lease: Arc<File>,
}
impl Plan {
    /// 在媒体工作线程生成独立的封面图片，返回供资源库写入端提交的准备结果。
    pub fn render(self) -> Result<Prepared> {
        let image = self.source.preview_edge(960)?.image;
        let mut file = tempfile::NamedTempFile::new_in(safe_path(&self.root, "metadata")?)?;
        image::write_buffer_with_format(
            &mut file,
            &image.rgba,
            image.width,
            image.height,
            image::ColorType::Rgba8,
            ImageFormat::Png,
        )?;
        file.as_file().sync_all()?;
        Ok(Prepared {
            root: self.root,
            owner: self.owner,
            asset: self.asset,
            version: self.version,
            file,
            _lease: self.lease,
        })
    }
}
impl Library {
    /// 确认封面素材及版本仍然有效，阻止过期或不匹配的封面请求。
    fn check_cover_asset(&self, asset: AssetId, version: AssetVersionId) -> Result<()> {
        let current: bool = self.db.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND display_version_id=?2 AND deleted_at IS NULL)",
            params![asset.to_string(), version.to_string()],
            |r| r.get(0),
        )?;
        if !current {
            return Err(Error::Invalid(
                "素材已移入回收站或版本已变化，请重新选择封面".into(),
            ));
        }
        Ok(())
    }
    /// 冻结封面素材的读取来源和版本身份，供后续解码使用。
    pub fn asset_cover_plan(&self, asset: AssetId, version: AssetVersionId) -> Result<Plan> {
        self.check_cover_asset(asset, version)?;
        Ok(Plan {
            root: self.root.clone(),
            owner: self.id(),
            asset,
            version,
            source: self.version_source(asset, version)?,
            lease: self._lock.clone(),
        })
    }
    /// 重新校验素材身份后发布已生成封面，避免失败结果替换原有封面。
    pub fn commit_asset_cover(&self, prepared: Prepared) -> Result<()> {
        if prepared.owner != self.id() || prepared.root != self.root {
            return Err(Error::Invalid("资源库已变化，封面操作已停止".into()));
        }
        self.check_cover_asset(prepared.asset, prepared.version)?;
        prepared
            .file
            .persist(safe_path(&self.root, "metadata/cover.png")?)
            .map_err(|e| Error::Io(e.error))?;
        self.touch_metadata()?;
        Ok(())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    /// 验证封面为独立持久副本，更新失败保留原封面。
    fn asset_cover_is_an_independent_durable_copy_and_failed_changes_keep_the_previous_cover() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("original.png");
        image::RgbImage::from_pixel(1400, 800, image::Rgb([40, 80, 120]))
            .save(&original)
            .unwrap();
        let original_bytes = fs::read(&original).unwrap();
        let mut lib = Library::create(dir.path(), "封面").unwrap();
        lib.import(&original, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let prepared = lib
            .asset_cover_plan(asset.id, asset.version_id)
            .unwrap()
            .render()
            .unwrap();
        lib.commit_asset_cover(prepared).unwrap();
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
        let cover = lib.root().join("metadata/cover.png");
        let bytes = fs::read(&cover).unwrap();
        assert_eq!(lib.cover().unwrap().unwrap().width, 960);
        let root = lib.root().to_owned();
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        assert!(settings::card_cover(&root).unwrap().is_some());
        let prepared = lib
            .asset_cover_plan(asset.id, asset.version_id)
            .unwrap()
            .render()
            .unwrap();
        let other = Library::create(dir.path(), "另一个库").unwrap();
        assert!(other.commit_asset_cover(prepared).is_err());
        assert!(other.cover().unwrap().is_none());
        let prepared = lib
            .asset_cover_plan(asset.id, asset.version_id)
            .unwrap()
            .render()
            .unwrap();
        lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
        assert!(lib.commit_asset_cover(prepared).is_err());
        assert!(lib.asset_cover_plan(asset.id, asset.version_id).is_err());
        assert_eq!(fs::read(&cover).unwrap(), bytes);
        assert_eq!(fs::read(&original).unwrap(), original_bytes);
        assert!(
            fs::read_dir(lib.root().join("metadata"))
                .unwrap()
                .all(|f| !f.unwrap().file_name().to_string_lossy().starts_with(".tmp"))
        );
    }
}
