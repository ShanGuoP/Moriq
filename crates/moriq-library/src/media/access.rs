use super::*;

impl Library {
    /// 读取指定版本的有界缩略图；缓存不存在时返回空值。
    pub fn thumbnail(&self, version: AssetVersionId) -> Result<Option<Pixels>> {
        let path = safe_path(&self.root, &format!("thumbnails/256/{version}.png"))?;
        if !path.exists() {
            return Ok(None);
        }
        let mut reader = ImageReader::open(path)?.with_guessed_format()?;
        let mut limits = image::Limits::default();
        limits.max_image_width = Some(256);
        limits.max_image_height = Some(256);
        limits.max_alloc = Some(1024 * 1024);
        reader.limits(limits);
        Ok(Some(pixels(reader.decode()?)))
    }

    /// 读取素材当前展示版本的预览像素，复用统一媒体来源流程。
    pub fn preview(&self, id: AssetId) -> Result<Pixels> {
        self.media_source(id)?.preview().map(|p| p.image)
    }
}
