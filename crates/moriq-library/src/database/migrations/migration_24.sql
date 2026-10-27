ALTER TABLE ai_vocabulary ADD COLUMN definition TEXT NOT NULL DEFAULT '';
ALTER TABLE ai_vocabulary ADD COLUMN position INTEGER NOT NULL DEFAULT 0;
ALTER TABLE ai_vocabulary ADD COLUMN enabled INTEGER NOT NULL DEFAULT 1 CHECK(enabled IN(0,1));
UPDATE ai_vocabulary SET position=rowid,definition=label;
CREATE TABLE ai_profile_state(id INTEGER PRIMARY KEY CHECK(id=1),revision INTEGER NOT NULL,max_themes INTEGER NOT NULL CHECK(max_themes BETWEEN 1 AND 8));
INSERT INTO ai_profile_state VALUES(1,1,3);
DROP VIEW asset_ai_categories; DROP VIEW ai_search_source; DROP VIEW ai_visible_tags; DROP VIEW ai_effective_content;
CREATE VIEW ai_effective_content AS
 SELECT v.id version_id,coalesce(o.medium,json_extract(c.data,'$.medium'),'') medium,
 (SELECT json_group_array(j.value) FROM json_each(coalesce(o.themes,json_extract(c.data,'$.themes'),'[]')) j JOIN ai_vocabulary t ON t.id=j.value WHERE t.kind='theme' AND t.enabled=1) themes,
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
CREATE VIEW asset_ai_categories AS
 SELECT a.id asset_id,v.label category FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id JOIN ai_vocabulary v ON v.id=e.medium
 UNION SELECT a.id,v.label FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id,json_each(e.themes) j JOIN ai_vocabulary v ON v.id=j.value AND v.enabled=1
 UNION SELECT a.id,c.category FROM assets a JOIN asset_classification c ON c.asset_id=a.id
 WHERE c.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium<>'');
CREATE VIEW ai_search_source AS
 SELECT e.version_id,e.description||' '||coalesce((SELECT label FROM ai_vocabulary WHERE id=e.medium),'')||' '||
 coalesce((SELECT group_concat(v.label,' ') FROM json_each(e.themes) j JOIN ai_vocabulary v ON v.id=j.value AND v.enabled=1),'')||' '||
 coalesce((SELECT group_concat(name,' ') FROM ai_visible_tags t WHERE t.version_id=e.version_id),'')||' '||
 coalesce((SELECT json_extract(data,'$.ocr') FROM ai_results WHERE version_id=e.version_id AND model_id='florence' ORDER BY created_at DESC,rowid DESC LIMIT 1),'') body
 FROM ai_effective_content e;
PRAGMA user_version=24;
