use deckard_ni_content::metadata::*;
use std::io::{Seek, SeekFrom, Write};
fn str_(b: &mut Vec<u8>, s: &str) {
    b.push(0xa0 | s.len() as u8);
    b.extend(s.as_bytes());
}
fn description() -> Vec<u8> {
    let mut b = vec![0x85];
    str_(&mut b, "name");
    str_(&mut b, "Shuffle Woo 1");
    str_(&mut b, "bankchain");
    b.push(0x91);
    str_(&mut b, "Rhythm Source");
    str_(&mut b, "types");
    b.extend([0x91, 0x92]);
    str_(&mut b, "Percussion");
    str_(&mut b, "Other");
    str_(&mut b, "deviceType");
    str_(&mut b, "LOOP");
    str_(&mut b, "tempo");
    b.push(0xcb);
    b.extend(123.0f64.to_be_bytes());
    b
}
fn sync(n: usize) -> [u8; 4] {
    [((n >> 21) & 127) as u8, ((n >> 14) & 127) as u8, ((n >> 7) & 127) as u8, (n & 127) as u8]
}
fn id3(version: u8) -> Vec<u8> {
    let mut geob = b"\0\0\0com.native-instruments.nks.soundinfo\0".to_vec();
    geob.extend(description());
    let mut frame = b"GEOB".to_vec();
    frame.extend(if version == 4 { sync(geob.len()) } else { (geob.len() as u32).to_be_bytes() });
    frame.extend([0, 0]);
    frame.extend(geob);
    let mut tag = vec![b'I', b'D', b'3', version, 0, 0];
    tag.extend(sync(frame.len()));
    tag.extend(frame);
    tag
}
fn riff(chunks: Vec<(&[u8; 4], Vec<u8>)>, form: &[u8; 4]) -> Vec<u8> {
    let mut body = form.to_vec();
    for (id, b) in chunks {
        body.extend(id);
        body.extend((b.len() as u32).to_le_bytes());
        body.extend(&b);
        if b.len() % 2 == 1 {
            body.push(0);
        }
    }
    let mut out = b"RIFF".to_vec();
    out.extend((body.len() as u32).to_le_bytes());
    out.extend(body);
    out
}
#[test]
fn wav_geob_types_product_kind_and_tempo() {
    for version in [3, 4] {
        let t = tempfile::tempdir().unwrap();
        let p = t.path().join("untagged 90.wav");
        std::fs::write(&p, riff(vec![(b"JUNK", vec![0]), (b"ID3 ", id3(version))], b"WAVE")).unwrap();
        let r = read_content(&p).unwrap();
        assert!(r.warnings.is_empty(), "{:?}", r.warnings);
        let m = r.metadata;
        assert_eq!(m.name.as_deref(), Some("Shuffle Woo 1"));
        assert_eq!(m.bank, vec!["Rhythm Source"]);
        assert_eq!(m.types, vec![vec!["Percussion", "Other"]]);
        assert_eq!(m.kind, Some(ContentKind::Loop));
        assert_eq!(m.tempo, Some(123.0));
    }
}
#[test]
fn nks_description_without_plugin_state() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("preset.nksf");
    let mut b = 1u32.to_le_bytes().to_vec();
    b.extend(description());
    std::fs::write(&p, riff(vec![(b"NISI", b)], b"NIKS")).unwrap();
    assert_eq!(read_content(&p).unwrap().metadata.name.as_deref(), Some("Shuffle Woo 1"));
}
#[test]
fn inventory_only_and_truncated_metadata() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("preset.nki");
    std::fs::write(&p, b"opaque native preset").unwrap();
    assert_eq!(read_content(&p).unwrap().metadata, ContentMetadata::default());
    let p = t.path().join("broken.wav");
    std::fs::write(&p, b"RIFF\xff\xff\xff\xffWAVEID3 \xff\xff\xff\xff").unwrap();
    assert!(!read_content(&p).unwrap().warnings.is_empty());
}
#[test]
fn skips_large_audio_payload() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("large.wav");
    let mut f = std::fs::File::create(&p).unwrap();
    let tag = id3(4);
    let size = 128 * 1024 * 1024u32;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(4 + 8 + size + 8 + tag.len() as u32 + (tag.len() % 2) as u32).to_le_bytes()).unwrap();
    f.write_all(b"WAVEdata").unwrap();
    f.write_all(&size.to_le_bytes()).unwrap();
    f.seek(SeekFrom::Current(size as i64)).unwrap();
    f.write_all(b"ID3 ").unwrap();
    f.write_all(&(tag.len() as u32).to_le_bytes()).unwrap();
    f.write_all(&tag).unwrap();
    if tag.len() % 2 == 1 {
        f.write_all(&[0]).unwrap();
    }
    assert_eq!(read_content(&p).unwrap().metadata.tempo, Some(123.0));
}
#[test]
fn maschine_sound_info_without_reading_archive() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("group.mxgrp");
    let mut info = mxprj::sound_info::SoundInfo::default();
    info.name = "Tagged kit".into();
    info.bank = vec!["Synthetic".into()];
    info.set_tags(&[vec!["Kits".into(), "Percussion".into()]], &["Acoustic".into()]);
    let b = mxprj::container::new_file(*b"MAS2", b"irrelevant archive", "test", &info, &mut || [0; 16]);
    std::fs::write(&p, b).unwrap();
    let r = read_content(&p).unwrap();
    assert!(r.warnings.is_empty(), "{:?}", r.warnings);
    assert_eq!(r.metadata.name.as_deref(), Some("Tagged kit"));
    assert_eq!(r.metadata.characters, vec!["Acoustic"]);
}
#[test]
fn mp3_and_aiff_metadata() {
    let t = tempfile::tempdir().unwrap();
    for ext in ["mp3", "aiff"] {
        let p = t.path().join(format!("sample.{ext}"));
        let tag = id3(4);
        let mut b = tag.clone();
        if ext == "aiff" {
            let mut body = b"AIFFID3 ".to_vec();
            body.extend((tag.len() as u32).to_be_bytes());
            body.extend(&tag);
            if tag.len() % 2 == 1 {
                body.push(0);
            }
            b = b"FORM".to_vec();
            b.extend((body.len() as u32).to_be_bytes());
            b.extend(body);
        }
        std::fs::write(&p, b).unwrap();
        assert_eq!(read_content(&p).unwrap().metadata.tempo, Some(123.0));
    }
}
#[test]
fn unnamed_maschine_description_does_not_corrupt_child_traversal() {
    let t = tempfile::tempdir().unwrap();
    let p = t.path().join("project.mxprj");
    let info = mxprj::sound_info::SoundInfo::default();
    std::fs::write(&p, mxprj::container::new_file(*b"PROJ", b"archive", "test", &info, &mut || [0; 16])).unwrap();
    assert!(read_content(&p).unwrap().warnings.is_empty());
}
