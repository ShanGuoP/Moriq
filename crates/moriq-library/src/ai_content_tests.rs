use super::*;

pub(crate) fn downgrade_to_22(db: &Connection) {
    db.execute_batch("DROP TABLE IF EXISTS ai_profile_state;")
        .unwrap();
    let triggers = db.prepare("SELECT name FROM sqlite_master WHERE type='trigger' AND (name LIKE 'content_ai_%' OR name='ai_document_delete')").unwrap()
        .query_map([], |r| r.get::<_, String>(0)).unwrap().collect::<std::result::Result<Vec<_>, _>>().unwrap();
    for name in triggers {
        db.execute_batch(&format!("DROP TRIGGER \"{name}\";"))
            .unwrap();
    }
    db.execute_batch("DROP VIEW ai_search_source; DROP VIEW asset_ai_categories; DROP VIEW effective_asset_tags; DROP VIEW ai_visible_tags; DROP VIEW ai_effective_content; DROP TABLE ai_effective_results; DROP TABLE ai_analysis_runs; DROP TABLE ai_overrides; DROP TABLE ai_rejected_tags; DROP TABLE ai_vocabulary; PRAGMA user_version=22;").unwrap();
}

fn fixture() -> (tempfile::TempDir, Library, Asset) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("plain.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([220, 90, 140]))
        .save(&input)
        .unwrap();
    let mut lib = Library::create(dir.path(), "内容闭环").unwrap();
    lib.import(&input, None).unwrap();
    let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
    (dir, lib, asset)
}
fn classify(lib: &mut Library, a: &Asset, revision: &str, medium: &str, themes: &[&str]) {
    save(
        lib,
        a,
        revision,
        "qwen_classification",
        serde_json::json!({"medium":medium,"themes":themes}),
    );
}
fn describe(lib: &mut Library, a: &Asset, revision: &str, description: &str, tags: &[&str]) {
    save(
        lib,
        a,
        revision,
        "qwen_description",
        serde_json::json!({"description":description,"tags":tags}),
    );
}
fn save(lib: &mut Library, a: &Asset, revision: &str, task: &'static str, data: serde_json::Value) {
    let value = analysis::qwen_payload(task, data).unwrap();
    let tx = lib.db.transaction().unwrap();
    analysis::store(
        &tx,
        &a.version_id.to_string(),
        "qwen35-4b",
        revision,
        &value,
    )
    .unwrap();
    tx.commit().unwrap();
    lib.snapshot = None;
}
fn count(lib: &mut Library, query: Query) -> u64 {
    lib.page(&query, 0, 10).unwrap().total
}
fn text(lib: &mut Library, s: &str) -> u64 {
    count(
        lib,
        Query {
            text: s.into(),
            ..Query::default()
        },
    )
}
fn edit(lib: &mut Library, a: &Asset, value: AiEdit) {
    lib.edit_ai_content(a.id, a.version_id, value).unwrap();
}

#[test]
fn medium_and_multiple_themes_match_independently_and_count_once() {
    let (_dir, mut lib, a) = fixture();
    classify(&mut lib, &a, "r1", "photography", &["portrait", "cosplay"]);
    for category in ["摄影", "人像", "角色扮演"] {
        assert_eq!(
            count(
                &mut lib,
                Query {
                    category: category.into(),
                    ..Query::default()
                }
            ),
            1
        );
    }
    assert_eq!(
        count(
            &mut lib,
            Query {
                medium: "photography".into(),
                themes: vec!["portrait".into(), "cosplay".into()],
                ..Query::default()
            }
        ),
        1
    );
    assert_eq!(
        count(
            &mut lib,
            Query {
                medium: "illustration".into(),
                themes: vec!["portrait".into()],
                ..Query::default()
            }
        ),
        0
    );
    let organizer = lib.organizer().unwrap();
    assert!(!organizer.ai_categories.iter().any(|(s, _)| s == "未分类"));
    for category in ["摄影", "人像", "角色扮演"] {
        assert!(organizer.ai_categories.contains(&(category.into(), 1)));
        assert!(
            lib.overview_assets().unwrap().categories.contains(&(
                AI_MEDIA
                    .iter()
                    .chain(AI_THEMES)
                    .find(|(_, label)| *label == category)
                    .unwrap()
                    .0
                    .into(),
                Some(a.version_id)
            ))
        );
    }
}

