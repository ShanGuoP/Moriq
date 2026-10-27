-- 标签只来自当前生效的描述结果，无需展开每个版本的分类、主题和人物范围。
DROP VIEW ai_visible_tags;
CREATE VIEW ai_visible_tags AS
 SELECT DISTINCT p.version_id,trim(j.value) name
 FROM ai_effective_results p JOIN ai_analysis_runs r ON r.id=p.run_id,
 json_each(coalesce(json_extract(r.data,'$.tags'),'[]')) j
 WHERE p.task='qwen_description' AND j.type='text' AND trim(j.value)<>''
 AND NOT EXISTS(SELECT 1 FROM ai_rejected_tags x WHERE x.version_id=p.version_id AND x.name=trim(j.value));
PRAGMA user_version=27;
