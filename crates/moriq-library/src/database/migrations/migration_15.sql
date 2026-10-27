ALTER TABLE background_job_items RENAME TO background_job_items_old;
ALTER TABLE background_jobs RENAME TO background_jobs_old;
CREATE TABLE background_jobs(id TEXT PRIMARY KEY,kind TEXT NOT NULL,state TEXT NOT NULL CHECK(state IN('queued','running','paused','cancelled','completed')),priority INTEGER NOT NULL DEFAULT 0,checkpoint INTEGER NOT NULL DEFAULT 0,total INTEGER NOT NULL DEFAULT 0,failed INTEGER NOT NULL DEFAULT 0,error TEXT NOT NULL DEFAULT '',inflight INTEGER NOT NULL DEFAULT 0,generation INTEGER NOT NULL DEFAULT 0,created_at INTEGER NOT NULL);
INSERT INTO background_jobs SELECT * FROM background_jobs_old;
CREATE TABLE background_job_items(job_id TEXT NOT NULL REFERENCES background_jobs(id) ON DELETE CASCADE,seq INTEGER NOT NULL,version_id TEXT NOT NULL,PRIMARY KEY(job_id,seq));
INSERT INTO background_job_items SELECT * FROM background_job_items_old;
DROP TABLE background_job_items_old;
DROP TABLE background_jobs_old;
CREATE INDEX background_jobs_schedule ON background_jobs(state,inflight,priority DESC,created_at);
CREATE TABLE ai_results(version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,model_id TEXT NOT NULL,revision TEXT NOT NULL,preprocessing TEXT NOT NULL,data TEXT NOT NULL,created_at INTEGER NOT NULL,PRIMARY KEY(version_id,model_id,revision,preprocessing));
CREATE TABLE ai_embeddings(version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,model_id TEXT NOT NULL,revision TEXT NOT NULL,vector BLOB NOT NULL,PRIMARY KEY(version_id,model_id,revision));
CREATE TABLE asset_classification(asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,category TEXT NOT NULL,manual INTEGER NOT NULL DEFAULT 0);
CREATE TABLE people(id TEXT PRIMARY KEY,name TEXT NOT NULL,revision TEXT NOT NULL,created_at INTEGER NOT NULL);
CREATE TABLE detected_faces(id TEXT PRIMARY KEY,version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,revision TEXT NOT NULL,ordinal INTEGER NOT NULL,box TEXT NOT NULL,vector BLOB NOT NULL,person_id TEXT REFERENCES people(id) ON DELETE SET NULL,manual INTEGER NOT NULL DEFAULT 0,UNIQUE(version_id,revision,ordinal));
CREATE INDEX detected_faces_person ON detected_faces(person_id);
CREATE VIRTUAL TABLE ai_text_fts USING fts5(version_id UNINDEXED,body,tokenize='trigram');
CREATE TRIGGER ai_search_insert AFTER INSERT ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,group_concat(coalesce(json_extract(data,'$.caption'),'')||' '||coalesce(json_extract(data,'$.ocr'),'')||' '||coalesce(json_extract(data,'$.objects.labels'),'')||' '||coalesce(json_extract(data,'$.labels'),''),' ') FROM ai_results WHERE version_id=new.version_id GROUP BY version_id;
END;
CREATE TRIGGER ai_search_update AFTER UPDATE ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,group_concat(coalesce(json_extract(data,'$.caption'),'')||' '||coalesce(json_extract(data,'$.ocr'),'')||' '||coalesce(json_extract(data,'$.objects.labels'),'')||' '||coalesce(json_extract(data,'$.labels'),''),' ') FROM ai_results WHERE version_id=new.version_id GROUP BY version_id;
END;
CREATE TRIGGER ai_search_delete AFTER DELETE ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,group_concat(coalesce(json_extract(data,'$.caption'),'')||' '||coalesce(json_extract(data,'$.ocr'),'')||' '||coalesce(json_extract(data,'$.objects.labels'),'')||' '||coalesce(json_extract(data,'$.labels'),''),' ') FROM ai_results WHERE version_id=old.version_id GROUP BY version_id;
END;
PRAGMA user_version=15;
