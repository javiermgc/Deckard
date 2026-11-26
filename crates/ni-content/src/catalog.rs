use std::path::{Path, PathBuf};
#[derive(Debug, Clone, Default)]
pub struct DiscoveryRequest {
    pub explicit_wine_prefixes: Vec<PathBuf>,
    pub env_wine_prefix: Option<PathBuf>,
    pub yabridge_plugin_dirs: Vec<PathBuf>,
    pub home: PathBuf,
}
impl DiscoveryRequest {
    /// Collect environment and yabridgectl configuration on a worker thread.
    pub fn from_environment(explicit_wine_prefixes: Vec<PathBuf>) -> Result<Self, String> {
        let home = std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from).unwrap_or_default();
        let mut request =
            Self { explicit_wine_prefixes, env_wine_prefix: std::env::var_os("WINEPREFIX").map(PathBuf::from), home, ..Self::default() };
        #[cfg(target_os = "linux")]
        {
            let config = std::env::var_os("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| request.home.join(".config"))
                .join("yabridgectl/config.toml");
            match std::fs::read_to_string(&config) {
                Ok(text) => {
                    let value: toml::Value = toml::from_str(&text).map_err(|e| format!("{}: {e}", config.display()))?;
                    request.yabridge_plugin_dirs = value
                        .get("plugin_dirs")
                        .and_then(|v| v.as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str())
                        .map(PathBuf::from)
                        .collect();
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(format!("{}: {e}", config.display())),
            }
        }
        Ok(request)
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Availability {
    Available,
    Missing,
    Unreadable,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Installation {
    pub id: String,
    pub product_id: String,
    pub name: String,
    pub root: Option<PathBuf>,
    pub version: Option<String>,
    pub availability: Availability,
    pub source: String,
    pub image_roots: Vec<PathBuf>,
    /// Host-bundled resources, searched only after normal roots and metadata aliases.
    pub fallback_image_roots: Vec<PathBuf>,
}
impl Installation {
    pub fn registered(source: &str, name: &str, root: Option<PathBuf>, version: Option<String>) -> Self {
        let product_id = format!("ni:{}", name.trim().to_lowercase());
        let availability = match root.as_ref().map(std::fs::metadata) {
            Some(Ok(m)) if m.is_dir() => Availability::Available,
            Some(Err(e)) if e.kind() != std::io::ErrorKind::NotFound => Availability::Unreadable,
            _ => Availability::Missing,
        };
        let image_roots = root.iter().map(|r| r.join("PAResources/image")).collect();
        Self {
            id: format!("{}:{}{}", source.len(), source, product_id),
            product_id,
            name: name.trim().into(),
            root,
            version,
            availability,
            source: source.into(),
            image_roots,
            fallback_image_roots: Vec::new(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct DiscoveryIssue {
    pub source: String,
    pub message: String,
}
#[derive(Debug, Clone, Default)]
pub struct DiscoveryReport {
    /// Complete configuration scope; absent means a partial report cannot retire other providers.
    pub sources: Option<Vec<String>>,
    pub installations: Vec<Installation>,
    pub complete_sources: Vec<String>,
    pub issues: Vec<DiscoveryIssue>,
}
impl DiscoveryReport {
    pub fn issue(&mut self, source: &str, message: impl ToString) {
        self.issues.push(DiscoveryIssue { source: source.into(), message: message.to_string() });
    }
    pub fn extend(&mut self, other: Self) {
        self.installations.extend(other.installations);
        self.complete_sources.extend(other.complete_sources);
        self.issues.extend(other.issues);
    }
}
pub fn discover(request: &DiscoveryRequest) -> DiscoveryReport {
    #[cfg(windows)]
    {
        let _ = request;
        let mut report = crate::windows::discover();
        report.sources = Some(vec!["windows64".into(), "windows32".into()]);
        report
    }
    #[cfg(target_os = "macos")]
    {
        let _ = request;
        let mut report = crate::macos::discover(Path::new("/Library/Preferences"));
        report.sources = Some(vec!["macos".into()]);
        report
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        crate::wine::discover(request)
    }
}
pub fn select_installations(report: &DiscoveryReport) -> Vec<Installation> {
    let mut selected: Vec<Installation> = Vec::new();
    for i in &report.installations {
        if let Some(old) = selected.iter_mut().find(|old| old.product_id == i.product_id) {
            if old.availability != Availability::Available && i.availability == Availability::Available {
                *old = i.clone();
            }
        } else {
            selected.push(i.clone());
        }
    }
    selected
}
pub(crate) fn canonical(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
