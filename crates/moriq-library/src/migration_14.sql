CREATE TABLE background_jobs(
 id TEXT PRIMARY KEY,kind TEXT NOT NULL CHECK(kind IN('fingerprints','thumbnails')),
 state TEXT NOT NULL CHECK(state IN('queued','running','paused','cancelled','completed')),
 priority INTEGER NOT NULL DEFAULT 0,checkpoint INTEGER NOT NULL DEFAULT 0,total INTEGER NOT NULL DEFAULT 0,
 failed INTEGER NOT NULL DEFAULT 0,error TEXT NOT NULL DEFAULT '',inflight INTEGER NOT NULL DEFAULT 0,
 generation INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL
);
CREATE INDEX background_jobs_schedule ON background_jobs(state,inflight,priority DESC,created_at);
CREATE TABLE background_job_items(job_id TEXT NOT NULL REFERENCES background_jobs(id) ON DELETE CASCADE,seq INTEGER NOT NULL,version_id TEXT NOT NULL,PRIMARY KEY(job_id,seq));
CREATE TABLE image_fingerprints(version_id TEXT PRIMARY KEY REFERENCES asset_versions(id) ON DELETE CASCADE,hash INTEGER NOT NULL,red INTEGER NOT NULL,green INTEGER NOT NULL,blue INTEGER NOT NULL,aspect REAL NOT NULL,algorithm INTEGER NOT NULL DEFAULT 1);
PRAGMA user_version=14;
