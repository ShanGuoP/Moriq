CREATE TABLE virtual_folders(id TEXT PRIMARY KEY,name TEXT NOT NULL,kind INTEGER NOT NULL CHECK(kind IN(0,1)),cover_id TEXT REFERENCES assets(id) ON DELETE SET NULL,created_at INTEGER NOT NULL);
CREATE TABLE virtual_folder_assets(folder_id TEXT NOT NULL REFERENCES virtual_folders(id) ON DELETE CASCADE,asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,PRIMARY KEY(folder_id,asset_id));
CREATE INDEX virtual_folder_assets_asset ON virtual_folder_assets(asset_id);
PRAGMA user_version=16;
