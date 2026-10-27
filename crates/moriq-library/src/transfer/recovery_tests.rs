use super::*;
fn interrupted(referenced: bool) -> (tempfile::TempDir, Library, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("photo.png");
    image::RgbImage::from_pixel(16, 12, image::Rgb([12, 34, 56]))
        .save(&source)
        .unwrap();
    let mut library = Library::create(dir.path(), "recovery").unwrap();
    let file = ImportFile {
        path: source.clone(),
        relative: PathBuf::from("photo.png"),
        project_name: None,
    };
    let ticket = library.begin_import(&file, None, referenced).unwrap();
    ticket.copy(&AtomicBool::new(false)).unwrap();
    library.interrupt_import(&ticket).unwrap();
    (dir, library, source)
}
#[test]
fn changed_journal_rejects_prepared_media_and_preserves_recoverable_input() {
    let (_dir, mut library, source) = interrupted(false);
    let original = fs::read(&source).unwrap();
    let plan = library.recovery_plans().unwrap().remove(0);
    let cancel = Arc::new(AtomicBool::new(false));
    let ready = plan
        .freeze(cancel.clone())
        .unwrap()
        .prepare(cancel)
        .unwrap();
    library
        .db
        .execute("UPDATE import_context SET relative_path='changed.png'", [])
        .unwrap();
    assert!(library.finish_recovery(ready).is_err());
    assert_eq!(library.info().unwrap().count, 0);
    assert_eq!(fs::read(&source).unwrap(), original);
    assert_eq!(
        fs::read_dir(library.root.join("backups")).unwrap().count(),
        0
    );
    assert_eq!(library.recover_imports().unwrap().imported, 1);
}
#[test]
fn reference_changes_and_cancelled_preparation_never_publish_or_destroy_journals() {
    let (_dir, library, source) = interrupted(true);
    let cancel = Arc::new(AtomicBool::new(false));
    let work = library
        .recovery_plans()
        .unwrap()
        .remove(0)
        .freeze(cancel.clone())
        .unwrap();
    fs::write(&source, b"changed external source").unwrap();
    assert!(work.prepare(cancel.clone()).is_err());
    assert_eq!(library.info().unwrap().count, 0);
    let work = library
        .recovery_plans()
        .unwrap()
        .remove(0)
        .freeze(cancel.clone())
        .unwrap();
    cancel.store(true, std::sync::atomic::Ordering::Release);
    assert!(work.prepare(cancel).is_err());
    assert_eq!(library.recovery_plans().unwrap().len(), 1);
    assert_eq!(fs::read(&source).unwrap(), b"changed external source");
    assert_eq!(
        fs::read_dir(library.root.join("backups")).unwrap().count(),
        0
    );
}
