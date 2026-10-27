-- 为人物按模型修订维护代表向量，并迁移已有代表数据。
CREATE TABLE person_vectors(person_id TEXT NOT NULL REFERENCES people(id) ON DELETE CASCADE,revision TEXT NOT NULL,vector BLOB NOT NULL,samples INTEGER NOT NULL,PRIMARY KEY(person_id,revision));
CREATE INDEX person_vectors_revision ON person_vectors(revision);
CREATE INDEX detected_faces_person_revision ON detected_faces(person_id,revision);
INSERT INTO person_vectors SELECT person_id,revision,vector,1 FROM detected_faces WHERE person_id IS NOT NULL GROUP BY person_id,revision HAVING id=min(id);
PRAGMA user_version=21;
