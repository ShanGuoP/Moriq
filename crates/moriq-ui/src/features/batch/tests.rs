use super::*;

fn context(owner: Owner, query: &Query, selected: AssetId) -> Context<'_> {
    Context {
        owner: Some(owner),
        query,
        selected: vec![selected],
        total: 30,
        language: Language::SimplifiedChinese,
        projects: vec![],
    }
}
#[test]
fn submission_freezes_scope_and_patch_without_app_and_rejects_other_projects_or_late_receipts() {
    let owner = (LibraryId::default(), Some(ProjectId::default()));
    let other = (owner.0, Some(ProjectId::default()));
    let asset = AssetId::default();
    let query = Query {
        project: owner.1,
        text: "saved query".into(),
        ..Default::default()
    };
    let mut state = BatchState::default();
    assert!(matches!(
        state.update(BatchMessage::Open, context(owner, &query, asset)),
        Effect::Open
    ));
    state.update(BatchMessage::All(true), context(owner, &query, asset));
    state.update(
        BatchMessage::AddTags("a， b".into()),
        context(owner, &query, asset),
    );
    assert!(matches!(
        state.update(BatchMessage::Apply, context(other, &query, asset)),
        Effect::None
    ));
    let Effect::Submit(submission) =
        state.update(BatchMessage::Apply, context(owner, &query, asset))
    else {
        panic!("missing submission");
    };
    assert_eq!(submission.owner, owner);
    let BatchScope::Query(saved) = submission.scope else {
        panic!("missing query");
    };
    assert_eq!(*saved, query);
    assert_eq!(submission.patch.add_tags, vec!["a", "b"]);
    assert!(matches!(
        state.update(BatchMessage::Apply, context(owner, &query, asset)),
        Effect::None
    ));
    state.update(BatchMessage::Open, context(other, &query, asset));
    assert!(matches!(
        state.update(
            BatchMessage::Stored(owner, submission.request, Ok(())),
            context(other, &query, asset)
        ),
        Effect::None
    ));
    let Effect::Submit(next) = state.update(BatchMessage::Apply, context(other, &query, asset))
    else {
        panic!("missing submission");
    };
    assert!(next.request > submission.request);
    assert!(matches!(
        state.update(
            BatchMessage::Stored(other, next.request, Err("failed".into())),
            context(other, &query, asset)
        ),
        Effect::Failed(_)
    ));
    assert!(matches!(
        state.update(BatchMessage::Apply, context(other, &query, asset)),
        Effect::Submit(_)
    ));
}
