use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

/// Whether DECKARD has a picture of the plug-in with this key.
pub fn has(key: &str) -> bool {
    PICTURES.iter().any(|(k, ..)| *k == key)
}

/// Give DECKARD's controller layer the pictures (thumbnails and logos).
pub fn give(s: &mut deckard_surface::Surface) {
    for (key, thumb, logo) in PICTURES {
        for (bytes, is_thumb) in [(*thumb, true), (*logo, false)] {
            if let Some((w, h, px)) = decode(bytes) {
                s.add_picture(key, is_thumb, w, h, px);
            }
        }
    }
}

/// The picture key for a chain slot (`s.name`): DECKARD's own built-in picture if it
/// has one, else the plug-in's thumbnail and logo from `NI Resources\image` (loaded on
/// demand into `s`).
pub fn slot_key(s: &deckard_surface::Surface, name: &str) -> Option<String> {
    refresh_surface(s);
    let key = deckard_lua_host::plugin_picture_key(name);
    if key.is_empty() {
        return None;
    }
    if has(&key) || s.has_picture(&key, true) || s.has_picture(&key, false) {
        return Some(key);
    }
    with_ni_pictures(|ni| ni.load_slot(s, name, &key)).then_some(key)
}

/// Ensure the controller's surface has the browser tile (`thumb = true`) and right-screen
/// logo (`thumb = false`) for `product` under `"artwork:<lower>"`.
pub fn ensure_browser_product(s: &deckard_surface::Surface, product: &str) {
    refresh_surface(s);
    let lower = product.trim().to_lowercase();
    if lower.is_empty() {
        return;
    }
    let key = format!("artwork:{lower}");
    if s.has_picture(&key, true) && s.has_picture(&key, false) {
        return;
    }
    with_ni_pictures(|ni| {
        ni.load_browser_product(s, &lower, &key);
    });
}

#[derive(Clone, Default)]
struct Entry {
    artwork: Option<PathBuf>,
    plugin: Option<PathBuf>,
    logo: Option<PathBuf>,
}

impl Entry {
    fn merge(&mut self, other: &Self) {
        if self.artwork.is_none() {
            self.artwork.clone_from(&other.artwork);
        }
        if self.plugin.is_none() {
            self.plugin.clone_from(&other.plugin);
        }
        if self.logo.is_none() {
            self.logo.clone_from(&other.logo);
        }
    }
}

type DecodedImage = Option<(usize, usize, Vec<[u8; 4]>)>;

#[derive(Default)]
struct NiPictures {
    revision: u64,
    by_name: HashMap<String, Entry>,
    by_key: HashMap<String, Entry>,
    decoded: HashMap<PathBuf, DecodedImage>,
}

fn ni_image_root() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("DECKARD_NI_IMAGE_DIR") {
        let p = PathBuf::from(dir);
        return p.is_dir().then_some(p);
    }
    let mut candidates =
        vec![PathBuf::from(r"C:\Users\Public\Documents\NI Resources\image"), PathBuf::from("/Users/Shared/NI Resources/image")];
    if let Some(pub_dir) = std::env::var_os("PUBLIC") {
        candidates.insert(0, PathBuf::from(pub_dir).join(r"Documents\NI Resources\image"));
    }
    candidates.into_iter().find(|p| p.is_dir())
}

fn strip_hash(name: &str) -> &str {
    if name.len() > 33 && name.is_char_boundary(name.len() - 33) {
        let (stem, suffix) = name.split_at(name.len() - 33);
        if suffix.starts_with('_') && suffix[1..].bytes().all(|b| b.is_ascii_hexdigit()) {
            return stem;
        }
    }
    name
}

fn candidate_keys(name: &str) -> Vec<String> {
    let base = deckard_lua_host::plugin_picture_key(name);
    if base.is_empty() {
        return Vec::new();
    }
    let mut out = vec![base.clone()];
    let no_fx = base.strip_suffix("fx").filter(|s| !s.is_empty()).unwrap_or(&base);
    if no_fx != base {
        out.push(no_fx.to_string());
    }
    for s in [base.as_str(), no_fx] {
        let no_digits = s.trim_end_matches(|c: char| c.is_ascii_digit());
        if !no_digits.is_empty() && !out.iter().any(|x| x == no_digits) {
            out.push(no_digits.to_string());
        }
    }
    out
}

