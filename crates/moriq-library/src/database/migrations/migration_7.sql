-- 登记项目文件存储位置及导入来源的相对路径。
ALTER TABLE import_context ADD COLUMN relative_path TEXT;
CREATE TABLE project_storage(project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,folder TEXT NOT NULL UNIQUE);
CREATE TABLE project_files(project_id TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,version_id TEXT NOT NULL REFERENCES asset_versions(id),path TEXT NOT NULL UNIQUE,PRIMARY KEY(project_id,path));
PRAGMA user_version=7;
