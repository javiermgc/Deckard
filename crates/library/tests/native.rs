use deckard_library::{user::Root, *};
use deckard_ni_content::{DiscoveryReport, Installation};
use std::path::Path;
fn report(root: &Path) -> DiscoveryReport {
    DiscoveryReport {
        installations: vec![Installation::registered("fixture", "Expansion", Some(root.into()), None)],
        complete_sources: vec!["fixture".into()],
        ..Default::default()
    }
}
fn scan(lib: &mut Library, r: DiscoveryReport, manual: Vec<Root>) {
    lib.begin_scan(ScanRequest { generation: 1, discovery: r, manual }).unwrap();
    for _ in 0..1000 {
        if lib.scan_step(2).unwrap().finished {
            return;
        }
    }
    panic!("scan did not finish");
}
#[test]
fn native_manual_merge_migration_and_root_move() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Libraries/Expansion Library");
    std::fs::create_dir_all(&root).unwrap();
    let p = root.join("Hit.wav");
    std::fs::write(&p, b"RIFF\x04\0\0\0WAVE").unwrap();
    let db = t.path().join("index.db");
    let mut lib = Library::new(None, &db).unwrap();
    let manual = Root { path: t.path().join("Libraries"), factory: true, product: Some("Libraries".into()) };
    lib.scan(&[manual.clone()]).unwrap();
    lib.set_favorite(&p, true);
    scan(&mut lib, report(&root), vec![manual]);
    let q = Query::default();
    let r = lib.query(&q);
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].product.as_deref(), Some("Expansion"));
    assert!(r.items[0].favorite);
    let key = r.items[0].favorite_key();
    let new = t.path().join("Relocated");
    std::fs::rename(&root, &new).unwrap();
    scan(&mut lib, report(&new), vec![]);
    let r = lib.query(&q);
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].favorite_key(), key);
    assert!(r.items[0].favorite);
    lib.set_favorite_item(&r.items[0], false);
    drop(lib);
    let mut lib = Library::new(None, &db).unwrap();
    scan(&mut lib, report(&new), vec![]);
    assert!(!lib.query(&q).items[0].favorite);
}
#[test]
fn cancelled_scan_keeps_published_rows_and_unavailable_preserves_cache() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Old.nki"), b"opaque").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    let q = Query { kind: Kind::Instrument, ..Default::default() };
    assert_eq!(lib.query(&q).items.len(), 1);
    std::fs::write(root.join("New.nki"), b"opaque").unwrap();
    lib.begin_scan(ScanRequest { generation: 2, discovery: report(&root), manual: vec![] }).unwrap();
    lib.scan_step(1).unwrap();
    lib.cancel_scan().unwrap();
    assert_eq!(lib.query(&q).items.len(), 1);
    let moved = t.path().join("offline");
    std::fs::rename(&root, &moved).unwrap();
    scan(&mut lib, report(&root), vec![]);
    assert!(lib.query(&q).items.is_empty());
    std::fs::rename(moved, &root).unwrap();
    scan(&mut lib, report(&root), vec![]);
    assert_eq!(lib.query(&q).items.len(), 2);
}
#[test]
fn saved_user_override_and_forget_do_not_destroy_native() {
    let t = tempfile::tempdir().unwrap();
    std::fs::write(t.path().join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(t.path()), vec![]);
    lib.add_saved(&t.path().join("Hit.wav"));
    assert!(lib.query(&Query::default()).items.is_empty());
    assert_eq!(lib.query(&Query { user: true, ..Default::default() }).items.len(), 1);
    lib.forget(t.path());
    assert_eq!(lib.query(&Query::default()).items.len(), 1);
}
#[test]
fn native_tags_feed_facets_and_changed_size_invalidates_cache() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    let mut info = mxprj::sound_info::SoundInfo::default();
    info.name = "Kit".into();
    info.set_tags(&[vec!["Kits".into(), "Percussion".into()]], &["Acoustic".into()]);
    let path = root.join("Kit.mxgrp");
    std::fs::write(&path, mxprj::container::new_file(*b"MAS2", b"archive", "test", &info, &mut || [0; 16])).unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    let q = Query { kind: Kind::Group, ..Default::default() };
    let r = lib.query(&q);
    assert_eq!(r.types, vec![("Kits".into(), 1)]);
    assert_eq!(r.characters, vec![("Acoustic".into(), 1)]);
    let r = lib.query(&Query { types: vec!["Kits".into()], ..q.clone() });
    assert_eq!(r.types, vec![("Percussion".into(), 1)]);
    info.name = "Changed kit".into();
    std::fs::write(&path, mxprj::container::new_file(*b"MAS2", b"archive", "test", &info, &mut || [0; 16])).unwrap();
    scan(&mut lib, report(&root), vec![]);
    assert_eq!(lib.query(&q).items[0].name, "Changed kit");
}
#[test]
fn failed_provider_retains_entries_but_complete_absence_retires_them() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.nki"), b"opaque").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    let q = Query { kind: Kind::Instrument, ..Default::default() };
    scan(&mut lib, DiscoveryReport::default(), vec![]);
    assert_eq!(lib.query(&q).items.len(), 1);
    scan(&mut lib, DiscoveryReport { complete_sources: vec!["fixture".into()], ..Default::default() }, vec![]);
    assert!(lib.query(&q).items.is_empty());
}
#[cfg(unix)]
#[test]
fn native_walk_handles_deep_folders_symlink_cycles_and_skips_previews() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    let deep = root.join("a/b/c/d/e/f/g/h/i/j/k");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(deep.join("Deep.nki"), b"opaque").unwrap();
    std::os::unix::fs::symlink(&root, deep.join("cycle")).unwrap();
    std::fs::create_dir(root.join(".previews")).unwrap();
    std::fs::write(root.join(".previews/skip.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    assert_eq!(lib.query(&Query { kind: Kind::Instrument, ..Default::default() }).items.len(), 1);
    assert!(lib.query(&Query::default()).items.is_empty());
}
#[test]
fn saved_override_survives_overlapping_manual_refresh() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Libraries/Expansion");
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("Hit.wav");
    std::fs::write(&path, b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    let manual = vec![Root { path: t.path().join("Libraries"), factory: true, product: Some("Libraries".into()) }];
    scan(&mut lib, report(&root), manual.clone());
    lib.add_saved(&path);
    scan(&mut lib, report(&root), manual);
    assert_eq!(lib.query(&Query { user: true, ..Default::default() }).items.len(), 1);
    assert!(lib.query(&Query::default()).items.is_empty());
}
#[test]
fn migrated_suppression_can_be_favorited_again() {
    let t = tempfile::tempdir().unwrap();
    let path = t.path().join("Hit.wav");
    std::fs::write(&path, b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    lib.set_favorite(&path, false);
    scan(&mut lib, report(t.path()), vec![]);
    let item = lib.query(&Query::default()).items.remove(0);
    assert!(!item.favorite);
    lib.set_favorite_item(&item, true);
    assert!(lib.query(&Query::default()).items[0].favorite);
}
#[test]
fn concurrent_scanners_do_not_publish_each_others_staging() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    for name in ["A.nki", "B.nki", "C.nki"] {
        std::fs::write(root.join(name), b"opaque").unwrap();
    }
    let db = t.path().join("db");
    let mut a = Library::new(None, &db).unwrap();
    let mut b = Library::new(None, &db).unwrap();
    a.begin_scan(ScanRequest { generation: 1, discovery: report(&root), manual: vec![] }).unwrap();
    a.scan_step(1).unwrap();
    b.begin_scan(ScanRequest { generation: 2, discovery: report(&root), manual: vec![] }).unwrap();
    while !a.scan_step(64).unwrap().finished {}
    assert_eq!(a.query(&Query { kind: Kind::Instrument, ..Default::default() }).items.len(), 3);
}
#[test]
fn confirmed_uninstall_releases_manual_root() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    scan(
        &mut lib,
        DiscoveryReport { complete_sources: vec!["fixture".into()], ..Default::default() },
        vec![Root { path: root, factory: true, product: Some("Manual".into()) }],
    );
    assert_eq!(lib.query(&Query::default()).items[0].product.as_deref(), Some("Manual"));
}
#[cfg(unix)]
#[test]
fn manual_alias_cannot_override_registered_product() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let alias = t.path().join("Alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![Root { path: alias, factory: true, product: Some("Wrong".into()) }]);
    let r = lib.query(&Query::default());
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].product.as_deref(), Some("Expansion"));
}
#[test]
fn unchanged_uppercase_manual_presets_are_not_reparsed() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static READS: AtomicUsize = AtomicUsize::new(0);
    fn read(p: &Path) -> Option<Item> {
        READS.fetch_add(1, Ordering::SeqCst);
        Some(Item { path: p.into(), name: "Preset".into(), kind: Kind::Instrument, user: true, ..Default::default() })
    }
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("User");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Lead.NKSF"), b"opaque").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    lib.set_reader(read);
    let roots = vec![Root { path: root, factory: false, product: None }];
    scan(&mut lib, DiscoveryReport::default(), roots.clone());
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    scan(&mut lib, DiscoveryReport::default(), roots);
    assert_eq!(READS.load(Ordering::SeqCst), 1);
    assert_eq!(lib.query(&Query { kind: Kind::Instrument, user: true, ..Default::default() }).items.len(), 1);
}
#[test]
fn removed_provider_retires_items_without_erasing_favorites() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    let item = lib.query(&Query::default()).items.remove(0);
    lib.set_favorite_item(&item, true);
    scan(&mut lib, DiscoveryReport { sources: Some(vec![]), ..Default::default() }, vec![]);
    assert!(lib.query(&Query::default()).items.is_empty());
    scan(&mut lib, report(&root), vec![]);
    assert!(lib.query(&Query::default()).items[0].favorite);
}
#[test]
fn unreadable_scan_marks_cached_entries_unavailable() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    let r = report(&root);
    scan(&mut lib, r.clone(), vec![]);
    std::fs::rename(&root, t.path().join("moved")).unwrap();
    std::fs::write(&root, b"no longer a directory").unwrap();
    scan(&mut lib, r, vec![]);
    assert!(lib.query(&Query::default()).items.is_empty());
}
#[test]
fn forgetting_a_manual_folder_cancels_its_pending_walk() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Manual");
    std::fs::create_dir(&root).unwrap();
    for name in ["A.wav", "B.wav", "C.wav"] {
        std::fs::write(root.join(name), b"RIFF\x04\0\0\0WAVE").unwrap();
    }
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    lib.begin_scan(ScanRequest {
        generation: 1,
        discovery: Default::default(),
        manual: vec![Root { path: root.clone(), factory: true, product: Some("Manual".into()) }],
    })
    .unwrap();
    lib.scan_step(1).unwrap();
    lib.forget(&root);
    while !lib.scan_step(64).unwrap().finished {}
    assert!(lib.query(&Query::default()).items.is_empty());
}
#[cfg(unix)]
#[test]
fn legacy_alias_favorite_migrates_and_does_not_leave_a_stale_row_after_move() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    std::fs::write(root.join("Hit.wav"), b"RIFF\x04\0\0\0WAVE").unwrap();
    let alias = t.path().join("Alias");
    std::os::unix::fs::symlink(&root, &alias).unwrap();
    let mut lib = Library::new(None, &t.path().join("db")).unwrap();
    lib.scan(&[Root { path: alias.clone(), factory: true, product: Some("Wrong".into()) }]).unwrap();
    lib.set_favorite(&alias.join("Hit.wav"), true);
    scan(&mut lib, report(&root), vec![]);
    let r = lib.query(&Query::default());
    assert_eq!(r.items.len(), 1);
    assert!(r.items[0].favorite);
    let moved = t.path().join("Moved");
    std::fs::rename(root, &moved).unwrap();
    scan(&mut lib, report(&moved), vec![]);
    assert_eq!(lib.query(&Query::default()).items.len(), 1);
    assert!(lib.query(&Query::default()).items[0].favorite);
}
#[test]
fn optional_maschine_uuid_survives_native_enrichment() {
    let t = tempfile::tempdir().unwrap();
    let ni = testing::fixture(t.path());
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    let path = root.join("Sound.mxsnd");
    testing::tagged_file(&path, "Native sound", &[], &[]);
    let db = rusqlite::Connection::open(&ni).unwrap();
    db.execute_batch("ALTER TABLE k_sound_info ADD COLUMN uuid TEXT;ALTER TABLE k_content_path ADD COLUMN upid TEXT;").unwrap();
    db.execute("UPDATE k_sound_info SET file_name=?1,uuid='reference-uuid' WHERE id=30", [path.to_string_lossy()]).unwrap();
    db.execute("UPDATE k_content_path SET path=?1,upid='product-upid' WHERE id=1", [root.to_string_lossy()]).unwrap();
    drop(db);
    let mut lib = Library::new(Some(ni), &t.path().join("db")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    let r = lib.query(&Query { kind: Kind::Sound, ..Default::default() });
    assert_eq!(r.items.len(), 1);
    assert_eq!(r.items[0].uuid.as_deref(), Some("reference-uuid"));
    assert_eq!(r.items[0].preview_subpath.as_deref(), Some(Path::new("product-upid")));
    assert!(r.items[0].native_key.is_some());
    std::fs::rename(&root, t.path().join("Offline")).unwrap();
    scan(&mut lib, report(&root), vec![]);
    assert!(
        lib.query(&Query { kind: Kind::Sound, ..Default::default() }).items.is_empty(),
        "optional NI rows must not revive unavailable native content"
    );
}
