-- Keep immutable task receipts separate from the effective per-version pointers.
CREATE TABLE ai_analysis_runs(
 id INTEGER PRIMARY KEY,version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,
 task TEXT NOT NULL CHECK(task IN('qwen_classification','qwen_description')),
 revision TEXT NOT NULL,preprocessing TEXT NOT NULL,data TEXT NOT NULL CHECK(json_valid(data)),created_at INTEGER NOT NULL,UNIQUE(version_id,task,id)
);
CREATE INDEX ai_runs_version ON ai_analysis_runs(version_id,task,id);
CREATE TABLE ai_effective_results(
 version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,
 task TEXT NOT NULL,run_id INTEGER NOT NULL REFERENCES ai_analysis_runs(id) ON DELETE CASCADE,
 PRIMARY KEY(version_id,task),FOREIGN KEY(version_id,task,run_id) REFERENCES ai_analysis_runs(version_id,task,id) ON DELETE CASCADE
);
CREATE TABLE ai_overrides(
 version_id TEXT PRIMARY KEY REFERENCES asset_versions(id) ON DELETE CASCADE,
 medium TEXT,themes TEXT CHECK(themes IS NULL OR (json_valid(themes) AND json_type(themes)='array')),description TEXT
);
CREATE TABLE ai_rejected_tags(
 version_id TEXT NOT NULL REFERENCES asset_versions(id) ON DELETE CASCADE,name TEXT NOT NULL,PRIMARY KEY(version_id,name)
);
CREATE TABLE ai_vocabulary(kind TEXT NOT NULL,id TEXT PRIMARY KEY,label TEXT NOT NULL UNIQUE);
INSERT INTO ai_vocabulary VALUES
 ('medium','photography','摄影'),('medium','illustration','插画'),('medium','3d_render','三维渲染'),
 ('medium','screenshot','界面截图'),('medium','uncertain','不确定'),
 ('theme','portrait','人像'),('theme','landscape','风景'),('theme','architecture','建筑'),('theme','animal','动物'),
 ('theme','plant','植物'),('theme','food','美食'),('theme','cosplay','角色扮演'),('theme','interface_document','界面与文档');
-- Old Qwen receipts used display labels. Only migrate actual Qwen fields, never legacy categories.
INSERT INTO ai_analysis_runs(version_id,task,revision,preprocessing,data,created_at)
 SELECT version_id,'qwen_classification',revision,preprocessing,
 json_set(data,'$.medium',coalesce((SELECT id FROM ai_vocabulary WHERE kind='medium' AND (id=json_extract(r.data,'$.medium') OR label=json_extract(r.data,'$.medium'))),'uncertain'),
 '$.themes',json((SELECT json_group_array(id) FROM ai_vocabulary WHERE kind='theme' AND (id IN(SELECT value FROM json_each(r.data,'$.themes')) OR label IN(SELECT value FROM json_each(r.data,'$.themes')))))),created_at
 FROM ai_results r WHERE model_id='qwen35-4b' AND json_type(data,'$.medium')='text' ORDER BY created_at,rowid;
INSERT INTO ai_analysis_runs(version_id,task,revision,preprocessing,data,created_at)
 SELECT version_id,'qwen_description',revision,preprocessing,data,created_at FROM ai_results
 WHERE model_id='qwen35-4b' AND (json_type(data,'$.description')='text' OR json_type(data,'$.tags')='array') ORDER BY created_at,rowid;
INSERT INTO ai_effective_results SELECT version_id,task,max(id) FROM ai_analysis_runs GROUP BY version_id,task;
CREATE VIEW ai_effective_content AS
 SELECT v.id version_id,coalesce(o.medium,json_extract(c.data,'$.medium'),'') medium,
 coalesce(o.themes,json_extract(c.data,'$.themes'),'[]') themes,
 coalesce(o.description,json_extract(d.data,'$.description'),
 (SELECT json_extract(data,'$.caption') FROM ai_results WHERE version_id=v.id AND model_id='florence' ORDER BY created_at DESC,rowid DESC LIMIT 1),'') description,
 coalesce(json_extract(d.data,'$.tags'),'[]') tags,
 o.medium IS NOT NULL manual_medium,o.themes IS NOT NULL manual_themes,o.description IS NOT NULL manual_description
 FROM asset_versions v LEFT JOIN ai_overrides o ON o.version_id=v.id
 LEFT JOIN ai_effective_results cp ON cp.version_id=v.id AND cp.task='qwen_classification'
 LEFT JOIN ai_analysis_runs c ON c.id=cp.run_id
 LEFT JOIN ai_effective_results dp ON dp.version_id=v.id AND dp.task='qwen_description'
 LEFT JOIN ai_analysis_runs d ON d.id=dp.run_id;
