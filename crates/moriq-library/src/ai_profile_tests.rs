use super::*;
fn fixture() -> (tempfile::TempDir, Library, Asset) {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("sample.png");
    image::RgbImage::from_pixel(4, 4, image::Rgb([1, 2, 3]))
        .save(&input)
        .unwrap();
    let mut lib = Library::create(dir.path(), "自定义分类").unwrap();
    lib.import(&input, None).unwrap();
    let asset = lib.page(&Query::default(), 0, 1).unwrap().assets.remove(0);
    (dir, lib, asset)
}
fn add(lib: &mut Library) -> AiProfile {
    let mut p = lib.ai_profile().unwrap();
    p.categories.push(AiCategory {
        id: "theme_stage".into(),
        kind: AiCategoryKind::Theme,
        label: "舞台表演".into(),
        definition: "舞台表演或舞台上的演出者是主体。".into(),
        enabled: true,
    });
    lib.save_ai_profile(p).unwrap().0
}
fn result(lib: &mut Library, a: &Asset, p: &AiProfile) {
    let value = analysis::qwen_payload_for_profile(
        "qwen_classification",
        serde_json::json!({"medium":"photography","themes":["theme_stage"]}),
        p,
    )
    .unwrap();
    let tx = lib.db.transaction().unwrap();
    analysis::store(&tx, &a.version_id.to_string(), "qwen35-4b", "test", &value).unwrap();
    tx.commit().unwrap();
    lib.snapshot = None;
}
#[test]
fn custom_group_has_real_count_cover_and_filter_and_rename_keeps_stable_membership() {
    let (_dir, mut lib, a) = fixture();
    let profile = add(&mut lib);
    result(&mut lib, &a, &profile);
    let before = lib
        .ai_groups()
        .unwrap()
        .into_iter()
        .find(|g| g.id == "theme_stage")
        .unwrap();
    assert_eq!(before.count, 1);
    assert_eq!(before.cover, Some(a.version_id));
    assert_eq!(lib.page(&before.query, 0, 10).unwrap().total, 1);
    assert_eq!(
        lib.page(
            &Query {
                text: "舞台表演".into(),
                ..Query::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        1
    );
    let hash = ai_profile::classification_hash(&profile).unwrap();
    let mut changed = profile;
    let last = changed.categories.pop().unwrap();
    changed.categories.insert(0, last);
    changed.categories[0].label = "演出现场".into();
    let updated = lib.save_ai_profile(changed).unwrap().0;
    assert_eq!(ai_profile::classification_hash(&updated).unwrap(), hash);
    let groups = lib.ai_groups().unwrap();
    assert_eq!(groups[0].id, "theme_stage");
    assert_eq!(groups[0].label, "演出现场");
    assert_eq!(groups[0].query, before.query);
    assert_eq!(lib.outdated_ai_count().unwrap(), 0);
    assert_eq!(
        lib.page(
            &Query {
                text: "舞台表演".into(),
                ..Query::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        0
    );
    assert_eq!(
        lib.page(
            &Query {
                text: "演出现场".into(),
                ..Query::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        1
    );
    assert!(
        lib.overview_assets()
            .unwrap()
            .categories
            .contains(&("theme_stage".into(), Some(a.version_id)))
    );
}
#[test]
fn hiding_and_restoring_a_theme_preserves_assets_raw_history_and_human_tags() {
    let (_dir, mut lib, a) = fixture();
    let mut profile = add(&mut lib);
    result(&mut lib, &a, &profile);
    lib.set_tags(a.id, &["人工标记".into()]).unwrap();
    let source = lib.version_path(&a.version_id.to_string(), false).unwrap();
    let bytes = fs::read(&source).unwrap();
    profile.categories.last_mut().unwrap().enabled = false;
    profile = lib.save_ai_profile(profile).unwrap().0;
    assert!(
        !lib.ai_groups()
            .unwrap()
            .iter()
            .any(|g| g.id == "theme_stage")
    );
    assert!(lib.details(a.id).unwrap().themes.is_empty());
    assert_eq!(
        lib.page(
            &Query {
                themes: vec!["theme_stage".into()],
                ..Query::default()
            },
            0,
            10
        )
        .unwrap()
        .total,
        0
    );
    let raw: String = lib
        .db
        .query_row("SELECT data FROM ai_analysis_runs LIMIT 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(raw.contains("theme_stage"));
    assert_eq!(lib.details(a.id).unwrap().manual_tags, vec!["人工标记"]);
    assert_eq!(fs::read(&source).unwrap(), bytes);
    profile.categories.last_mut().unwrap().enabled = true;
    lib.save_ai_profile(profile).unwrap();
    assert_eq!(
        lib.ai_groups()
            .unwrap()
            .iter()
            .find(|g| g.id == "theme_stage")
            .unwrap()
            .count,
        1
    );
}
#[test]
fn profile_is_library_local_durable_and_concurrent_edits_are_rejected() {
    let (dir, mut lib, _a) = fixture();
    let p = add(&mut lib);
    let root = lib.root.clone();
    let other = Library::create(dir.path(), "另一个库").unwrap();
    assert!(
        !other
            .ai_profile()
            .unwrap()
            .categories
            .iter()
            .any(|c| c.id == "theme_stage")
    );
    drop(other);
    let old = p.clone();
    let mut p = p;
    p.max_themes = 4;
    let saved = lib.save_ai_profile(p).unwrap().0;
    assert!(lib.save_ai_profile(old).is_err());
    drop(lib);
    let lib = Library::open(&root).unwrap();
    assert_eq!(lib.ai_profile().unwrap(), saved);
}
#[test]
fn definition_changes_cancel_old_jobs_and_reject_late_classification_receipts() {
    let (_dir, mut lib, a) = fixture();
    let mut profile = add(&mut lib);
    let job = lib
        .enqueue_media_job(JobKind::QwenClassification, 0)
        .unwrap();
    let work = lib.claim_job_step().unwrap().unwrap();
    let request = match work.execute(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
        false,
    ))) {
        jobs::Execution::QwenClassification(r) => r,
        _ => panic!("wrong work"),
    };
    assert_eq!(request.profile().unwrap(), &profile);
    profile.categories.last_mut().unwrap().definition =
        "仅在舞台照明和表演动作都清晰可见时归入。".into();
    let (updated, cancelled) = lib.save_ai_profile(profile).unwrap();
    assert_eq!(cancelled, vec![job]);
    let outcome = request.complete_qwen(
        "qwen_classification",
        Ok(analysis::InferenceResult {
            model: ModelId::Qwen35_4b,
            revision: "test".into(),
            data: serde_json::json!({"medium":"photography","themes":["theme_stage"]}),
        }),
    );
    lib.finish_job_step(outcome).unwrap();
    assert!(lib.details(a.id).unwrap().themes.is_empty());
    result(&mut lib, &a, &updated);
    assert_eq!(lib.outdated_ai_count().unwrap(), 0);
    let mut changed = updated;
    changed.max_themes = 4;
    lib.save_ai_profile(changed).unwrap();
    assert_eq!(lib.outdated_ai_count().unwrap(), 1);
}
#[test]
fn rename_does_not_cancel_jobs_and_label_swaps_are_atomic() {
    let (_dir, mut lib, _a) = fixture();
    let mut p = lib.ai_profile().unwrap();
    let job = lib
        .enqueue_media_job(JobKind::QwenClassification, 0)
        .unwrap();
    let a = p.categories[0].label.clone();
    p.categories[0].label = p.categories[1].label.clone();
    p.categories[1].label = a;
    let (_, cancelled) = lib.save_ai_profile(p).unwrap();
    assert!(cancelled.is_empty());
    assert_eq!(
        lib.background_jobs()
            .unwrap()
            .iter()
            .find(|j| j.id == job)
            .unwrap()
            .state,
        JobState::Queued
    );
}
#[test]
fn disabled_medium_cancels_old_classification_and_cannot_be_returned_by_the_model() {
    let (_dir, mut lib, a) = fixture();
    let mut p = add(&mut lib);
    result(&mut lib, &a, &p);
    lib.set_tags(a.id, &["人工保留".into()]).unwrap();
    let hash = ai_profile::classification_hash(&p).unwrap();
    let job = lib
        .enqueue_media_job(JobKind::QwenClassification, 0)
        .unwrap();
    let request = match lib
        .claim_job_step()
        .unwrap()
        .unwrap()
        .execute(std::sync::Arc::new(std::sync::atomic::AtomicBool::new(
            false,
        ))) {
        jobs::Execution::QwenClassification(r) => r,
        _ => panic!("wrong work"),
    };
    let description = lib.enqueue_media_job(JobKind::QwenDescription, 0).unwrap();
    for c in &mut p.categories {
        if c.id == "photography" || c.id == "uncertain" {
            c.enabled = false;
        }
    }
    let (mut saved, cancelled) = lib.save_ai_profile(p).unwrap();
    assert_eq!(cancelled, vec![job]);
    let disabled_hash = ai_profile::classification_hash(&saved).unwrap();
    assert_ne!(disabled_hash, hash);
    assert_eq!(lib.outdated_ai_count().unwrap(), 1);
    assert!(
        lib.background_jobs()
            .unwrap()
            .iter()
            .any(|j| j.id == description && j.state == JobState::Queued)
    );
    let late = request.complete_qwen(
        "qwen_classification",
        Ok(analysis::InferenceResult {
            model: ModelId::Qwen35_4b,
            revision: "late".into(),
            data: serde_json::json!({"medium":"photography","themes":[]}),
        }),
    );
    lib.finish_job_step(late).unwrap();
    // Disabling future inference preserves previously stored records and human content.
    assert_eq!(lib.details(a.id).unwrap().medium, "photography");
    assert_eq!(lib.details(a.id).unwrap().manual_tags, vec!["人工保留"]);
    assert!(
        !lib.ai_groups()
            .unwrap()
            .iter()
            .any(|g| g.id == "photography")
    );
    for id in ["photography", "uncertain", "made_up"] {
        assert!(
            analysis::qwen_payload_for_profile(
                "qwen_classification",
                serde_json::json!({"medium":id,"themes":[]}),
                &saved
            )
            .is_err()
        );
    }
    saved
        .categories
        .iter_mut()
        .find(|c| c.id == "photography")
        .unwrap()
        .definition = "已禁用的说明修改不应影响当前识别".into();
    assert_eq!(
        ai_profile::classification_hash(&saved).unwrap(),
        disabled_hash
    );
    for c in &mut saved.categories {
        c.enabled = false;
    }
    saved.validate().unwrap();
    let value = analysis::qwen_payload_for_profile(
        "qwen_classification",
        serde_json::json!({"medium":"","themes":[]}),
        &saved,
    )
    .unwrap();
    assert_eq!(value["medium"], "");
    assert!(
        analysis::qwen_payload_for_profile(
            "qwen_classification",
            serde_json::json!({"medium":"","themes":["portrait"]}),
            &saved
        )
        .is_err()
    );
    let tx = lib.db.transaction().unwrap();
    analysis::store(&tx, &a.version_id.to_string(), "qwen35-4b", "empty", &value).unwrap();
    tx.commit().unwrap();
    assert_eq!(lib.details(a.id).unwrap().medium, "");
    assert_eq!(lib.details(a.id).unwrap().manual_tags, vec!["人工保留"]);
}
#[test]
fn profile_validation_and_custom_payload_validation_do_not_broaden_rules() {
    let (_dir, mut lib, _a) = fixture();
    let profile = add(&mut lib);
    assert!(
        analysis::qwen_payload(
            "qwen_classification",
            serde_json::json!({"medium":"photography","themes":["theme_stage"]})
        )
        .is_err()
    );
    assert!(
        analysis::qwen_payload_for_profile(
            "qwen_classification",
            serde_json::json!({"medium":"photography","themes":["theme_stage"]}),
            &profile
        )
        .is_ok()
    );
    let mut invalid = profile.clone();
    invalid.categories[1].label = invalid.categories[0].label.clone();
    assert!(lib.save_ai_profile(invalid).is_err());
    let mut invalid = profile.clone();
    invalid.categories.remove(0);
    assert!(lib.save_ai_profile(invalid).is_err());
    let mut invalid = profile.clone();
    invalid.categories.last_mut().unwrap().id = "changed_id".into();
    assert!(lib.save_ai_profile(invalid).is_err());
    let mut invalid = profile.clone();
    invalid.categories.last_mut().unwrap().id = "_unclassified".into();
    assert!(lib.save_ai_profile(invalid).is_err());
    assert_eq!(lib.ai_profile().unwrap(), profile);
}
