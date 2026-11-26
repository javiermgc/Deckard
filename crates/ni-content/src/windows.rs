use crate::{DiscoveryReport,Installation};
use std::path::PathBuf;
#[derive(Debug,Clone)]
pub struct Registration {pub name:String,pub content_dir:Option<PathBuf>,pub version:Option<String>}
pub fn from_registrations(source:&str,rows:Vec<Registration>)->DiscoveryReport {
    let mut r=DiscoveryReport::default();
    for row in rows {if let Some(root)=row.content_dir {if !root.as_os_str().is_empty(){r.installations.push(Installation::registered(source,&row.name,Some(root),row.version));}}}
    r.complete_sources.push(source.into());r
}
#[cfg(windows)]
pub fn discover()->DiscoveryReport{
    use windows_sys::Win32::System::Registry::*;
    use windows_sys::Win32::Foundation::{ERROR_NO_MORE_ITEMS,ERROR_FILE_NOT_FOUND,ERROR_PATH_NOT_FOUND};
    let mut report=DiscoveryReport::default();
    for (label,view) in [("windows64",KEY_WOW64_64KEY),("windows32",KEY_WOW64_32KEY)] {
        let mut rows=Vec::new();let mut complete=true;
        // SAFETY: all keys are read-only, handles are closed by Key's Drop.
        let base=match Key::open(HKEY_LOCAL_MACHINE,"SOFTWARE\\Native Instruments",view){
            Ok(k)=>k,Err(e) if e==ERROR_FILE_NOT_FOUND || e==ERROR_PATH_NOT_FOUND=>{report.complete_sources.push(label.into());continue;},
            Err(e)=>{report.issue(label,format!("registry open error {e}"));continue;}
        };
        let mut index=0;
        loop {
            let mut buf=[0u16;256];let mut n=buf.len() as u32;
            let rc=unsafe{RegEnumKeyExW(base.0,index,buf.as_mut_ptr(),&mut n,std::ptr::null(),std::ptr::null_mut(),std::ptr::null_mut(),std::ptr::null_mut())};
            if rc==ERROR_NO_MORE_ITEMS{break;}
            if rc!=0{complete=false;report.issue(label,format!("registry enumerate error {rc}"));break;}
            index+=1;let name=String::from_utf16_lossy(&buf[..n as usize]);
            match Key::open(base.0,&name,view).and_then(|k|Ok((k.string("ContentDir")?,k.string("ContentVersion")?))) {
                Ok((dir,version))=>rows.push(Registration{name,content_dir:dir.map(PathBuf::from),version}),
                Err(e)=>{complete=false;report.issue(label,format!("{name}: registry read error {e}"));}
            }
        }
        let mut part=from_registrations(label,rows);if !complete{part.complete_sources.clear();}
        if let Some(public)=std::env::var_os("PUBLIC"){for i in &mut part.installations{i.image_roots.push(PathBuf::from(&public).join("Documents/NI Resources/image"));}}
        report.extend(part);
    }
    report
}
#[cfg(windows)]
struct Key(windows_sys::Win32::System::Registry::HKEY);
#[cfg(windows)]
impl Drop for Key{fn drop(&mut self){unsafe{windows_sys::Win32::System::Registry::RegCloseKey(self.0);}}}
#[cfg(windows)]
impl Key {
    fn open(root:windows_sys::Win32::System::Registry::HKEY,name:&str,view:u32)->Result<Self,u32>{
        use windows_sys::Win32::System::Registry::*;
        let name:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();let mut key=std::ptr::null_mut();
        let rc=unsafe{RegOpenKeyExW(root,name.as_ptr(),0,KEY_READ|view,&mut key)};if rc==0{Ok(Self(key))}else{Err(rc)}
    }
    fn string(&self,name:&str)->Result<Option<String>,u32>{
        use windows_sys::Win32::System::Registry::*;
        let name:Vec<u16>=name.encode_utf16().chain(Some(0)).collect();let mut n=0;let mut typ=0;
        let rc=unsafe{RegQueryValueExW(self.0,name.as_ptr(),std::ptr::null(),&mut typ,std::ptr::null_mut(),&mut n)};
        if rc==2{return Ok(None);}if rc!=0{return Err(rc);}if ![REG_SZ,REG_EXPAND_SZ].contains(&typ)||n>131072{return Ok(None);}
        let mut data=vec![0u16;(n as usize).div_ceil(2)+1];
        let rc=unsafe{RegQueryValueExW(self.0,name.as_ptr(),std::ptr::null(),&mut typ,data.as_mut_ptr().cast(),&mut n)};if rc!=0{return Err(rc);}
        let end=data.iter().position(|c|*c==0).unwrap_or(data.len());let value=String::from_utf16_lossy(&data[..end]);
        if typ==REG_EXPAND_SZ {
            use windows_sys::Win32::System::Environment::ExpandEnvironmentStringsW;
            let need=unsafe{ExpandEnvironmentStringsW(data.as_ptr(),std::ptr::null_mut(),0)};
            if need==0||need>65536{return Err(13);}let mut expanded=vec![0u16;need as usize];
            let n=unsafe{ExpandEnvironmentStringsW(data.as_ptr(),expanded.as_mut_ptr(),need)};if n==0||n>need{return Err(13);}
            Ok(Some(String::from_utf16_lossy(&expanded[..n.saturating_sub(1) as usize])))
        }else{Ok(Some(value))}
    }
}