impl NiPictures {
    fn walk(dir: &Path, depth: usize, ni: &mut Self) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        let mut dirs: Vec<PathBuf> = rd.flatten().map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for path in dirs {
            let artwork = ["MST_artwork.png", "VB_artwork.png"].into_iter().map(|f| path.join(f)).find(|p| p.is_file());
            let plugin = ["MST_plugin.png"].into_iter().map(|f| path.join(f)).find(|p| p.is_file());
            let logo = ["MST_logo.png", "VB_logo.png"].into_iter().map(|f| path.join(f)).find(|p| p.is_file());
            if artwork.is_some() || plugin.is_some() || logo.is_some() {
                if let Some(name) = path.file_name().and_then(|s| s.to_str()) {
                    let entry = Entry { artwork, plugin, logo };
                    let lower = name.trim().to_lowercase();
                    let clean = strip_hash(&lower).trim().to_string();
                    ni.by_name.entry(lower).or_default().merge(&entry);
                    ni.by_name.entry(clean.clone()).or_default().merge(&entry);
                    for k in candidate_keys(&clean) {
                        ni.by_key.entry(k).or_default().merge(&entry);
                    }
                }
            }
            if depth > 1 {
                Self::walk(&path, depth - 1, ni);
            }
        }
    }

    fn lookup(&self, name: &str) -> Option<Entry> {
        let lower = strip_hash(&name.trim().to_lowercase()).trim().to_string();
        let mut found = self.by_name.get(&lower).cloned();
        for k in candidate_keys(&lower) {
            if let Some(e) = self.by_key.get(&k) {
                match &mut found {
                    Some(cur) => cur.merge(e),
                    None => found = Some(e.clone()),
                }
            }
        }
        found
    }

    fn decode_cached(&mut self, path: &Path) -> DecodedImage {
        if let Some(cached) = self.decoded.get(path) {
            return cached.clone();
        }
        let img = std::fs::read(path).ok().and_then(|b| decode(&b));
        self.decoded.insert(path.to_path_buf(), img.clone());
        img
    }

    fn load_slot(&mut self, s: &deckard_surface::Surface, name: &str, key: &str) -> bool {
        let Some(entry) = self.lookup(name) else { return false };
        let thumb_path = entry.plugin.as_ref().or(entry.artwork.as_ref()).or(entry.logo.as_ref()).cloned();
        let logo_path = entry.logo.as_ref().or(entry.plugin.as_ref()).or(entry.artwork.as_ref()).cloned();
        let mut any = false;
        if let Some(p) = thumb_path {
            if let Some((w, h, px)) = self.decode_cached(&p) {
                s.add_picture(key, true, w, h, px);
                any = true;
            }
        }
        if let Some(p) = logo_path {
            if let Some((w, h, px)) = self.decode_cached(&p) {
                s.add_picture(key, false, w, h, px);
                any = true;
            }
        }
        any
    }

    fn load_browser_product(&mut self, s: &deckard_surface::Surface, product_lower: &str, key: &str) -> bool {
        let Some(entry) = self.lookup(product_lower) else { return false };
        let thumb_path = entry.artwork.as_ref().or(entry.plugin.as_ref()).or(entry.logo.as_ref()).cloned();
        let logo_path = entry.logo.as_ref().or(entry.artwork.as_ref()).or(entry.plugin.as_ref()).cloned();
        let mut any = false;
        if let Some(p) = thumb_path {
            if let Some((w, h, px)) = self.decode_cached(&p) {
                s.add_picture(key, true, w, h, px);
                any = true;
            }
        }
        if let Some(p) = logo_path {
            if let Some((w, h, px)) = self.decode_cached(&p) {
                s.add_picture(key, false, w, h, px);
                any = true;
            }
        }
        any
    }
}

static NI: OnceLock<Mutex<NiPictures>> = OnceLock::new();

/// Called by the catalog worker: enumerate and decode resource files off the UI thread.
pub fn set_content_catalog(revision: u64, installations: &[deckard_ni_content::Installation]) {
    let mut roots = Vec::new();
    if let Some(path) = std::env::var_os("DECKARD_NI_IMAGE_DIR") {
        roots.push(PathBuf::from(path));
    }
    for i in installations {
        if let Some(root) = &i.root {
            roots.push(root.join("PAResources/image"));
        }
    }
    for i in installations {
        roots.extend(i.image_roots.clone());
    }
    if let Some(root) = ni_image_root() {
        roots.push(root);
    }
    let mut seen = std::collections::HashSet::new();
    let mut ni = NiPictures { revision, ..Default::default() };
    for root in roots {
        if seen.insert(root.clone()) {
            NiPictures::walk(&root, 3, &mut ni);
        }
    }
    let paths: Vec<_> = ni.by_name.values().flat_map(|e| [e.artwork.clone(), e.plugin.clone(), e.logo.clone()]).flatten().collect();
    for path in paths {
        ni.decode_cached(&path);
    }
    *NI.get_or_init(|| Mutex::new(NiPictures::default())).lock().expect("ni pictures lock") = ni;
}
fn with_ni_pictures<R>(f: impl FnOnce(&mut NiPictures) -> R) -> R {
    let mut guard = NI.get_or_init(|| Mutex::new(NiPictures::default())).lock().expect("ni pictures lock");
    f(&mut guard)
}

/// A PNG as RGBA pixels (w, h, pixels).
fn decode(bytes: &[u8]) -> Option<(usize, usize, Vec<[u8; 4]>)> {
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut r = dec.read_info().ok()?;
    let mut buf = vec![0; r.output_buffer_size()?];
    let info = r.next_frame(&mut buf).ok()?;
    let buf = &buf[..info.buffer_size()];
    let px = match info.color_type {
        png::ColorType::Rgba => buf.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect(),
        png::ColorType::Rgb => buf.chunks(3).map(|c| [c[0], c[1], c[2], 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf.chunks(2).map(|c| [c[0], c[0], c[0], c[1]]).collect(),
        png::ColorType::Grayscale => buf.iter().map(|&g| [g, g, g, 255]).collect(),
        _ => return None,
    };
    Some((info.width as usize, info.height as usize, px))
}

fn refresh_surface(surface: &deckard_surface::Surface) {
    let revision = with_ni_pictures(|ni| ni.revision);
    surface.refresh_external_pictures(revision, &PICTURES.iter().map(|(key, ..)| *key).collect::<Vec<_>>());
}
