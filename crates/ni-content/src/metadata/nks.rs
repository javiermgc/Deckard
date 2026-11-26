use crate::msgpack::{Mp, Reader};
/// A preset's description (`NISI`): its name, where it's filed and its tags.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct NksSoundInfo {
    pub author: String,
    /// Product, bank and sub-bank (three levels, empty where there's none).
    pub bankchain: Vec<String>,
    pub characters: Vec<String>,
    pub comment: String,
    /// `"INST"` or `"FX"`.
    pub device_type: String,
    /// Modes (a preset's playing styles, e.g. "Arp"; most have none).
    pub modes: Vec<String>,
    pub name: String,
    /// Each a type and its sub-type.
    pub types: Vec<Vec<String>>,
    /// Lower-case, hyphenated (Maschine gives each preset it saves a random one).
    pub uuid: String,
    pub vendor: String,
}

pub fn read_nks_description(b: &[u8]) -> Result<NksSoundInfo, String> {
    let Mp::Map(fields) = (Reader { b, at: 0 }).value(0)? else { return Err("NKS description is not a map".into()) };
    let text = |v: &Mp| match v {
        Mp::Str(s) => s.clone(),
        _ => String::new(),
    };
    let texts = |v: &Mp| match v {
        Mp::List(l) => l.iter().map(text).collect(),
        _ => Vec::new(),
    };
    let mut info = NksSoundInfo::default();
    for (key, v) in &fields {
        match key.as_str() {
            "author" => info.author = text(v),
            "bankchain" => info.bankchain = texts(v),
            "characters" => info.characters = texts(v),
            "comment" => info.comment = text(v),
            "deviceType" => info.device_type = text(v),
            "modes" => info.modes = texts(v),
            "name" => info.name = text(v),
            "types" => info.types = if let Mp::List(l) = v { l.iter().map(texts).collect() } else { Vec::new() },
            "uuid" => info.uuid = text(v),
            "vendor" => info.vendor = text(v),
            _ => {}
        }
    }
    Ok(info)
}
