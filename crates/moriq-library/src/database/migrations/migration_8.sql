-- 增加项目父级，并用全文索引和触发器维护名称、笔记与标签搜索。
ALTER TABLE projects ADD COLUMN parent_id TEXT REFERENCES projects(id) ON DELETE SET NULL;
CREATE INDEX projects_parent ON projects(parent_id);
CREATE INDEX versions_path_nocase ON asset_versions(path COLLATE NOCASE);
CREATE INDEX project_files_path_nocase ON project_files(path COLLATE NOCASE);
CREATE TABLE search_keys(fts_row INTEGER PRIMARY KEY,asset_id TEXT NOT NULL UNIQUE REFERENCES assets(id) ON DELETE CASCADE);
INSERT INTO search_keys(asset_id) SELECT id FROM assets;
CREATE VIRTUAL TABLE search_fts USING fts5(name,notes,tags,asset_id UNINDEXED,tokenize='trigram');
INSERT INTO search_fts(rowid,name,notes,tags,asset_id)
SELECT k.fts_row,a.name,a.notes,coalesce((SELECT group_concat(t.name,' ') FROM asset_tags x JOIN tags t ON t.id=x.tag_id WHERE x.asset_id=a.id),''),a.id FROM assets a JOIN search_keys k ON k.asset_id=a.id;
CREATE TRIGGER search_asset_insert AFTER INSERT ON assets BEGIN
 INSERT INTO search_keys(asset_id) VALUES(new.id);
 INSERT INTO search_fts(rowid,name,notes,tags,asset_id) VALUES(last_insert_rowid(),new.name,new.notes,'',new.id);
END;
CREATE TRIGGER search_asset_update AFTER UPDATE OF name,notes ON assets BEGIN
 UPDATE search_fts SET name=new.name,notes=new.notes WHERE rowid=(SELECT fts_row FROM search_keys WHERE asset_id=new.id);
END;
CREATE TRIGGER search_asset_delete BEFORE DELETE ON assets BEGIN
 DELETE FROM search_fts WHERE rowid=(SELECT fts_row FROM search_keys WHERE asset_id=old.id);
 DELETE FROM search_keys WHERE asset_id=old.id;
END;
CREATE TRIGGER search_tag_insert AFTER INSERT ON asset_tags BEGIN
 UPDATE search_fts SET tags=coalesce((SELECT group_concat(t.name,' ') FROM asset_tags x JOIN tags t ON t.id=x.tag_id WHERE x.asset_id=new.asset_id),'') WHERE rowid=(SELECT fts_row FROM search_keys WHERE asset_id=new.asset_id);
END;
CREATE TRIGGER search_tag_delete AFTER DELETE ON asset_tags BEGIN
 UPDATE search_fts SET tags=coalesce((SELECT group_concat(t.name,' ') FROM asset_tags x JOIN tags t ON t.id=x.tag_id WHERE x.asset_id=old.asset_id),'') WHERE rowid=(SELECT fts_row FROM search_keys WHERE asset_id=old.asset_id);
END;
CREATE TRIGGER search_tag_rename AFTER UPDATE OF name ON tags BEGIN
 UPDATE search_fts SET tags=coalesce((SELECT group_concat(t.name,' ') FROM asset_tags x JOIN tags t ON t.id=x.tag_id WHERE x.asset_id=search_fts.asset_id),'') WHERE rowid IN(SELECT fts_row FROM search_keys WHERE asset_id IN(SELECT asset_id FROM asset_tags WHERE tag_id=new.id));
END;
PRAGMA user_version=8;
