use super::*;
impl App {
    /// 按当前界面上传预算把已读取像素转为图像句柄，避免单帧大量上传。
    pub(super) fn drain_uploads(&mut self) {
        if self.generation != self.upload_generation
            || self.library.as_ref().map(|l| l.info.id) != self.upload_owner
        {
            self.uploads.clear();
            return;
        }
        let mut bytes = 0;
        for _ in 0..8 {
            let Some((version, pixels)) = self.uploads.pop_front() else {
                break;
            };
            bytes += pixels.rgba.len();
            if let Some(tile) = self
                .tiles
                .iter_mut()
                .find(|t| t.asset.version_id == version)
            {
                tile.handle = Some(image::Handle::from_rgba(
                    pixels.width,
                    pixels.height,
                    pixels.rgba,
                ));
            }
            if bytes >= 2 * 1024 * 1024 {
                break;
            }
        }
    }
}