#[test]
fn ai_tags_are_immediately_visible_exact_searchable_and_used_by_rules_and_export() {
    let (dir, mut lib, a) = fixture();
    describe(
        &mut lib,
        &a,
        "r1",
        "人物手持道具",
        &["灯笼", "  Cosplay  ", "角色扮演", "猫形灯笼", "%_\""],
    );
    assert_eq!(text(&mut lib, "灯"), 1);
    assert_eq!(text(&mut lib, "灯笼"), 1);
    assert_eq!(text(&mut lib, "猫形灯笼"), 1);
    assert_eq!(text(&mut lib, "%_\""), 1);
    assert_eq!(text(&mut lib, "不存在"), 0);
    assert_eq!(
        count(
            &mut lib,
            Query {
                tag: "灯笼".into(),
                ..Query::default()
            }
        ),
        1
    );
    assert_eq!(
        count(
            &mut lib,
            Query {
                rule: Some(Rule::Tag("灯笼".into())),
                ..Query::default()
            }
        ),
        1
    );
    assert_eq!(
        count(
            &mut lib,
            Query {
                untagged: true,
                ..Query::default()
            }
        ),
        0
    );
    let tags = lib
        .page(&Query::default(), 0, 1)
        .unwrap()
        .assets
        .remove(0)
        .tags;
    assert_eq!(tags.iter().filter(|s| *s == "角色扮演").count(), 1);
    assert!(tags.contains(&"猫形灯笼".into()));
    let plan = lib.export_plan(&[a.id]).unwrap();
    let out = export::run(
        plan,
        dir.path(),
        ExportOptions {
            metadata_sidecar: true,
            ..ExportOptions::default()
        },
    )
    .unwrap();
    let sidecar = fs::read_dir(out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|s| s == "json"))
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&fs::read(sidecar).unwrap()).unwrap();
    assert_eq!(json["description"], "人物手持道具");
    assert!(
        json["tags"]
            .as_array()
            .unwrap()
            .contains(&serde_json::json!("灯笼"))
    );
}

