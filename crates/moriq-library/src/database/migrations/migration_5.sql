CREATE TABLE synco_tasks(
 id TEXT PRIMARY KEY,asset_id TEXT NOT NULL REFERENCES assets(id),sent_version_id TEXT NOT NULL REFERENCES asset_versions(id),
 request TEXT NOT NULL,state TEXT NOT NULL,error TEXT NOT NULL DEFAULT '',result_id TEXT,
 returned_version_id TEXT REFERENCES asset_versions(id),ack TEXT,created_at INTEGER NOT NULL,
 UNIQUE(id,result_id)
);
CREATE INDEX synco_state ON synco_tasks(state,created_at);
PRAGMA user_version=5;
