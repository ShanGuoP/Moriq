-- 登记平台原生外部路径及资源库偏好。
CREATE TABLE file_locations(version_id TEXT PRIMARY KEY REFERENCES asset_versions(id),platform TEXT NOT NULL,native_path BLOB NOT NULL);
CREATE TABLE library_preferences(key TEXT PRIMARY KEY,value TEXT NOT NULL);
PRAGMA user_version=3;