#[test]
fn reruns_replace_search_terms_without_discarding_history_or_the_other_task() {
    let (_dir, mut lib, a) = fixture();
    classify(&mut lib, &a, "r1", "photography", &["portrait"]);
    describe(&mut lib, &a, "r1", "旧描述独有", &["旧标签独有"]);
    describe(&mut lib, &a, "r2", "新描述内容", &["新词"]);
    assert_eq!(text(&mut lib, "旧描述"), 0);
    assert_eq!(text(&mut lib, "旧标签"), 0);
    assert_eq!(text(&mut lib, "新词"), 1);
    assert_eq!(lib.details(a.id).unwrap().medium, "photography");
    classify(&mut lib, &a, "r3", "illustration", &[]);
    assert_eq!(lib.details(a.id).unwrap().ai_caption, "新描述内容");
    assert_eq!(text(&mut lib, "摄影"), 0);
    assert_eq!(
        lib.db
            .query_row("SELECT count(*) FROM ai_analysis_runs", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        4
    );
}

#[test]
fn human_edits_rejections_and_promotions_survive_reanalysis_and_reopening() {
    let (_dir, mut lib, a) = fixture();
    classify(&mut lib, &a, "r1", "photography", &["portrait", "cosplay"]);
    describe(
        &mut lib,
        &a,
        "r1",
        "自动原文",
        &["长标签删除", "长标签保留"],
    );
    lib.edit(&[a.id], &Edit::Notes("原始备注".into())).unwrap();
    edit(&mut lib, &a, AiEdit::RejectTag("长标签删除".into()));
    edit(&mut lib, &a, AiEdit::PromoteTag("长标签保留".into()));
    edit(
        &mut lib,
        &a,
        AiEdit::Classification("illustration".into(), vec![]),
    );
    edit(&mut lib, &a, AiEdit::Description(Some("人工描述".into())));
    classify(&mut lib, &a, "r2", "photography", &["cosplay"]);
    describe(&mut lib, &a, "r2", "新的自动原文", &["长标签删除"]);
    assert_eq!(text(&mut lib, "长标签删除"), 0);
    assert_eq!(text(&mut lib, "长标签保留"), 1);
    assert_eq!(text(&mut lib, "自动原文"), 0);
    let root = lib.root.clone();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    let details = lib.details(a.id).unwrap();
    assert_eq!(details.medium, "illustration");
    assert!(details.themes.is_empty());
    assert_eq!(details.ai_caption, "人工描述");
    assert_eq!(details.notes, "原始备注");
    assert!(details.manual_tags.contains(&"长标签保留".into()));
    edit(&mut lib, &a, AiEdit::Medium(None));
    edit(&mut lib, &a, AiEdit::Themes(None));
    edit(&mut lib, &a, AiEdit::Description(None));
    edit(&mut lib, &a, AiEdit::ResetRejectedTags);
    assert_eq!(lib.details(a.id).unwrap().medium, "photography");
    assert_eq!(text(&mut lib, "新的自动原文"), 1);
    assert_eq!(text(&mut lib, "长标签删除"), 1);
}

#[test]
fn merged_tag_editor_and_global_tag_management_preserve_sources_and_reject_removed_ai() {
    let (_dir, mut lib, a) = fixture();
    describe(&mut lib, &a, "r1", "", &["自动甲", "自动乙"]);
    lib.set_tags(a.id, &["自动甲".into(), "人工丙".into()])
        .unwrap();
    let d = lib.details(a.id).unwrap();
    assert_eq!(d.manual_tags, vec!["人工丙"]);
    assert_eq!(d.ai_tags, vec!["自动甲"]);
    lib.rename_tag("自动甲", "手动新名").unwrap();
    assert_eq!(text(&mut lib, "自动甲"), 0);
    assert_eq!(text(&mut lib, "手动新名"), 1);
    describe(&mut lib, &a, "r2", "", &["自动甲", "自动乙", "待删除"]);
    assert_eq!(text(&mut lib, "自动乙"), 0);
    lib.delete_tag("待删除").unwrap();
    assert_eq!(text(&mut lib, "待删除"), 0);
    lib.edit(&[a.id], &Edit::RemoveTags(vec!["自动甲".into()]))
        .unwrap();
    assert_eq!(text(&mut lib, "自动甲"), 0);
}

#[test]
fn results_and_overrides_are_bound_to_display_version_and_clear_preserves_human_content() {
    let (_dir, mut lib, a) = fixture();
    describe(&mut lib, &a, "r1", "旧版结果", &["旧版标签"]);
    let new_version = AssetVersionId::default();
    lib.db.execute("INSERT INTO asset_versions(id,asset_id,path,sha256,bytes,width,height,format,origin) SELECT ?1,asset_id,'versions/new.png',sha256,bytes,width,height,format,origin FROM asset_versions WHERE id=?2", params![new_version.to_string(), a.version_id.to_string()]).unwrap();
    lib.display_version(a.id, new_version).unwrap();
    assert_eq!(text(&mut lib, "旧版结果"), 0);
    assert!(
        lib.edit_ai_content(a.id, a.version_id, AiEdit::Description(Some("迟到".into())))
            .is_err()
    );
    lib.display_version(a.id, a.version_id).unwrap();
    assert_eq!(text(&mut lib, "旧版标签"), 1);
    edit(
        &mut lib,
        &a,
        AiEdit::Description(Some("保留人工描述".into())),
    );
    edit(&mut lib, &a, AiEdit::PromoteTag("旧版标签".into()));
    lib.clear_ai_indexes("qwen35-4b").unwrap();
    assert_eq!(text(&mut lib, "旧版结果"), 0);
    assert_eq!(text(&mut lib, "保留人工描述"), 1);
    assert_eq!(text(&mut lib, "旧版标签"), 1);
}

#[test]
fn schema_22_migration_converts_actual_qwen_labels_preserves_legacy_ocr_and_manual_category() {
    let (_dir, mut lib, a) = fixture();
    downgrade_to_22(&lib.db);
    let old = serde_json::json!({"medium":"摄影","themes":["人像","角色扮演"],"description":"迁移描述","tags":["迁移标签"]});
    lib.db
        .execute(
            "INSERT INTO ai_results VALUES(?1,'qwen35-4b','old','srgb-1024-v1',?2,1)",
            params![a.version_id.to_string(), old.to_string()],
        )
        .unwrap();
    lib.db
        .execute(
            "INSERT INTO ai_results VALUES(?1,'florence','old','old',?2,1)",
            params![
                a.version_id.to_string(),
                serde_json::json!({"caption":"旧的重复描述","ocr":"历史文字"}).to_string()
            ],
        )
        .unwrap();
    lib.set_category(&[a.id], "人工旧类").unwrap();
    let root = lib.root.clone();
    drop(lib);
    let mut lib = Library::open(&root).unwrap();
    let d = lib.details(a.id).unwrap();
    assert_eq!(d.medium, "photography");
    assert!(d.themes.contains(&"cosplay".into()));
    assert_eq!(d.category, "人工旧类");
    assert_eq!(d.ocr, "历史文字");
    assert_eq!(text(&mut lib, "迁移标签"), 1);
    assert_eq!(text(&mut lib, "历史文字"), 1);
    assert_eq!(text(&mut lib, "旧的重复描述"), 0);
    assert!(fs::read_dir(root.join("backups")).unwrap().any(|e| {
        e.unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("schema-22-")
    }));
}

#[test]
fn malformed_outputs_do_not_become_effective_results() {
    assert!(
        analysis::qwen_payload(
            "qwen_classification",
            serde_json::json!({"medium":"made_up","themes":[]})
        )
        .is_err()
    );
    assert!(
        analysis::qwen_payload(
            "qwen_classification",
            serde_json::json!({"medium":"photography","themes":["made_up"]})
        )
        .is_err()
    );
    assert!(
        analysis::qwen_payload(
            "qwen_description",
            serde_json::json!({"description":"ok","tags":[42]})
        )
        .is_err()
    );
}
