use super::*;
impl Library {
    /// 读取素材当前展示版本的人工地点，缺少记录时返回空值。
    pub(super) fn manual_location(&self, asset: AssetId) -> Result<Option<AssetLocation>> {
        Ok(self
            .db
            .query_row(
                "SELECT l.latitude,l.longitude,l.label FROM version_locations l JOIN assets a ON \
                a.display_version_id=l.version_id WHERE a.id=?1",
                [asset.to_string()],
                |r| {
                    Ok(AssetLocation {
                        latitude: r.get(0)?,
                        longitude: r.get(1)?,
                        label: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }
    /// 校验经纬度和文本，保存或清除当前版本地点并刷新相关查询。
    pub fn set_location(
        &mut self,
        asset: AssetId,
        version: AssetVersionId,
        value: Option<&AssetLocation>,
    ) -> Result<()> {
        if value.is_some_and(|v| !v.valid()) {
            return Err(Error::Invalid(
                "请输入有效经纬度：纬度 -90～90，经度 -180～180；地点名称最多 1 KiB".into(),
            ));
        }
        let tx = self.db.transaction()?;
        let matches = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM assets WHERE id=?1 AND display_version_id=?2)",
            params![asset.to_string(), version.to_string()],
            |r| r.get::<_, bool>(0),
        )?;
        if !matches {
            return Err(Error::Invalid(
                "素材或展示版本已经改变，未保存地点，请重新打开编辑".into(),
            ));
        }
        if let Some(value) = value {
            tx.execute("INSERT INTO version_locations VALUES(?1,?2,?3,?4) ON CONFLICT(version_id) DO UPDATE SET \
                latitude=excluded.latitude,longitude=excluded.longitude,label=excluded.label",
params![version.to_string(),
value.latitude,
value.longitude,
value.label])?;
        } else {
            tx.execute(
                "DELETE FROM version_locations WHERE version_id=?1",
                [version.to_string()],
            )?;
        }
        tx.commit()?;
        self.snapshot = None;
        Ok(())
    }
}
/// 从 EXIF GPS 有理数及方向字段解析经纬度，无需网络定位。
pub(super) fn exif_location(data: &exif::Exif) -> Option<AssetLocation> {
    let coordinate = |tag, reference, negative: u8, limit: f64| -> Option<f64> {
        let exif::Value::Rational(values) = &data.get_field(tag, exif::In::PRIMARY)?.value else {
            return None;
        };
        if values.len() != 3 || values.iter().any(|v| v.denom == 0) {
            return None;
        }
        let nums = values
            .iter()
            .map(|v| v.num as f64 / v.denom as f64)
            .collect::<Vec<_>>();
        if nums[1] >= 60.0 || nums[2] >= 60.0 {
            return None;
        }
        let exif::Value::Ascii(refs) = &data.get_field(reference, exif::In::PRIMARY)?.value else {
            return None;
        };
        let direction = *refs.first()?.first()?;
        let positive = if negative == b'S' { b'N' } else { b'E' };
        if direction != negative && direction != positive {
            return None;
        }
        let result = (nums[0] + nums[1] / 60.0 + nums[2] / 3600.0)
            * if direction == negative { -1.0 } else { 1.0 };
        (result.is_finite() && result.abs() <= limit).then_some(result)
    };
    Some(AssetLocation {
        latitude: coordinate(
            exif::Tag::GPSLatitude,
            exif::Tag::GPSLatitudeRef,
            b'S',
            90.0,
        )?,
        longitude: coordinate(
            exif::Tag::GPSLongitude,
            exif::Tag::GPSLongitudeRef,
            b'W',
            180.0,
        )?,
        label: String::new(),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    #[test]
    /// 验证人工地点按版本保存、可撤销并校验输入，且不修改原片。
    fn manual_location_is_version_scoped_reversible_validated_and_preserves_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("原图.png");
        image::RgbImage::from_pixel(3, 2, image::Rgb([1, 2, 3]))
            .save(&source)
            .unwrap();
        let bytes = fs::read(&source).unwrap();
        let mut lib = Library::create(dir.path(), "地点").unwrap();
        lib.import(&source, None).unwrap();
        let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
        let location = AssetLocation {
            latitude: 31.5,
            longitude: 121.25,
            label: "拍摄地".into(),
        };
        lib.set_location(asset.id, asset.version_id, Some(&location))
            .unwrap();
        assert!(
            lib.set_location(
                asset.id,
                asset.version_id,
                Some(&AssetLocation {
                    latitude: f64::NAN,
                    ..location.clone()
                })
            )
            .is_err()
        );
        let version = AssetVersionId::default();
        fs::copy(&source, lib.root.join("versions/new.png")).unwrap();
        lib.db.execute("INSERT INTO \
                asset_versions(id,asset_id,parent_version_id,origin,path,sha256,format,width,height,bytes) SELECT \
                ?1,asset_id,id,'synco','versions/new.png',sha256,format,width,height,bytes FROM asset_versions \
                WHERE id=?2",
params![version.to_string(),
asset.version_id.to_string()]).unwrap();

        lib.display_version(asset.id, version).unwrap();
        assert!(lib.details(asset.id).unwrap().manual_location.is_none());
        assert!(lib.set_location(asset.id, asset.version_id, None).is_err());
        lib.display_version(asset.id, asset.version_id).unwrap();
        assert_eq!(
            lib.details(asset.id).unwrap().manual_location,
            Some(location.clone())
        );
        let root = lib.root.clone();
        drop(lib);
        let mut lib = Library::open(&root).unwrap();
        assert_eq!(
            lib.details(asset.id).unwrap().manual_location,
            Some(location)
        );
        lib.set_location(asset.id, asset.version_id, None).unwrap();
        assert!(lib.details(asset.id).unwrap().manual_location.is_none());
        assert_eq!(fs::read(source).unwrap(), bytes);
        lib.edit(&[asset.id], &Edit::Trash(true)).unwrap();
        lib.purge(&[asset.id]).unwrap();
        assert_eq!(
            lib.db
                .query_row("SELECT count(*) FROM version_locations", [], |r| r
                    .get::<_, u64>(0))
                .unwrap(),
            0
        );
    }
    #[test]
    /// 验证 GPS 有理数和半球方向解析正确，无需网络访问。
    fn gps_rationals_are_decoded_with_hemispheres_without_network_access() {
        let mut tiff = b"II\x2a\0\x08\0\0\0".to_vec();
        tiff.extend_from_slice(&1u16.to_le_bytes());
        {
            let (tag, kind, count, value) = (0x8825u16, 4u16, 1u32, 26u32);
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&kind.to_le_bytes());
            tiff.extend_from_slice(&count.to_le_bytes());
            tiff.extend_from_slice(&value.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        tiff.extend_from_slice(&4u16.to_le_bytes());
        for (tag, kind, count, value) in [
            (1u16, 2u16, 2u32, b'N' as u32),
            (2, 5, 3, 80),
            (3, 2, 2, b'W' as u32),
            (4, 5, 3, 104),
        ] {
            tiff.extend_from_slice(&tag.to_le_bytes());
            tiff.extend_from_slice(&kind.to_le_bytes());
            tiff.extend_from_slice(&count.to_le_bytes());
            tiff.extend_from_slice(&value.to_le_bytes());
        }
        tiff.extend_from_slice(&0u32.to_le_bytes());
        for num in [31u32, 30, 0, 121, 0, 0] {
            tiff.extend_from_slice(&num.to_le_bytes());
            tiff.extend_from_slice(&1u32.to_le_bytes());
        }
        let data = exif::Reader::new().read_raw(tiff).unwrap();
        let location = exif_location(&data).unwrap();
        assert_eq!((location.latitude, location.longitude), (31.5, -121.0));
        assert!(
            location
                .map_url()
                .unwrap()
                .starts_with("https://www.openstreetmap.org/")
        );
        assert!(
            AssetLocation {
                latitude: f64::INFINITY,
                ..location
            }
            .map_url()
            .is_none()
        );
    }
}
