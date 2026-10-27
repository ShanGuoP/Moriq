PRAGMA foreign_keys = ON;
CREATE TABLE library_identity(id TEXT PRIMARY KEY NOT NULL);
CREATE TABLE assets(
 id TEXT PRIMARY KEY NOT NULL,
 display_version_id TEXT NOT NULL UNIQUE,
 name TEXT NOT NULL,
 favorite INTEGER NOT NULL DEFAULT 0 CHECK(favorite IN (0,1)),
 rating INTEGER NOT NULL DEFAULT 0 CHECK(rating BETWEEN 0 AND 5),
 imported_at INTEGER NOT NULL,
 FOREIGN KEY(display_version_id,id) REFERENCES asset_versions(id,asset_id) DEFERRABLE INITIALLY DEFERRED
);
CREATE TABLE asset_versions(
 id TEXT PRIMARY KEY NOT NULL,
 asset_id TEXT NOT NULL REFERENCES assets(id) DEFERRABLE INITIALLY DEFERRED,
 parent_version_id TEXT REFERENCES asset_versions(id),
 origin TEXT NOT NULL CHECK(origin IN ('original','synco','import','export')),
 path TEXT NOT NULL UNIQUE,
 sha256 TEXT NOT NULL CHECK(length(sha256)=64),
 format TEXT NOT NULL,
 width INTEGER NOT NULL CHECK(width>0),
 height INTEGER NOT NULL CHECK(height>0),
 bytes INTEGER NOT NULL CHECK(bytes>0),
 UNIQUE(id,asset_id)
);
CREATE INDEX versions_hash ON asset_versions(sha256);
CREATE INDEX versions_asset ON asset_versions(asset_id);
CREATE INDEX versions_parent ON asset_versions(parent_version_id);
CREATE INDEX assets_order ON assets(imported_at DESC,id DESC);
CREATE INDEX assets_favorites ON assets(favorite,imported_at DESC,id DESC);
-- Dense append-only rank supports arbitrary jumps without deep OFFSET scans.
CREATE TABLE asset_order(position INTEGER PRIMARY KEY CHECK(position>=0),asset_id TEXT NOT NULL UNIQUE REFERENCES assets(id));
CREATE TRIGGER assets_append_order AFTER INSERT ON assets BEGIN
 INSERT INTO asset_order(position,asset_id) VALUES((SELECT COALESCE(max(position),-1)+1 FROM asset_order),NEW.id);
END;
CREATE TABLE projects(id TEXT PRIMARY KEY NOT NULL,name TEXT NOT NULL UNIQUE);
CREATE TABLE project_assets(project_id TEXT NOT NULL REFERENCES projects(id),asset_id TEXT NOT NULL REFERENCES assets(id),PRIMARY KEY(project_id,asset_id));
CREATE TABLE tags(id INTEGER PRIMARY KEY,name TEXT NOT NULL UNIQUE);
CREATE TABLE asset_tags(asset_id TEXT NOT NULL REFERENCES assets(id),tag_id INTEGER NOT NULL REFERENCES tags(id),PRIMARY KEY(asset_id,tag_id));
CREATE INDEX asset_tags_reverse ON asset_tags(tag_id,asset_id);
CREATE TABLE operation_journal(id TEXT PRIMARY KEY NOT NULL,path TEXT NOT NULL,phase TEXT NOT NULL CHECK(phase IN ('prepared','committed','interrupted')));
PRAGMA user_version = 1;
