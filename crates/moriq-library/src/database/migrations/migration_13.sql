-- 为素材版本保存视频探测元数据。
CREATE TABLE video_metadata(version_id TEXT PRIMARY KEY REFERENCES asset_versions(id) ON DELETE CASCADE,json TEXT NOT NULL);
PRAGMA user_version=13;