CREATE VIEW ai_visible_tags AS
 SELECT DISTINCT e.version_id,trim(j.value) name FROM ai_effective_content e,json_each(e.tags) j
 WHERE j.type='text' AND trim(j.value)<>'' AND NOT EXISTS(SELECT 1 FROM ai_rejected_tags r WHERE r.version_id=e.version_id AND r.name=trim(j.value));
CREATE VIEW effective_asset_tags AS
 SELECT at.asset_id,t.name FROM asset_tags at JOIN tags t ON t.id=at.tag_id
 UNION SELECT a.id,t.name FROM assets a JOIN ai_visible_tags t ON t.version_id=a.display_version_id;
CREATE VIEW asset_ai_categories AS
 SELECT a.id asset_id,v.label category FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id JOIN ai_vocabulary v ON v.id=e.medium
 UNION SELECT a.id,v.label FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id,json_each(e.themes) j JOIN ai_vocabulary v ON v.id=j.value
 UNION SELECT a.id,c.category FROM assets a JOIN asset_classification c ON c.asset_id=a.id
 WHERE c.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium<>'');
CREATE VIEW ai_search_source AS
 SELECT e.version_id,e.description||' '||coalesce((SELECT label FROM ai_vocabulary WHERE id=e.medium),'')||' '||
 coalesce((SELECT group_concat(v.label,' ') FROM json_each(e.themes) j JOIN ai_vocabulary v ON v.id=j.value),'')||' '||
 coalesce((SELECT group_concat(name,' ') FROM ai_visible_tags t WHERE t.version_id=e.version_id),'')||' '||
 coalesce((SELECT json_extract(data,'$.ocr') FROM ai_results WHERE version_id=e.version_id AND model_id='florence' ORDER BY created_at DESC,rowid DESC LIMIT 1),'') body
 FROM ai_effective_content e;
DROP TRIGGER IF EXISTS ai_search_insert;
DROP TRIGGER IF EXISTS ai_search_update;
DROP TRIGGER IF EXISTS ai_search_delete;
DELETE FROM ai_text_fts;
INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE trim(body)<>'';
CREATE TRIGGER content_ai_results_insert AFTER INSERT ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_results_update AFTER UPDATE ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_results_delete AFTER DELETE ON ai_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_effective_results_insert AFTER INSERT ON ai_effective_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_effective_results_update AFTER UPDATE ON ai_effective_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_effective_results_delete AFTER DELETE ON ai_effective_results BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_overrides_insert AFTER INSERT ON ai_overrides BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_overrides_update AFTER UPDATE ON ai_overrides BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_overrides_delete AFTER DELETE ON ai_overrides BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_rejected_tags_insert AFTER INSERT ON ai_rejected_tags BEGIN
 DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_rejected_tags_update AFTER UPDATE ON ai_rejected_tags BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
DELETE FROM ai_text_fts WHERE version_id=new.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=new.version_id AND trim(body)<>'';
END;
CREATE TRIGGER content_ai_rejected_tags_delete AFTER DELETE ON ai_rejected_tags BEGIN
 DELETE FROM ai_text_fts WHERE version_id=old.version_id;
 INSERT INTO ai_text_fts SELECT version_id,body FROM ai_search_source WHERE version_id=old.version_id AND trim(body)<>'';
END;
CREATE TRIGGER ai_document_delete AFTER DELETE ON asset_versions BEGIN DELETE FROM ai_text_fts WHERE version_id=old.id; END;
PRAGMA user_version=23;
