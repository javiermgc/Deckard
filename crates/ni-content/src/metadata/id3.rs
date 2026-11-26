use super::*;
pub(super) fn sync(b: &[u8]) -> Result<usize, String> {
    if b.len() != 4 || b.iter().any(|b| b & 128 != 0) {
        return Err("invalid ID3 syncsafe size".into());
    }
    Ok(b.iter().fold(0, |v, b| (v << 7) | *b as usize))
}
fn deunsync(b: &[u8]) -> Vec<u8> {
    let mut v = Vec::with_capacity(b.len());
    let mut prev = false;
    for &c in b {
        if prev && c == 0 {
            prev = false;
            continue;
        }
        v.push(c);
        prev = c == 255;
    }
    v
}
fn terminated(b: &[u8], wide: bool) -> Result<(&[u8], &[u8]), String> {
    let n = if wide { b.chunks_exact(2).position(|c| c == [0, 0]).map(|n| n * 2) } else { b.iter().position(|c| *c == 0) }
        .ok_or("unterminated ID3 text")?;
    Ok((&b[..n], &b[n + if wide { 2 } else { 1 }..]))
}
fn text(b: &[u8], enc: u8) -> String {
    match enc {
        1 | 2 => {
            let le = enc == 1 && b.starts_with(&[255, 254]);
            let b = if enc == 1 && (b.starts_with(&[255, 254]) || b.starts_with(&[254, 255])) { &b[2..] } else { b };
            String::from_utf16_lossy(
                &b.chunks_exact(2)
                    .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
                    .collect::<Vec<_>>(),
            )
        }
        _ => String::from_utf8_lossy(b).into_owned(),
    }
}
pub(super) fn parse(b: &[u8], result: &mut MetadataRead) -> Result<(), String> {
    if b.len() < 10 || &b[..3] != b"ID3" {
        return Err("invalid ID3 header".into());
    }
    let version = b[3];
    if ![3, 4].contains(&version) {
        return Err(format!("unsupported ID3 version {version}"));
    }
    let n = sync(&b[6..10])?;
    let body = b.get(10..10 + n).ok_or("truncated ID3 tag")?;
    let decoded;
    if version == 3 && b[5] & 128 != 0 {
        decoded = deunsync(body);
    } else {
        decoded = body.to_vec();
    }
    let body = &decoded[..];
    let mut at = 0;
    if b[5] & 64 != 0 {
        let head = body.get(..4).ok_or("truncated ID3 extended header")?;
        at = if version == 4 { sync(head)? } else { 4 + u32::from_be_bytes(head.try_into().unwrap()) as usize };
        if at < 4 || at > body.len() {
            return Err("invalid ID3 extended header size".into());
        }
    }
    let mut primary = ContentMetadata::default();
    let mut fallback = ContentMetadata::default();
    while at < body.len() {
        let Some(h) = body.get(at..at + 10) else {
            if body[at..].iter().all(|c| *c == 0) {
                break;
            }
            return Err("truncated ID3 frame header".into());
        };
        if h[..4] == [0, 0, 0, 0] {
            break;
        }
        let size = if version == 4 { sync(&h[4..8])? } else { u32::from_be_bytes(h[4..8].try_into().unwrap()) as usize };
        at += 10;
        let frame = body.get(at..at + size).ok_or("truncated ID3 frame")?;
        at += size;
        if &h[..4] != b"GEOB" {
            continue;
        }
        let parsed = (|| {
            let flags = h[9];
            if (version == 3 && flags & 0xe0 != 0) || (version == 4 && flags & 0x4d != 0) {
                return Err("unsupported compressed/encrypted/grouped ID3 metadata".into());
            }
            let unsynced;
            let frame = if version == 4 && (flags & 2 != 0 || b[5] & 128 != 0) {
                unsynced = deunsync(frame);
                &unsynced[..]
            } else {
                frame
            };
            let (&enc, rest) = frame.split_first().ok_or("empty GEOB")?;
            if enc > 3 {
                return Err("invalid GEOB encoding".into());
            }
            let (_, rest) = terminated(rest, false)?;
            let (_, rest) = terminated(rest, enc == 1 || enc == 2)?;
            let (desc, data) = terminated(rest, enc == 1 || enc == 2)?;
            match text(desc, enc).as_str() {
                "com.native-instruments.nks.soundinfo" => primary.fill(from_nks(data)?),
                "com.native-instruments.nisound.soundinfo" => fallback.fill(from_nisound(data)?),
                _ => {}
            }
            Ok::<_, String>(())
        })();
        if let Err(e) = parsed {
            result.warnings.push(e);
        }
    }
    primary.fill(fallback);
    result.metadata.fill(primary);
    Ok(())
}
