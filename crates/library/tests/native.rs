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
