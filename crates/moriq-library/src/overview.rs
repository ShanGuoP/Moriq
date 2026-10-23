//! Bounded homepage projections. These queries never touch the gallery snapshot.
use super::*;

pub struct Assets {
    pub favorites: Vec<Asset>,
    pub categories: Vec<(String, Option<AssetVersionId>)>,
}

impl Library {
    pub fn overview_assets(&self) -> Result<Assets> {
        let favorites = self.read_assets(
            "SELECT a.id,v.id,a.name,v.format,v.width,v.height,v.bytes,a.favorite,a.rating,a.imported_at \
             FROM assets a JOIN asset_versions v ON v.id=a.display_version_id \
             WHERE a.deleted_at IS NULL AND a.favorite=1 \
             ORDER BY a.imported_at DESC,a.id DESC LIMIT 4",
            [],
        )?;
        let mut statement = self.db.prepare(
            "SELECT a.display_version_id FROM assets a WHERE a.deleted_at IS NULL AND \
             (EXISTS(SELECT 1 FROM asset_classification c WHERE c.asset_id=a.id AND c.category=?1) \
             OR (?1='未分类' AND NOT EXISTS(SELECT 1 FROM asset_classification c WHERE c.asset_id=a.id))) \
             ORDER BY a.imported_at DESC,a.id DESC LIMIT 1",
        )?;
        let categories = ["人像", "风景", "建筑", "插画", "截图", "未分类"]
            .into_iter()
            .map(|category| {
                let version: Option<String> = statement
                    .query_row([category], |row| row.get(0))
                    .optional()?;
                Ok((
                    category.to_owned(),
                    version
                        .map(|id| parse_id(&id).map(AssetVersionId))
                        .transpose()?,
                ))
            })
            .collect::<Result<_>>()?;
        Ok(Assets {
            favorites,
            categories,
        })
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn homepage_respects_current_versions_and_trash_without_replacing_gallery_snapshot() {
        let dir = tempfile::tempdir().unwrap();
        let mut lib = Library::create(dir.path(), "首页查询").unwrap();
        for n in 0..8u8 {
            let path = dir.path().join(format!("shot-{n}.png"));
            image::RgbImage::from_pixel(2, 2, image::Rgb([n, 20, 30]))
                .save(&path)
                .unwrap();
            lib.import(&path, None).unwrap();
        }
        let assets = lib.page(&Query::default(), 0, 20).unwrap().assets;
        for (n, asset) in assets.iter().enumerate() {
            lib.set_favorite(asset.id, true).unwrap();
            lib.db
                .execute(
                    "UPDATE assets SET imported_at=?1 WHERE id=?2",
                    params![n as i64, asset.id.to_string()],
                )
                .unwrap();
        }
        lib.set_tags(assets[6].id, &["保留标签".into()]).unwrap();
        lib.set_category(&[assets[6].id], "风景").unwrap();
        lib.set_category(&[assets[7].id], "风景").unwrap();
        lib.edit(&[assets[7].id], &Edit::Trash(true)).unwrap();
        let query = Query {
            text: "shot".into(),
            sort: SortOrder::Name,
            ..Default::default()
        };
        let gallery = lib.page(&query, 0, 3).unwrap();
        let snapshot = lib.snapshot.clone();
        lib.db.execute_batch("CREATE TEMP TRIGGER preserve_gallery BEFORE DELETE ON query_snapshot BEGIN SELECT RAISE(ABORT,'homepage replaced gallery'); END;").unwrap();
        let overview = lib.overview_assets().unwrap();
        assert_eq!(lib.snapshot, snapshot);
        assert_eq!(overview.favorites.len(), 4);
        assert_eq!(overview.favorites[0].id, assets[6].id);
        assert_eq!(overview.favorites[0].tags, vec!["保留标签"]);
        assert_eq!(overview.categories[1].1, Some(assets[6].version_id));
        assert!(overview.categories[0].1.is_none());
        assert_eq!(overview.categories[5].1, Some(assets[5].version_id));
        let page = lib.page(&query, 0, 3).unwrap();
        assert_eq!(page.total, gallery.total);
        assert_eq!(
            page.assets.iter().map(|a| a.id).collect::<Vec<_>>(),
            gallery.assets.iter().map(|a| a.id).collect::<Vec<_>>()
        );
    }
}
