use crate::{native_index as index, *};
use deckard_ni_content::{
    Availability, DiscoveryReport, Installation,
    metadata::{self, ContentKind},
};
use std::{
    collections::{HashSet, VecDeque},
    fs::ReadDir,
    path::{Path, PathBuf},
};
#[derive(Debug, Clone)]
pub struct ScanRequest {
    pub generation: u64,
    pub discovery: DiscoveryReport,
    pub manual: Vec<user::Root>,
}
#[derive(Debug, Clone, Default)]
pub struct ScanProgress {
    pub generation: u64,
    pub processed: usize,
    pub finished: bool,
    pub issues: Vec<String>,
    pub product_status: std::collections::BTreeMap<String, String>,
}
enum Root {
    Native(Installation),
    Manual(user::Root),
}
impl Root {
    fn path(&self) -> &Path {
        match self {
            Self::Native(i) => i.root.as_deref().unwrap(),
            Self::Manual(r) => &r.path,
        }
    }
}
struct Walk {
    root: Root,
    stack: Vec<ReadDir>,
    visited: HashSet<PathBuf>,
    seen: HashSet<String>,
    failed: bool,
    started_count: usize,
}
pub(crate) struct Scanner {
    roots: VecDeque<Root>,
    walk: Option<Walk>,
    progress: ScanProgress,
    owned: Vec<PathBuf>,
    known: std::collections::HashMap<String, i64>,
}
impl Library {
    pub fn begin_scan(&mut self, request: ScanRequest) -> Result<(), String> {
        self.cancel_scan()?;
        let installations = index::catalog(&self.own.db, &request.discovery)?;
        let mut roots: VecDeque<_> =
            installations.into_iter().filter(|i| i.availability == Availability::Available && i.root.is_some()).map(Root::Native).collect();
        roots.extend(request.manual.into_iter().map(Root::Manual));
        self.canonical_paths.clear();
        let owned = index::owned_roots(&self.own.db).into_iter().map(|p| p.canonicalize().unwrap_or(p)).collect();
        let known = self.own.known_mtimes()?;
        self.scan = Some(Scanner {
            roots,
            walk: None,
            owned,
            known,
            progress: ScanProgress {
                generation: request.generation,
                issues: request.discovery.issues.iter().map(|e| format!("{}: {}", e.source, e.message)).collect(),
                ..Default::default()
            },
        });
        Ok(())
    }
    pub fn cancel_scan(&mut self) -> Result<(), String> {
        self.scan = None;
        self.own.db.execute("DELETE FROM native_staging", []).map(|_| ()).map_err(|e| e.to_string())
    }
    pub fn scan_step(&mut self, max_files: usize) -> Result<ScanProgress, String> {
        let Some(mut scan) = self.scan.take() else { return Ok(ScanProgress { finished: true, ..Default::default() }) };
        self.own.db.execute_batch("BEGIN").map_err(|e| e.to_string())?;
        let result = scan.step(self, max_files.max(1));
        match &result {
            Ok(()) => self.own.db.execute_batch("COMMIT").map_err(|e| e.to_string())?,
            Err(_) => {
                let _ = self.own.db.execute_batch("ROLLBACK");
            }
        }
        let progress = scan.progress.clone();
        if result.is_ok() && !progress.finished {
            self.scan = Some(scan);
        }
        result.map(|_| progress)
    }
}
impl Scanner {
    fn step(&mut self, lib: &mut Library, budget: usize) -> Result<(), String> {
        // Budget includes directory entries, not only supported files; empty trees yield too.
        let started = std::time::Instant::now();
        for step in 0..budget {
            if step > 0 && started.elapsed() > std::time::Duration::from_millis(20) {
                break;
            }
            if self.walk.is_none() {
                let Some(root) = self.roots.pop_front() else {
                    self.progress.finished = true;
                    return Ok(());
                };
                match std::fs::read_dir(root.path()) {
                    Ok(rd) => {
                        let canonical = root.path().canonicalize().unwrap_or_else(|_| root.path().to_path_buf());
                        if let Root::Native(i) = &root {
                            self.progress.product_status.insert(i.id.clone(), "Indexing…".into());
                        }
                        self.walk = Some(Walk {
                            root,
                            stack: vec![rd],
                            visited: HashSet::from([canonical]),
                            seen: HashSet::new(),
                            failed: false,
                            started_count: self.progress.processed,
                        });
                    }
                    Err(e) => {
                        self.progress.issues.push(format!("{}: {e}", root.path().display()));
                        if let Root::Native(i) = &root {
                            lib.own
                                .db
                                .execute("UPDATE native_installations SET available=0 WHERE id=?1", [&i.id])
                                .map_err(|e| e.to_string())?;
                            self.progress.product_status.insert(i.id.clone(), "Unreadable — cached index retained".into());
                        }
                        continue;
                    }
                }
            }
            let walk = self.walk.as_mut().unwrap();
            let Some(rd) = walk.stack.last_mut() else {
                let walk = self.walk.take().unwrap();
                if let Root::Native(i) = &walk.root {
                    let status = if walk.failed {
                        "Scan failed — cached index retained".into()
                    } else {
                        let n = self.progress.processed - walk.started_count;
                        if n == 0 { "No supported content".into() } else { format!("Indexed ({n} files)") }
                    };
                    self.progress.product_status.insert(i.id.clone(), status.clone());
                    lib.own
                        .db
                        .execute(
                            "INSERT OR REPLACE INTO native_scan_status VALUES(?1,?2,?3)",
                            rusqlite::params![i.product_id, status, (self.progress.processed - walk.started_count) as i64],
                        )
                        .map_err(|e| e.to_string())?;
                    if walk.failed {
                        lib.own
                            .db
                            .execute("UPDATE native_installations SET available=0 WHERE id=?1", [&i.id])
                            .map_err(|e| e.to_string())?;
                    }
                }
                if !walk.failed {
                    match walk.root {
                        Root::Native(i) => index::publish(&lib.own.db, &i.product_id)?,
                        Root::Manual(root) => {
                            let mut st = lib.own.db.prepare("SELECT path FROM items").map_err(|e| e.to_string())?;
                            let gone: Vec<String> = st
                                .query_map([], |r| r.get::<_, String>(0))
                                .map_err(|e| e.to_string())?
                                .flatten()
                                .filter(|p| {
                                    Path::new(p).starts_with(&root.path)
                                        && !walk.seen.contains(p)
                                        && !self
                                            .owned
                                            .iter()
                                            .any(|r| Path::new(p).canonicalize().unwrap_or_else(|_| PathBuf::from(p)).starts_with(r))
                                })
                                .collect();
                            drop(st);
                            for path in gone {
                                lib.own.db.execute("DELETE FROM items WHERE path=?1", [path]).map_err(|e| e.to_string())?;
                            }
                        }
                    }
                }
                continue;
            };
            let Some(entry) = rd.next() else {
                walk.stack.pop();
                continue;
            };
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    walk.failed = true;
                    self.progress.issues.push(e.to_string());
                    continue;
                }
            };
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            let path = entry.path();
            if entry.file_type().is_ok_and(|t| t.is_file()) && kind(&path).is_none() {
                continue;
            }
            let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
            if matches!(walk.root, Root::Manual(_)) && self.owned.iter().any(|r| canonical.starts_with(r)) {
                continue;
            }
            let meta = match std::fs::metadata(&path) {
                Ok(m) => m,
                Err(e) => {
                    walk.failed = true;
                    self.progress.issues.push(format!("{}: {e}", path.display()));
                    continue;
                }
            };
            if meta.is_dir() {
                let canonical = match path.canonicalize() {
                    Ok(p) => p,
                    Err(e) => {
                        walk.failed = true;
                        self.progress.issues.push(e.to_string());
                        continue;
                    }
                };
                if walk.visited.insert(canonical) {
                    match std::fs::read_dir(&path) {
                        Ok(rd) => walk.stack.push(rd),
                        Err(e) => {
                            walk.failed = true;
                            self.progress.issues.push(format!("{}: {e}", path.display()));
                        }
                    }
                }
                continue;
            }
            if !meta.is_file() {
                continue;
            }
            match &walk.root {
                Root::Manual(root) => {
                    if Kind::of_file(&path).is_some()
                        || path
                            .extension()
                            .and_then(|s| s.to_str())
                            .is_some_and(|s| ["nksf", "nksfx"].iter().any(|e| s.eq_ignore_ascii_case(e)))
                    {
                        if self.known.get(&path.to_string_lossy().into_owned()) != Some(&user::mtime(&path)) {
                            lib.own.add_file(&path, root)?;
                        }
                        walk.seen.insert(path.to_string_lossy().into_owned());
                        self.progress.processed += 1;
                    }
                }
                Root::Native(i) => {
                    let Some(kind) = kind(&path) else { continue };
                    let relative =
                        path.strip_prefix(i.root.as_ref().unwrap()).map_err(|e| e.to_string())?.to_string_lossy().replace('\\', "/");
                    let key = index::key(&i.product_id, &relative);
                    let fingerprint = format!(
                        "{}:{}:{:?}:{}:{}",
                        metadata::READER_VERSION,
                        meta.len(),
                        meta.modified().ok(),
                        i.root.as_ref().unwrap().display(),
                        i.version.as_deref().unwrap_or("")
                    );
                    let mut item = if let Some(item) = index::cached(&lib.own.db, &key, &fingerprint)? {
                        item
                    } else {
                        let mut item = Item {
                            kind,
                            path: path.clone(),
                            name: path.file_stem().unwrap_or_default().to_string_lossy().into_owned(),
                            product: Some(i.name.clone()),
                            native_key: Some(key),
                            ..Default::default()
                        };
                        item.tempo = names::tempo_in_name(&item.name);
                        match metadata::read_content(&path) {
                            Ok(read) => {
                                for warning in read.warnings {
                                    self.progress.issues.push(format!("{}: {warning}", path.display()));
                                }
                                let m = read.metadata;
                                if let Some(name) = m.name {
                                    item.name = name;
                                }
                                if let Some(v) = m.vendor {
                                    item.vendor = v;
                                }
                                if let Some(v) = m.author {
                                    item.author = v;
                                }
                                item.bank = m.bank;
                                item.types = m.types;
                                item.characters = m.characters;
                                item.uuid = m.uuid;
                                if let Some(v) = m.tempo {
                                    item.tempo = Some(v);
                                }
                                if let Some(v) = m.kind {
                                    item.kind = match v {
                                        ContentKind::Project => Kind::Project,
                                        ContentKind::Group => Kind::Group,
                                        ContentKind::Sound => Kind::Sound,
                                        ContentKind::Instrument => Kind::Instrument,
                                        ContentKind::Effect => Kind::Effect,
                                        ContentKind::Loop => Kind::Loop,
                                        ContentKind::OneShot => Kind::OneShot,
                                    };
                                }
                            }
                            Err(e) => {
                                walk.failed = true;
                                self.progress.issues.push(e);
                            }
                        }
                        item
                    };
                    item.path = path;
                    index::stage(&lib.own.db, &i.product_id, &fingerprint, &item)?;
                    self.progress.processed += 1;
                }
            }
        }
        Ok(())
    }
}
fn kind(path: &Path) -> Option<Kind> {
    Kind::of_file(path).or_else(|| match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "nksf" | "nki" | "nksn" | "nmsv" | "nfm8" | "nrkt" | "ens" | "rkplr" => Some(Kind::Instrument),
        "nksfx" => Some(Kind::Effect),
        _ => None,
    })
}
