use super::*;
use std::io::{Read, Seek, SeekFrom};
fn bytes<R: Read + Seek>(r: &mut R, at: u64, n: usize) -> Result<Vec<u8>, String> {
    r.seek(SeekFrom::Start(at)).map_err(|e| e.to_string())?;
    let mut b = vec![0; n];
    r.read_exact(&mut b).map_err(|e| e.to_string())?;
    Ok(b)
}
pub(super) fn chunks<R: Read + Seek>(r: &mut R, len: u64, out: &mut MetadataRead) -> Result<(), String> {
    let h = bytes(r, 0, 12)?;
    let big = &h[..4] == b"FORM";
    if !big && &h[..4] != b"RIFF" {
        return Err("not a RIFF/AIFF container".into());
    }
    let number = |b: &[u8]| if big { u32::from_be_bytes(b.try_into().unwrap()) } else { u32::from_le_bytes(b.try_into().unwrap()) };
    let end = 8 + number(&h[4..8]) as u64;
    if end > len || end < 12 {
        return Err("truncated container".into());
    }
    let mut at = 12;
    let mut total = 0;
    let mut count = 0;
    while at < end {
        count += 1;
        if count > 100000 {
            return Err("container chunk limit".into());
        }
        if end - at < 8 {
            return Err("truncated chunk header".into());
        }
        let h = bytes(r, at, 8)?;
        let n = number(&h[4..]) as usize;
        at += 8;
        let next = at.checked_add(n as u64).ok_or("chunk overflow")?;
        if next > end {
            return Err("truncated chunk".into());
        }
        if h[..4].eq_ignore_ascii_case(b"ID3 ") || &h[..4] == b"NISI" {
            if n > BLOCK || total + n > TOTAL {
                out.warnings.push("metadata size limit".into());
            } else {
                total += n;
                let b = bytes(r, at, n)?;
                let result = if &h[..4] == b"NISI" {
                    b.get(4..).ok_or_else(|| "truncated NISI".into()).and_then(from_nks).map(|m| out.metadata.fill(m))
                } else {
                    id3::parse(&b, out)
                };
                if let Err(e) = result {
                    out.warnings.push(e);
                }
            }
        }
        at = next + (n & 1) as u64;
    }
    Ok(())
}
pub(super) fn mp3<R: Read + Seek>(r: &mut R, len: u64, out: &mut MetadataRead) -> Result<(), String> {
    if len < 10 {
        return Ok(());
    }
    let h = bytes(r, 0, 10)?;
    if &h[..3] != b"ID3" {
        return Ok(());
    }
    let n = id3::sync(&h[6..10])? + 10;
    if n > BLOCK {
        return Err("metadata size limit".into());
    }
    if n as u64 > len {
        return Err("truncated ID3 tag".into());
    }
    id3::parse(&bytes(r, 0, n)?, out)
}
/// Follow item/frame headers and seek over all non-description payloads.
pub(super) fn maschine<R: Read + Seek>(r: &mut R, len: u64, out: &mut MetadataRead) -> Result<(), String> {
    let mut budget = 10000;
    item(r, 0, len, 0, &mut budget, out).map(|_| ())
}
fn item<R: Read + Seek>(
    r: &mut R,
    start: u64,
    limit: u64,
    depth: usize,
    budget: &mut usize,
    out: &mut MetadataRead,
) -> Result<u64, String> {
    if depth > 64 || *budget == 0 {
        return Err("Maschine container nesting/node limit".into());
    }
    *budget -= 1;
    let h = bytes(r, start, 40)?;
    if &h[12..16] != b"hsin" {
        return Err("invalid Maschine item".into());
    }
    let size = u64::from_le_bytes(h[..8].try_into().unwrap());
    let end = start.checked_add(size).filter(|e| *e <= limit && size >= 68).ok_or("invalid item size")?;
    let frame = bytes(r, start + 40, 20)?;
    let frame_end = (start + 40).checked_add(u64::from_le_bytes(frame[..8].try_into().unwrap())).ok_or("frame size overflow")?;
    if frame_end > end || frame_end < start + 60 {
        return Err("invalid frame size".into());
    }
    let mut at = start + 40;
    let mut parents = Vec::new();
    for _ in 0..64 {
        let h = bytes(r, at, 20)?;
        if &h[8..12] != b"DSIN" {
            return Err("invalid Maschine frame".into());
        }
        let n = u64::from_le_bytes(h[..8].try_into().unwrap());
        let fe = at
            .checked_add(n)
            .filter(|e| *e <= parents.last().map(|(_, e)| *e).unwrap_or(frame_end) && n >= 20)
            .ok_or("invalid nested frame size")?;
        let id = u32::from_le_bytes(h[12..16].try_into().unwrap());
        at += 20;
        if id == 1 {
            at = fe;
            for (id, parent_end) in parents.into_iter().rev() {
                if id == 108 {
                    let n = (parent_end - at) as usize;
                    if n > BLOCK {
                        return Err("metadata size limit".into());
                    }
                    let b = bytes(r, at, n)?;
                    out.metadata.fill(from_nisound(b.get(4..).ok_or("truncated SoundInfo")?)?);
                    return Ok(end);
                }
                at = parent_end;
            }
            break;
        }
        parents.push((id, fe));
    }
    let list = bytes(r, frame_end, 8)?;
    let n = u32::from_le_bytes(list[4..].try_into().unwrap());
    let mut at = frame_end + 8;
    for _ in 0..n {
        if at + 12 > end {
            return Err("truncated child list".into());
        }
        at = item(r, at + 12, end, depth + 1, budget, out)?;
        if out.metadata.name.is_some() {
            return Ok(end);
        }
    }
    Ok(end)
}
