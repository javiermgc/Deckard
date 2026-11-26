//! Discovery runs independently of browser opening and indexing.
use deckard_ni_content::{DiscoveryReport, DiscoveryRequest};
use std::{
    path::PathBuf,
    sync::mpsc,
    time::{Duration, Instant},
};
#[derive(Clone)]
pub struct CatalogSnapshot {
    pub revision: u64,
    pub report: DiscoveryReport,
}
#[derive(Default)]
pub struct NiLibrary {
    pub snapshot: Option<CatalogSnapshot>,
    pub status: String,
    worker: Option<(mpsc::Sender<(u64, Vec<PathBuf>)>, mpsc::Receiver<CatalogSnapshot>)>,
    requested: u64,
    last: Option<Instant>,
}
impl NiLibrary {
    pub fn refresh(&mut self, prefixes: Vec<PathBuf>, force: bool) {
        if !force && self.last.is_some_and(|t| t.elapsed() < Duration::from_secs(5)) {
            return;
        }
        if self.worker.is_none() {
            let (tx, rx) = mpsc::channel::<(u64, Vec<PathBuf>)>();
            let (done, ready) = mpsc::channel();
            std::thread::Builder::new()
                .name("ni-catalog".into())
                .spawn(move || {
                    let mut previous: DiscoveryReport = Default::default();
                    while let Ok(mut job) = rx.recv() {
                        while let Ok(next) = rx.try_recv() {
                            job = next;
                        }
                        let mut report = match DiscoveryRequest::from_environment(job.1.clone()) {
                            Ok(request) => deckard_ni_content::discover(&request),
                            Err(error) => {
                                let mut r = deckard_ni_content::discover(&DiscoveryRequest {
                                    explicit_wine_prefixes: job.1,
                                    env_wine_prefix: std::env::var_os("WINEPREFIX").map(Into::into),
                                    home: std::env::var_os("HOME").map(Into::into).unwrap_or_default(),
                                    ..Default::default()
                                });
                                r.sources = None;
                                r.issue("configuration", error);
                                r
                            }
                        };
                        for old in &previous.installations {
                            if !report.complete_sources.contains(&old.source)
                                && report.sources.as_ref().is_none_or(|sources| sources.contains(&old.source))
                                && !report.installations.iter().any(|i| i.id == old.id)
                            {
                                report.installations.push(old.clone());
                            }
                        }
                        previous = report.clone();
                        // Normalize provider ordering before publishing a revision.
                        report.complete_sources.sort();
                        report.complete_sources.dedup();
                        crate::plugin_pictures::set_content_catalog(job.0, &deckard_ni_content::select_installations(&report));
                        crate::browser::set_content_catalog(job.0, &deckard_ni_content::select_installations(&report));
                        if done.send(CatalogSnapshot { revision: job.0, report }).is_err() {
                            break;
                        }
                    }
                })
                .expect("NI catalog worker");
            self.worker = Some((tx, ready));
        }
        self.requested += 1;
        self.last = Some(Instant::now());
        self.status = "Discovering installed libraries…".into();
        let _ = self.worker.as_ref().unwrap().0.send((self.requested, prefixes));
    }
    pub fn started(&self) -> bool {
        self.worker.is_some()
    }
    pub fn pending(&self) -> bool {
        self.snapshot.as_ref().is_none_or(|s| s.revision != self.requested)
    }
    pub fn poll(&mut self) -> Option<CatalogSnapshot> {
        let (_, rx) = self.worker.as_ref()?;
        let mut latest = None;
        while let Ok(s) = rx.try_recv() {
            if s.revision == self.requested {
                latest = Some(s);
            }
        }
        let s = latest?;
        self.status = format!("{} installed libraries", deckard_ni_content::select_installations(&s.report).len());
        self.snapshot = Some(s.clone());
        Some(s)
    }
}
impl crate::App {
    pub(crate) fn refresh_native_libraries(&mut self, force: bool) {
        #[cfg(not(test))]
        self.ni_library.refresh(self.settings.ni_wine_prefixes.clone(), force);
        #[cfg(test)]
        let _ = force; // GUI fixtures must never discover/write the real user's library.
    }
    pub(crate) fn poll_native_libraries(&mut self, ctx: &eframe::egui::Context) {
        #[cfg(test)]
        if !self.ni_library.started() {
            return;
        }
        if !self.ni_library.started() {
            self.refresh_native_libraries(false);
        }
        if let Some(snapshot) = self.ni_library.poll() {
            let roots = deckard_ni_content::select_installations(&snapshot.report)
                .into_iter()
                .filter(|i| i.availability == deckard_ni_content::Availability::Available)
                .filter_map(|i| i.root)
                .collect::<Vec<_>>();
            self.samples.set_installed_content(snapshot.revision, &roots);
            self.remember_libraries();
            let manual = self.library_roots();
            self.browser.set_content_catalog(snapshot.report, manual.clone());
            if !self.browser.started()
                && let Some(own) = crate::menu::settings_dir().map(|d| d.join("library.db3"))
            {
                self.browser.start(deckard_library::ni::default_path(), own, manual);
            }
            // A project may have loaded before discovery. Resolve/decode its samples off-thread.
            if let Some(doc) = &self.doc {
                let revision = doc.borrow().revision();
                let groups = doc.borrow().project().groups.clone();
                let path = self.path.clone();
                let known = self.samples.known_sample_state();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let samples =
                        groups.iter().map(|g| deckard_engine::SampleCache::preload_group_samples(g, path.as_deref(), &known)).collect();
                    let _ = tx.send(samples);
                });
                self.ni_sample_retry = Some((doc.clone(), revision, rx));
            }
            ctx.request_repaint();
        }
        if let Some((doc, revision, rx)) = &self.ni_sample_retry {
            match rx.try_recv() {
                Ok(samples) => {
                    if self.doc.as_ref().is_some_and(|current| std::rc::Rc::ptr_eq(current, doc)) && doc.borrow().revision() == *revision {
                        for samples in samples {
                            self.samples.ingest_preloaded(samples);
                        }
                        self.seen_revision = self.seen_revision.wrapping_sub(1);
                        self.built = None;
                        self.waveform = None;
                    }
                    self.ni_sample_retry = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => self.ni_sample_retry = None,
                Err(_) => {}
            }
        }
        if self.ni_library.pending() || self.ni_sample_retry.is_some() || self.browser.scanning {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
    }
}
