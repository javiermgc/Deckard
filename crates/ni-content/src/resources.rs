//! Optional, bounded NI artwork metadata. Never installation evidence.
use quick_xml::{Reader, events::Event};

/// Names explicitly declared by an image resource, including its vendor-qualified alias.
/// No generic vendor stripping: similarly named products must not acquire unrelated artwork.
pub fn image_aliases(bytes: &[u8]) -> Option<Vec<String>> {
    if bytes.len() > 64 * 1024 {
        return None;
    }
    let mut reader = Reader::from_reader(bytes);
    let mut opened = false;
    let mut closed = false;
    let mut name = None;
    let mut vendor = None;
    let mut kind = None;
    loop {
        match reader.read_event().ok()? {
            Event::Start(e) if !opened && e.name().as_ref() == b"resource" => opened = true,
            Event::Start(e) if opened && !closed => {
                let tag = e.name();
                if [b"name".as_slice(), b"vendor", b"type"].contains(&tag.as_ref()) {
                    let text = reader.read_text(tag).ok()?;
                    let text = text.decode().ok()?;
                    // These fields are plain text, not nested XML.
                    if text.contains('<') {
                        return None;
                    }
                    let text = quick_xml::escape::unescape(&text).ok()?.trim().to_string();
                    let field = match tag.as_ref() {
                        b"name" => &mut name,
                        b"vendor" => &mut vendor,
                        _ => &mut kind,
                    };
                    if field.replace(text).is_some() {
                        return None;
                    }
                } else {
                    reader.read_to_end(tag).ok()?;
                }
            }
            Event::End(e) if opened && !closed && e.name().as_ref() == b"resource" => closed = true,
            Event::Decl(_) | Event::Comment(_) => {}
            Event::Text(e) if e.decode().ok()?.trim().is_empty() => {}
            Event::Eof if closed => break,
            _ => return None,
        }
    }
    if kind.as_deref() != Some("image") {
        return None;
    }
    let name = name.filter(|v| !v.is_empty())?;
    let mut aliases = vec![name.clone()];
    if let Some(vendor) = vendor.filter(|v| !v.is_empty()) {
        aliases.push(format!("{vendor}-{name}"));
    }
    Some(aliases)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_names_only_and_xml_entities() {
        assert_eq!(
            image_aliases(b"<resource><vendor>A &amp; B</vendor><name>EQ</name><type>image</type></resource>"),
            Some(vec!["EQ".into(), "A & B-EQ".into()])
        );
        for bytes in [
            b"<resource><name>EQ</name><type>database</type></resource>".as_slice(),
            b"<resource><name>EQ</name><type>image</type>",
            b"<resource><name>EQ</name><name>Other</name><type>image</type></resource>",
            b"<resource><name>EQ</name><type>image</type></resource><resource/>",
            b"<!DOCTYPE resource><resource><name>EQ</name><type>image</type></resource>",
        ] {
            assert!(image_aliases(bytes).is_none());
        }
        assert!(image_aliases(&vec![b' '; 65537]).is_none());
    }
}
