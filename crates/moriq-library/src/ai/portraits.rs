//! Representative faces always refer to a live asset's displayed version.
use super::*;

pub struct PortraitSource {
    pub asset: AssetId,
    pub version: AssetVersionId,
    pub bounds: [f32; 4],
}

impl Library {
    /// 为人物选择有效代表人脸，并冻结其素材版本与裁剪范围。
    pub fn person_portrait_source(&self, person: &str) -> Result<Option<PortraitSource>> {
        let row: Option<(String, String, String)> = self.db.query_row(
            "SELECT a.id,f.version_id,f.box FROM detected_faces f \
             JOIN assets a ON a.display_version_id=f.version_id \
             WHERE f.person_id=?1 AND a.deleted_at IS NULL \
             ORDER BY json_extract(f.box,'$[2]')*json_extract(f.box,'$[3]') DESC,a.imported_at DESC,f.id LIMIT 1",
            [person], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?)),
        ).optional()?;
        row.map(|(asset, version, bounds)| {
            Ok(PortraitSource {
                asset: AssetId(parse_id(&asset)?),
                version: AssetVersionId(parse_id(&version)?),
                bounds: serde_json::from_str(&bounds).map_err(|e| Error::Invalid(e.to_string()))?,
            })
        })
        .transpose()
    }
}

/// Crop the actual detected face, with a little context, from a bounded preview.
/// Copy only the crop rows rather than cloning the entire image for every face.
///
/// 按已准备的人脸范围裁出头像像素。
pub fn crop(preview: &Pixels, bounds: [f32; 4]) -> Result<Pixels> {
    crop_at(preview, bounds, 96)
}
/// 校验人脸位置与源像素尺寸后裁剪指定区域，拒绝不完整像素数据。
pub(crate) fn crop_at(preview: &Pixels, bounds: [f32; 4], output: u32) -> Result<Pixels> {
    if preview.width == 0
        || preview.height == 0
        || preview.width > 1600
        || preview.height > 1600
        || bounds
            .iter()
            .any(|n| !n.is_finite() || !(0.0..=1.0).contains(n))
        || bounds[2] <= 0.
        || bounds[3] <= 0.
        || bounds[0] >= 1.
        || bounds[1] >= 1.
    {
        return Err(Error::Invalid("人脸预览位置无效".into()));
    }
    let width = preview.width as f32;
    let height = preview.height as f32;
    let left = bounds[0] * width;
    let top = bounds[1] * height;
    let right = (bounds[0] + bounds[2]).min(1.) * width;
    let bottom = (bounds[1] + bounds[3]).min(1.) * height;
    let edge = (((right - left).max(bottom - top) * 1.22).ceil() as u32)
        .clamp(1, preview.width.min(preview.height));
    let x = (((left + right - edge as f32) / 2.).max(0.) as u32).min(preview.width - edge);
    let y = (((top + bottom - edge as f32) / 2.).max(0.) as u32).min(preview.height - edge);
    let mut pixels = Vec::with_capacity((edge * edge * 4) as usize);
    for row in y..y + edge {
        let start = ((row * preview.width + x) * 4) as usize;
        let bytes = preview
            .rgba
            .get(start..start + (edge * 4) as usize)
            .ok_or_else(|| Error::Invalid("人脸预览像素不完整".into()))?;
        pixels.extend_from_slice(bytes);
    }
    let image = image::RgbaImage::from_raw(edge, edge, pixels)
        .ok_or_else(|| Error::Invalid("人脸预览像素无效".into()))?;
    Ok(super::pixels(image::DynamicImage::ImageRgba8(
        image::imageops::resize(
            &image,
            output,
            output,
            image::imageops::FilterType::Triangle,
        ),
    )))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证头像裁剪取实际人脸区域，并拒绝无效范围或不完整像素。
    fn crops_the_detected_face_and_rejects_invalid_or_incomplete_pixels() {
        let image = image::RgbaImage::from_fn(100, 100, |x, _| {
            image::Rgba(if x < 50 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 255, 255]
            })
        });
        let preview = super::super::pixels(image::DynamicImage::ImageRgba8(image));
        for (bounds, color) in [
            ([0.1, 0.2, 0.2, 0.4], [255, 0, 0, 255]),
            ([0.7, 0.2, 0.2, 0.4], [0, 0, 255, 255]),
        ] {
            let portrait = crop(&preview, bounds).unwrap();
            assert_eq!((portrait.width, portrait.height), (96, 96));
            assert_eq!(
                &portrait.rgba[(48 * 96 + 48) * 4..(48 * 96 + 48) * 4 + 4],
                &color
            );
        }
        assert!(crop(&preview, [f32::NAN, 0., 0.2, 0.2]).is_err());
        assert!(crop(&preview, [0., 0., 0., 0.2]).is_err());
        assert!(
            crop(
                &Pixels {
                    width: 100,
                    height: 100,
                    rgba: Vec::new().into()
                },
                [0., 0., 0.2, 0.2]
            )
            .is_err()
        );
        assert!(crop(&preview, [0.99, 0.99, 0.2, 0.2]).is_ok());
    }
    #[test]
    /// 验证人物代表头像排除旧展示版本和回收站素材。
    fn representative_face_excludes_old_versions_and_trashed_assets() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("sample.png");
        image::RgbImage::from_pixel(20, 20, image::Rgb([10, 20, 30]))
            .save(&file)
            .unwrap();
        let mut lib = Library::create(dir.path(), "人脸预览").unwrap();
        lib.import(&file, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        lib.db
            .execute(
                "INSERT INTO people VALUES('person','测试人物','test',0)",
                [],
            )
            .unwrap();
        lib.db
            .execute(
                "INSERT INTO detected_faces(id,version_id,revision,ordinal,box,vector,person_id) \
                VALUES('face',?1,'test',0,'[0.1,0.2,0.3,0.4]',x'00000000','person')",
                [asset.version_id.to_string()],
            )
            .unwrap();

        let source = lib.person_portrait_source("person").unwrap().unwrap();
        assert_eq!(source.version, asset.version_id);
        assert_eq!(source.bounds, [0.1, 0.2, 0.3, 0.4]);
        let next = AssetVersionId(Uuid::new_v4());
        lib.db.execute("INSERT INTO \
                asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes,dominant_color) SELECT ?1,asset_id,id,'import','versions/another.png',sha256,format,width,height,bytes,dominant_color FROM asset_versions WHERE id=?2",
params![next.to_string(),
asset.version_id.to_string()]).unwrap();

        lib.display_version(asset.id, next).unwrap();
        assert!(lib.person_portrait_source("person").unwrap().is_none());
        lib.display_version(asset.id, asset.version_id).unwrap();
        lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
        assert!(lib.person_portrait_source("person").unwrap().is_none());
    }
}
