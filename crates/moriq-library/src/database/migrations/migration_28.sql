-- Explicitly approved SMB roots. Network originals remain external references.
CREATE TABLE IF NOT EXISTS nas_sources(
 id TEXT PRIMARY KEY NOT NULL,
 platform TEXT NOT NULL,
 native_path BLOB NOT NULL,
 label TEXT NOT NULL
);
CREATE TABLE IF NOT EXISTS nas_versions(
 version_id TEXT PRIMARY KEY NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,
 source_id TEXT NOT NULL REFERENCES nas_sources(id)
);
CREATE INDEX IF NOT EXISTS nas_versions_source ON nas_versions(source_id);
PRAGMA user_version=28;
