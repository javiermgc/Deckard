use deckard_ni_content::{macos::read_plist, windows::Registration, *};
use std::path::PathBuf;
#[test]
fn native_registry_views_merge_and_missing_is_not_uninstalled() {
    let t = tempfile::tempdir().unwrap();
    let rows = vec![
        Registration { name: "Rhythm Source".into(), content_dir: Some(t.path().into()), version: None },
        Registration { name: "Rhythm Source".into(), content_dir: Some(t.path().into()), version: None },
        Registration { name: "Executable".into(), content_dir: None, version: None },
    ];
    let report = windows::from_registrations("windows", rows);
    assert_eq!(select_installations(&report).len(), 1);
    assert_eq!(report.installations[0].name, "Rhythm Source");
    let report = windows::from_registrations(
        "windows",
        vec![Registration { name: "Absent".into(), content_dir: Some(t.path().join("absent")), version: None }],
    );
    assert_eq!(report.installations[0].availability, Availability::Missing);
    assert_eq!(report.complete_sources, vec!["windows"]);
}
#[test]
fn binary_and_xml_plists_agree_and_external_hfs_is_not_startup() {
    let mut d = plist::Dictionary::new();
    d.insert("ContentDir".into(), plist::Value::String("External:Libraries:Piano:".into()));
    let v = plist::Value::Dictionary(d);
    let mut xml = Vec::new();
    let mut binary = Vec::new();
    v.to_writer_xml(&mut xml).unwrap();
    v.to_writer_binary(&mut binary).unwrap();
    let a = read_plist("Piano", &xml).unwrap().unwrap();
    let b = read_plist("Piano", &binary).unwrap().unwrap();
    assert_eq!(a, b);
    assert_eq!(a.root, Some(PathBuf::from("/Volumes/External/Libraries/Piano")));
    assert!(read_plist("bad", b"broken").is_err());
    assert!(read_plist("catalog", b"<?xml version=\"1.0\"?><plist version=\"1.0\"><dict/></plist>").unwrap().is_none());
}
#[cfg(unix)]
#[test]
fn wine_custom_mapping_precedence_and_case_resolution() {
    use std::os::unix::fs::symlink;
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("prefix");
    std::fs::create_dir_all(p.join("dosdevices")).unwrap();
    let content = t.path().join("mapped");
    std::fs::create_dir_all(content.join("Rhéthm Library")).unwrap();
    symlink(&content, p.join("dosdevices/z:")).unwrap();
    std::fs::write(
        p.join("system.reg"),
        r#"WINE REGISTRY Version 2
[Software\\Native Instruments\\Rhythm Source] 1
"ContentDir"="Z:\\Rhéthm Library"
"ContentVersion"="1.0"
[Software\\Native Instruments\\Only Plugin] 1
"InstallDir"="C:\\Plugins"
"#,
    )
    .unwrap();
    let request = DiscoveryRequest {
        explicit_wine_prefixes: vec![p.clone()],
        env_wine_prefix: Some(p.clone()),
        home: t.path().join("home"),
        ..Default::default()
    };
    let r = wine::discover(&request);
    assert_eq!(r.installations.len(), 1, "{r:?}");
    assert_eq!(r.installations[0].root, Some(content.canonicalize().unwrap().join("Rhéthm Library")));
    assert_eq!(wine_to_host(&p, "z:\\rhéthm library").unwrap(), content.canonicalize().unwrap().join("Rhéthm Library"));
    assert!(wine_to_host(&p, "Q:\\unknown").is_err());
    assert!(wine_to_host(&p, "relative").is_err());
    std::fs::create_dir(content.join("RHÉTHM LIBRARY")).unwrap();
    assert!(wine_to_host(&p, "z:\\rhéthm library").is_err());
    assert_eq!(wine_to_host(&p, "z:\\Rhéthm Library").unwrap(), content.canonicalize().unwrap().join("Rhéthm Library"));
}
#[test]
fn missing_prefix_is_failed_not_complete() {
    let t = tempfile::tempdir().unwrap();
    let r =
        discover(&DiscoveryRequest { explicit_wine_prefixes: vec![t.path().join("absent")], home: t.path().into(), ..Default::default() });
    #[cfg(target_os = "linux")]
    {
        assert!(r.complete_sources.is_empty());
        assert!(!r.issues.is_empty());
    }
}
#[cfg(unix)]
#[test]
fn wine_prefix_precedence_yabridge_and_symlink_dedup() {
    let t = tempfile::tempdir().unwrap();
    let a = t.path().join("a");
    let b = t.path().join("b");
    for p in [&a, &b] {
        std::fs::create_dir_all(p.join("dosdevices")).unwrap();
        std::fs::write(p.join("system.reg"), "WINE REGISTRY Version 2").unwrap();
    }
    let alias = t.path().join("alias");
    std::os::unix::fs::symlink(&a, &alias).unwrap();
    let r = DiscoveryRequest {
        explicit_wine_prefixes: vec![alias],
        env_wine_prefix: Some(a.clone()),
        yabridge_plugin_dirs: vec![b.join("drive_c/Program Files/VST3")],
        home: t.path().into(),
    };
    assert_eq!(wine::prefixes(&r), vec![a.canonicalize().unwrap(), b.canonicalize().unwrap()]);
}
#[test]
fn same_product_copies_select_first_available() {
    let t = tempfile::tempdir().unwrap();
    let a = Installation::registered("a", "Piano", Some(t.path().join("gone")), None);
    let b = Installation::registered("b", "Piano", Some(t.path().into()), None);
    let r = DiscoveryReport { installations: vec![a, b.clone()], ..Default::default() };
    assert_eq!(select_installations(&r), vec![b]);
}
