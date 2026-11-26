use crate::{browser::Browser, settings::Settings};
use deckard_library::{Kind, Query};
#[test]
fn native_prefix_preferences_round_trip() {
    let s = Settings::parse("ni_wine_prefix = /custom/Wine Prefix\nni_wine_prefix = /other/prefix\nlibrary_folder = /samples\n");
    assert_eq!(s.ni_wine_prefixes.len(), 2);
    let restored = Settings::parse(&s.to_text());
    assert_eq!(s.ni_wine_prefixes, restored.ni_wine_prefixes);
    assert_eq!(s.library_folders, restored.library_folders);
}
#[test]
fn native_index_reaches_browser_and_knob_touch_facets() {
    let t = tempfile::tempdir().unwrap();
    let root = t.path().join("Expansion");
    std::fs::create_dir(&root).unwrap();
    let mut info = mxprj::sound_info::SoundInfo::default();
    info.name = "Tagged Kit".into();
    info.set_tags(&[vec!["Kits".into(), "Percussion".into()]], &["Acoustic".into()]);
    std::fs::write(root.join("Kit.mxgrp"), mxprj::container::new_file(*b"GRP ", b"archive", "test", &info, &mut || [0; 16])).unwrap();
    let report = deckard_ni_content::DiscoveryReport {
        installations: vec![deckard_ni_content::Installation::registered("fixture", "Synthetic Expansion", Some(root), None)],
        complete_sources: vec!["fixture".into()],
        ..Default::default()
    };
    let mut browser = Browser::default();
    browser.set_content_catalog(report, vec![]);
    browser.start(None, t.path().join("index.db"), vec![]);
    browser.set_query(Query { kind: Kind::Group, ..Default::default() });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        browser.poll();
        if !browser.scanning && !browser.waiting() {
            break;
        }
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    assert_eq!(browser.results.items.len(), 1);
    assert_eq!(browser.results.products, vec!["Synthetic Expansion"]);
    assert_eq!(browser.results.types, vec![("Kits".into(), 1)]);
    assert!(browser.scan_status.starts_with("Library refreshed"));
    let mut surface = deckard_surface::Surface::new();
    let mut view = deckard_surface::View::default();
    view.browser.kind = 2;
    view.browser.attributes[0] = (browser.results.types.iter().map(|(s, _)| s.clone()).collect(), None);
    let now = std::time::Instant::now();
    surface.event(&deckard_mk3::Event::Button("BUTTON_BROWSE", true), &view, now);
    surface.event(&deckard_mk3::Event::Button("BUTTON_BROWSE", false), &view, now);
    surface.event(&deckard_mk3::Event::Button("BUTTON_CAP_5", true), &view, now);
    assert!(surface.screen_picture(&view).unwrap().1.texts.contains(&"Kits".into()));
    browser.set_query(Query { kind: Kind::Group, types: vec!["Kits".into()], ..Default::default() });
    while browser.waiting() {
        browser.poll();
        assert!(std::time::Instant::now() < deadline);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    view.browser.attributes[1] = (browser.results.types.iter().map(|(s, _)| s.clone()).collect(), None);
    surface.event(&deckard_mk3::Event::Button("BUTTON_CAP_6", true), &view, now);
    assert!(surface.screen_picture(&view).unwrap().1.texts.contains(&"Percussion".into()));
}
