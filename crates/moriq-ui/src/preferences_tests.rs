use super::*;

#[test]
fn snapshots_and_other_domain_receipts_never_revert_live_values_or_import_drafts() {
    let mut state = State::default();
    let mut current = UserSettings::default();
    state.hydrate(&mut current, UserSettings::default());
    current.reduced_motion = true;
    state.enqueue(Change::ReducedMotion(true));
    let (request, _) = state.begin_next().unwrap();
    current.ai.on_demand = false;
    state.enqueue(Change::Ai(current.ai));
    state.import_draft.workers = 7;
    state.import_dirty = true;
    state.hydrate(&mut current, UserSettings::default());
    assert!(current.reduced_motion);
    assert!(!current.ai.on_demand);
    assert_eq!(state.import_draft.workers, 7);
    let saved = UserSettings {
        reduced_motion: true,
        ..Default::default()
    };
    assert!(state.finish(request, &Ok(saved), &mut current));
    assert!(!current.ai.on_demand);
    assert_eq!(state.import_draft.workers, 7);
    assert!(!state.finish(request, &Ok(UserSettings::default()), &mut current));
}

#[test]
fn saves_serialize_and_coalesce_and_failure_preserves_newer_edits() {
    let mut state = State::default();
    let mut current = UserSettings::default();
    state.hydrate(&mut current, UserSettings::default());
    state.enqueue(Change::ReducedMotion(true));
    let (first, _) = state.begin_next().unwrap();
    state.enqueue(Change::ReducedMotion(false));
    state.enqueue(Change::ReducedMotion(true));
    state.enqueue(Change::DiagnosticLogging(true));
    current.reduced_motion = true;
    current.diagnostic_logging = true;
    state.close_after_save = true;
    assert!(state.begin_next().is_none());
    assert!(state.finish(first, &Err("read only".into()), &mut current));
    assert!(current.reduced_motion);
    assert!(!state.close_after_save);
    let (second, change) = state.begin_next().unwrap();
    assert_eq!(change, Change::ReducedMotion(true));
    assert!(!state.finish(first, &Ok(UserSettings::default()), &mut current));
    assert!(state.finish(
        second,
        &Ok(UserSettings {
            reduced_motion: true,
            ..Default::default()
        }),
        &mut current
    ));
    let (third, change) = state.begin_next().unwrap();
    assert_eq!(change, Change::DiagnosticLogging(true));
    assert!(state.finish(third, &Err("read only".into()), &mut current));
    assert!(!current.diagnostic_logging);
    assert!(current.reduced_motion);
    assert!(!state.pending());
}

#[test]
fn initial_load_after_edit_restores_the_real_saved_value_on_failure_and_keeps_import_draft() {
    let mut state = State::default();
    let mut current = UserSettings::default();
    current.ai.on_demand = true;
    state.enqueue(Change::Ai(current.ai));
    let (request, _) = state.begin_next().unwrap();
    state.import_draft.workers = 7;
    state.import_dirty = true;
    state.set_category_template(&mut current, AiProfile::default());
    let mut disk = UserSettings::default();
    disk.ai.on_demand = false;
    disk.import.workers = 3;
    state.hydrate(&mut current, disk);
    assert_eq!(current.import.workers, 3);
    assert_eq!(state.import_draft.workers, 7);
    assert!(current.category_template.is_some());
    assert!(state.finish(request, &Err("failed".into()), &mut current));
    assert!(!current.ai.on_demand);
}

#[test]
fn import_acknowledgement_updates_saved_values_without_discarding_later_drafts() {
    let mut state = State::default();
    let mut current = UserSettings::default();
    state.hydrate(&mut current, UserSettings::default());
    state.import_draft.workers = 7;
    state.import_dirty = true;
    state.enqueue(Change::Import(state.import_draft));
    let (request, _) = state.begin_next().unwrap();
    state.import_draft.workers = 9;
    let mut receipt = UserSettings::default();
    receipt.import.workers = 7;
    state.finish(request, &Ok(receipt), &mut current);
    assert_eq!(current.import.workers, 7);
    assert_eq!(state.import_draft.workers, 9);
    assert!(state.import_dirty);
    let old = state.request_snapshot();
    let new = state.request_snapshot();
    assert!(!state.accepts_snapshot(old));
    assert!(state.accepts_snapshot(new));
}
