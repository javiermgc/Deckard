use crate::{DiscoveryReport, Installation, paths::hfs_to_posix};
use std::{io::Cursor, path::Path};
pub fn read_plist(name: &str, bytes: &[u8]) -> Result<Option<Installation>, String> {
    let value = plist::Value::from_reader(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    let Some(dict) = value.as_dictionary() else { return Err("product plist is not a dictionary".into()) };
    let Some(dir) = dict.get("ContentDir").and_then(|v| v.as_string()).filter(|s| !s.is_empty()) else { return Ok(None) };
    let root = hfs_to_posix(dir);
    if !root.is_absolute() {
        return Err(format!("relative ContentDir: {dir}"));
    }
    Ok(Some(Installation::registered("macos", name, Some(root), dict.get("ContentVersion").and_then(|v| v.as_string()).map(str::to_owned))))
}
pub fn discover(preferences: &Path) -> DiscoveryReport {
    let mut report = DiscoveryReport::default();
    let mut complete = true;
    match std::fs::read_dir(preferences) {
        Ok(entries) => {
            for entry in entries {
                let entry = match entry {
                    Ok(e) => e,
                    Err(e) => {
                        report.issue("macos", e);
                        complete = false;
                        continue;
                    }
                };
                let file = entry.file_name();
                let file = file.to_string_lossy();
                let Some(name) = file.strip_prefix("com.native-instruments.").and_then(|s| s.strip_suffix(".plist")) else { continue };
                match std::fs::read(entry.path()).map_err(|e| e.to_string()).and_then(|b| read_plist(name, &b)) {
                    Ok(Some(mut i)) => {
                        i.image_roots.push("/Users/Shared/NI Resources/image".into());
                        report.installations.push(i);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        complete = false;
                        report.issue("macos", format!("{file}: {e}"));
                    }
                }
            }
        }
        Err(e) => {
            complete = false;
            report.issue("macos", e);
        }
    }
    if complete {
        report.complete_sources.push("macos".into());
    }
    report.installations.sort_by(|a, b| a.id.cmp(&b.id));
    report
}
