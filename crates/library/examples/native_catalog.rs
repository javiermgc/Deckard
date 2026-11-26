//! Read-only installed-content acceptance check. Writes only a temporary Deckard index.
use deckard_library::*;
fn main() {
    let request = deckard_ni_content::DiscoveryRequest::from_environment(std::env::args_os().skip(1).map(Into::into).collect()).unwrap();
    let discovery = deckard_ni_content::discover(&request);
    let t = tempfile::tempdir().unwrap();
    let mut library = Library::new(None, &t.path().join("index.db")).unwrap();
    library.begin_scan(ScanRequest { generation: 1, discovery, manual: vec![] }).unwrap();
    loop {
        let p = library.scan_step(64).unwrap();
        if p.finished {
            eprintln!("{} indexed; {} issues", p.processed, p.issues.len());
            for issue in p.issues.iter().take(10) {
                eprintln!("{issue}");
            }
            break;
        }
    }
    for kind in Kind::ALL {
        let r = library.query(&Query { kind, ..Default::default() });
        let mut products = std::collections::BTreeMap::<String, usize>::new();
        for i in r.items {
            if std::env::var_os("DECKARD_INDEX_ITEMS").is_some() {
                println!("ITEM\t{kind:?}\t{}", i.path.display());
            }
            *products.entry(i.product.unwrap_or_default()).or_default() += 1;
        }
        for (product, count) in products {
            println!("{product}\t{kind:?}\t{count}");
        }
    }
}
