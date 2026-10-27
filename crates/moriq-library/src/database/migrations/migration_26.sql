CREATE TABLE IF NOT EXISTS project_ai_profiles(project_id TEXT PRIMARY KEY REFERENCES projects(id) ON DELETE CASCADE,revision INTEGER NOT NULL,max_themes INTEGER NOT NULL,categories TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workspace_people(person_id TEXT PRIMARY KEY REFERENCES people(id) ON DELETE CASCADE,workspace TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workspace_folders(folder_id TEXT PRIMARY KEY REFERENCES virtual_folders(id) ON DELETE CASCADE,workspace TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workspace_jobs(job_id TEXT PRIMARY KEY REFERENCES background_jobs(id) ON DELETE CASCADE,workspace TEXT NOT NULL);
CREATE TABLE IF NOT EXISTS workspace_collections(id TEXT PRIMARY KEY,name TEXT NOT NULL,query TEXT NOT NULL,workspace TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,UNIQUE(workspace,name));
CREATE INDEX IF NOT EXISTS workspace_people_scope ON workspace_people(workspace);
CREATE INDEX IF NOT EXISTS workspace_folders_scope ON workspace_folders(workspace);
CREATE INDEX IF NOT EXISTS workspace_jobs_scope ON workspace_jobs(workspace);
CREATE INDEX IF NOT EXISTS project_assets_asset_project ON project_assets(asset_id,project_id);
CREATE TRIGGER IF NOT EXISTS project_workspace_cleanup AFTER DELETE ON projects BEGIN
 DELETE FROM people WHERE id IN(SELECT person_id FROM workspace_people WHERE workspace=old.id);
 DELETE FROM virtual_folders WHERE id IN(SELECT folder_id FROM workspace_folders WHERE workspace=old.id);
 DELETE FROM background_jobs WHERE id IN(SELECT job_id FROM workspace_jobs WHERE workspace=old.id);
END;
CREATE VIEW IF NOT EXISTS asset_workspaces AS SELECT a.id asset_id,coalesce((SELECT min(project_id) FROM project_assets p WHERE p.asset_id=a.id),'') workspace FROM assets a;
DROP VIEW IF EXISTS workspace_vocabulary;
CREATE VIEW workspace_vocabulary AS
 SELECT '' workspace,id,kind,label,definition,position,enabled FROM ai_vocabulary
 UNION ALL SELECT p.project_id,json_extract(j.value,'$.id'),json_extract(j.value,'$.kind'),json_extract(j.value,'$.label'),json_extract(j.value,'$.definition'),CAST(j.key AS INTEGER),json_extract(j.value,'$.enabled') FROM project_ai_profiles p,json_each(p.categories) j;
DROP VIEW asset_ai_categories; DROP VIEW ai_search_source; DROP VIEW ai_visible_tags; DROP VIEW ai_effective_content;
CREATE VIEW ai_effective_content AS
 SELECT v.id version_id,coalesce(o.medium,json_extract(c.data,'$.medium'),'') medium,
 (SELECT json_group_array(j.value) FROM json_each(coalesce(o.themes,json_extract(c.data,'$.themes'),'[]')) j JOIN workspace_vocabulary t ON t.id=j.value AND t.workspace=w.workspace WHERE t.kind='theme' AND t.enabled=1) themes,
 coalesce(o.description,json_extract(d.data,'$.description'),
 (SELECT json_extract(data,'$.caption') FROM ai_results WHERE version_id=v.id AND model_id='florence' ORDER BY created_at DESC,rowid DESC LIMIT 1),'') description,
 coalesce(json_extract(d.data,'$.tags'),'[]') tags,
 o.medium IS NOT NULL manual_medium,o.themes IS NOT NULL manual_themes,o.description IS NOT NULL manual_description,w.workspace
 FROM asset_versions v JOIN asset_workspaces w ON w.asset_id=v.asset_id LEFT JOIN ai_overrides o ON o.version_id=v.id
 LEFT JOIN ai_effective_results cp ON cp.version_id=v.id AND cp.task='qwen_classification'
 LEFT JOIN ai_analysis_runs c ON c.id=cp.run_id
 LEFT JOIN ai_effective_results dp ON dp.version_id=v.id AND dp.task='qwen_description'
 LEFT JOIN ai_analysis_runs d ON d.id=dp.run_id;
CREATE VIEW ai_visible_tags AS
 SELECT DISTINCT e.version_id,trim(j.value) name FROM ai_effective_content e,json_each(e.tags) j
 WHERE j.type='text' AND trim(j.value)<>'' AND NOT EXISTS(SELECT 1 FROM ai_rejected_tags r WHERE r.version_id=e.version_id AND r.name=trim(j.value));
CREATE VIEW asset_ai_categories AS
 SELECT a.id asset_id,v.label category FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id JOIN workspace_vocabulary v ON v.id=e.medium AND v.workspace=e.workspace
 UNION SELECT a.id,v.label FROM assets a JOIN ai_effective_content e ON e.version_id=a.display_version_id,json_each(e.themes) j JOIN workspace_vocabulary v ON v.id=j.value AND v.workspace=e.workspace AND v.enabled=1
 UNION SELECT a.id,c.category FROM assets a JOIN asset_classification c ON c.asset_id=a.id
 WHERE c.manual=1 OR NOT EXISTS(SELECT 1 FROM ai_effective_content e WHERE e.version_id=a.display_version_id AND e.medium<>'');
CREATE VIEW ai_search_source AS
 SELECT e.version_id,e.description||' '||coalesce((SELECT label FROM workspace_vocabulary WHERE id=e.medium AND workspace=e.workspace),'')||' '||
 coalesce((SELECT group_concat(v.label,' ') FROM json_each(e.themes) j JOIN workspace_vocabulary v ON v.id=j.value AND v.workspace=e.workspace AND v.enabled=1),'')||' '||
 coalesce((SELECT group_concat(name,' ') FROM ai_visible_tags t WHERE t.version_id=e.version_id),'')||' '||
 coalesce((SELECT json_extract(data,'$.ocr') FROM ai_results WHERE version_id=e.version_id AND model_id='florence' ORDER BY created_at DESC,rowid DESC LIMIT 1),'') body
 FROM ai_effective_content e;
