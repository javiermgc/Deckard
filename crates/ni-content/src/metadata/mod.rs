//! Bounded metadata-only readers. Audio, archives and plug-in state are never decoded.
mod audio;
mod id3;
mod nks;
use crate::msgpack::{Mp, Reader};
pub use nks::{NksSoundInfo, read_nks_description};
use std::path::Path;
pub const READER_VERSION: u32 = 1;
pub(crate) const BLOCK: usize = 4 * 1024 * 1024;
pub(crate) const TOTAL: usize = 16 * 1024 * 1024;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContentKind {
    Project,
    Group,
    Sound,
    Instrument,
    Effect,
    Loop,
    OneShot,
}
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContentMetadata {
    pub name: Option<String>,
    pub vendor: Option<String>,
    pub author: Option<String>,
    pub tempo: Option<f64>,
    pub kind: Option<ContentKind>,
    pub uuid: Option<String>,
    pub bank: Vec<String>,
    pub types: Vec<Vec<String>>,
    pub characters: Vec<String>,
}
impl ContentMetadata {
    fn fill(&mut self, other: Self) {
        if self.name.is_none() {
            self.name = other.name;
        }
        if self.vendor.is_none() {
            self.vendor = other.vendor;
        }
        if self.author.is_none() {
            self.author = other.author;
        }
        if self.tempo.is_none() {
            self.tempo = other.tempo;
        }
        if self.kind.is_none() {
            self.kind = other.kind;
        }
        if self.uuid.is_none() {
            self.uuid = other.uuid;
        }
        if self.bank.is_empty() {
            self.bank = other.bank;
        }
        if self.types.is_empty() {
            self.types = other.types;
        }
        if self.characters.is_empty() {
            self.characters = other.characters;
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct MetadataRead {
    pub metadata: ContentMetadata,
    pub warnings: Vec<String>,
}
fn nonempty(s: String) -> Option<String> {
    (!s.is_empty()).then_some(s)
}
fn tempo(t: f64) -> Option<f64> {
    (t.is_finite() && t > 0.0).then_some(t)
}
fn from_nks(b: &[u8]) -> Result<ContentMetadata, String> {
    let info = read_nks_description(b)?;
    let mut m = ContentMetadata {
        name: nonempty(info.name),
        vendor: nonempty(info.vendor),
        author: nonempty(info.author),
        uuid: nonempty(info.uuid),
        bank: info.bankchain.into_iter().filter(|s| !s.is_empty()).collect(),
        types: info.types,
        characters: info.characters,
        ..Default::default()
    };
    m.kind = match info.device_type.as_str() {
        "INST" => Some(ContentKind::Instrument),
        "FX" => Some(ContentKind::Effect),
        "LOOP" => Some(ContentKind::Loop),
        "ONESHOT" => Some(ContentKind::OneShot),
        _ => None,
    };
    if let Mp::Map(fields) = (Reader { b, at: 0 }).value(0)? {
        m.tempo = fields.into_iter().find_map(|(k, v)| {
            if k == "tempo" {
                match v {
                    Mp::Float(f) => tempo(f),
                    Mp::Int(i) => tempo(i as f64),
                    _ => None,
                }
            } else {
                None
            }
        });
    }
    Ok(m)
}
fn from_nisound(b: &[u8]) -> Result<ContentMetadata, String> {
    let info = mxprj::sound_info::SoundInfo::parse(b).map_err(|e| e.to_string())?;
    let kind = match info.prop("\\@devicetypeflags") {
        Some("4") => Some(ContentKind::OneShot),
        Some("8") => Some(ContentKind::Loop),
        _ => None,
    };
    Ok(ContentMetadata {
        types: info.types(),
        characters: info.characters(),
        tempo: info.prop("\\@tempo").and_then(|s| s.parse().ok()).and_then(tempo),
        kind,
        name: nonempty(info.name),
        vendor: nonempty(info.vendor),
        author: nonempty(info.author),
        bank: info.bank,
        ..Default::default()
    })
}
pub fn read_content(path: &Path) -> Result<MetadataRead, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase();
    let mut result = MetadataRead::default();
    let r = match ext.as_str() {
        "wav" | "aif" | "aiff" | "aifc" | "nksf" | "nksfx" => audio::chunks(&mut file, len, &mut result),
        "mp3" => audio::mp3(&mut file, len, &mut result),
        "mxprj" | "mxgrp" | "mxsnd" | "mxinst" | "mxfx" => audio::maschine(&mut file, len, &mut result),
        _ => Ok(()),
    };
    if let Err(e) = r {
        result.warnings.push(e);
    }
    Ok(result)
}
