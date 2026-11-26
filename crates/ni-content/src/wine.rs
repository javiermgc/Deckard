use crate::{catalog::canonical, *};
use std::path::{Path, PathBuf};
/// Wine's quoted registry strings, not JSON strings (including hexadecimal escapes).
fn unescape(s: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('\\') => out.push('\\'),
            Some('"') => out.push('"'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('x') => {
                let mut hex = String::new();
                for _ in 0..4 {
                    if chars.peek().is_some_and(|c| c.is_ascii_hexdigit()) {
                        hex.push(chars.next().unwrap());
                    } else {
                        break;
                    }
                }
                let n = u32::from_str_radix(&hex, 16).map_err(|_| "invalid registry escape")?;
                out.push(char::from_u32(n).ok_or("invalid registry character")?);
            }
            _ => return Err("invalid registry escape".into()),
        }
    }
    Ok(out)
}
pub fn prefixes(request: &DiscoveryRequest) -> Vec<PathBuf> {
    let mut paths = request.explicit_wine_prefixes.clone();
    paths.extend(request.env_wine_prefix.clone());
    let mut inferred = Vec::new();
    for dir in &request.yabridge_plugin_dirs {
        for parent in dir.ancestors() {
            if parent.join("system.reg").is_file() && parent.join("dosdevices").is_dir() {
                inferred.push(parent.to_path_buf());
                break;
            }
        }
    }
    inferred.sort();
    paths.extend(inferred);
    if !request.home.as_os_str().is_empty() && request.home.join(".wine").exists() {
        paths.push(request.home.join(".wine"));
    }
    let mut seen = std::collections::HashSet::new();
    paths.into_iter().map(|p| canonical(&p)).filter(|p| seen.insert(p.clone())).collect()
}
pub fn discover(request: &DiscoveryRequest) -> DiscoveryReport {
    let mut report = DiscoveryReport::default();
    let prefixes = prefixes(request);
    report.sources = Some(prefixes.iter().map(|p| format!("wine:{}", canonical(p).display())).collect());
    for prefix in prefixes {
        report.extend(read_prefix(&prefix));
    }
    report
}
pub fn read_prefix(prefix: &Path) -> DiscoveryReport {
    let source = format!("wine:{}", canonical(prefix).display());
    let mut report = DiscoveryReport::default();
    let text = match std::fs::read_to_string(prefix.join("system.reg")) {
        Ok(t) => t,
        Err(e) => {
            report.issue(&source, e);
            return report;
        }
    };
    if !text.starts_with("WINE REGISTRY Version 2") {
        report.issue(&source, "invalid Wine registry header");
        return report;
    }
    let mut entries: Vec<(String, Option<String>, Option<String>)> = Vec::new();
    let mut current = None;
    let mut complete = true;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            current = None;
            let key = line
                .strip_prefix('[')
                .and_then(|s| s.split_once(']').map(|(s, _)| s))
                .ok_or_else(|| "unterminated registry section".to_string())
                .and_then(unescape);
            match key {
                Ok(key) => {
                    let parts: Vec<_> = key.split('\\').collect();
                    let lower = key.to_lowercase();
                    let valid = (lower.starts_with("software\\native instruments\\") && parts.len() == 3)
                        || (lower.starts_with("software\\wow6432node\\native instruments\\") && parts.len() == 4);
                    if valid {
                        entries.push((parts.last().unwrap().to_string(), None, None));
                        current = Some(entries.len() - 1);
                    }
                }
                Err(e) => {
                    complete = false;
                    report.issue(&source, e);
                }
            }
        } else if let Some(i) = current {
            for (field, target) in [("\"ContentDir\"=", 1), ("\"ContentVersion\"=", 2)] {
                if let Some(value) = line.strip_prefix(field) {
                    let value = value
                        .strip_prefix('"')
                        .and_then(|v| v.strip_suffix('"'))
                        .ok_or_else(|| "non-string registry content value".to_string())
                        .and_then(unescape);
                    match value {
                        Ok(v) => {
                            if target == 1 {
                                entries[i].1 = Some(v);
                            } else {
                                entries[i].2 = Some(v);
                            }
                        }
                        Err(e) => {
                            complete = false;
                            report.issue(&source, e);
                        }
                    }
                }
            }
        }
    }
    // Resolve with this prefix's drive mappings and case rules, never the host OS paths.
    let fallback_roots: Vec<_> = [
        r"C:\Program Files\Common Files\Native Instruments\Komplete Kontrol\PAResources\image",
        r"C:\Program Files (x86)\Common Files\Native Instruments\Komplete Kontrol\PAResources\image",
    ]
    .into_iter()
    .filter_map(|path| wine_to_host(prefix, path).ok())
    .collect();
    for (name, dir, version) in entries {
        let Some(dir) = dir.filter(|s| !s.is_empty()) else { continue };
        let root = match wine_to_host(prefix, &dir) {
            Ok(p) => Some(p),
            Err(e) => {
                report.issue(&source, format!("{name}: {e}"));
                None
            }
        };
        let mut i = Installation::registered(&source, &name, root, version);
        if let Ok(shared) = wine_to_host(prefix, "C:\\Users\\Public\\Documents\\NI Resources\\image") {
            i.image_roots.push(shared);
        }
        i.fallback_image_roots = fallback_roots.clone();
        report.installations.push(i);
    }
    if complete {
        report.complete_sources.push(source);
    }
    report
}
