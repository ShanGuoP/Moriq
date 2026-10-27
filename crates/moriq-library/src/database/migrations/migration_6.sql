-- 为素材版本登记主色并建立颜色筛选索引。
ALTER TABLE asset_versions ADD COLUMN dominant_color TEXT NOT NULL DEFAULT '';
CREATE INDEX versions_color ON asset_versions(dominant_color);
PRAGMA user_version=6;
