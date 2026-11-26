use std::path::{Path,PathBuf};
/// HFS paths retain their volume; only a verified startup volume maps to `/`.
pub fn hfs_to_posix(value:&str)->PathBuf {
    if value.starts_with('/') {return PathBuf::from(value);}
    let Some((volume,tail))=value.split_once(':') else{return PathBuf::from(value)};
    let base=PathBuf::from("/Volumes").join(volume);
    let base=if base.canonicalize().ok().as_deref()==Some(Path::new("/")){PathBuf::from("/")}else{base};
    base.join(tail.trim_matches(':').replace(':',"/"))
}
pub fn wine_to_host(prefix:&Path,value:&str)->Result<PathBuf,String> {
    let value=value.replace('\\',"/");let bytes=value.as_bytes();
    if bytes.len()<3 || bytes[1]!=b':' || bytes[2]!=b'/' || !bytes[0].is_ascii_alphabetic(){return Err(format!("unsupported Wine path: {value}"));}
    let drive=prefix.join("dosdevices").join(format!("{}:",(bytes[0] as char).to_ascii_lowercase()));
    let mut path=drive.canonicalize().map_err(|e|format!("{}: {e}",drive.display()))?;
    for part in value[3..].split('/').filter(|p|!p.is_empty() && *p!=".") {
        if part==".." {return Err("parent traversal in registered Wine path".into());}
        let exact=path.join(part);
        if exact.exists(){path=exact;continue;}
        match std::fs::read_dir(&path) {
            Ok(entries)=>{
                let mut found=None;let mut count=0;
                for e in entries {
                    count+=1;if count>100_000{return Err(format!("case lookup limit at {}",path.display()));}
                    let e=e.map_err(|e|e.to_string())?;
                    if e.file_name().to_string_lossy().to_lowercase()==part.to_lowercase(){
                        if found.is_some(){return Err(format!("ambiguous case match for {part} in {}",path.display()));} found=Some(e.path());
                    }
                }
                path=found.unwrap_or(exact);
            }
            Err(e) if e.kind()==std::io::ErrorKind::NotFound=>path=exact,
            Err(e)=>return Err(format!("{}: {e}",path.display())),
        }
    }
    Ok(path)
}
