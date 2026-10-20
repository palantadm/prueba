//! Show persistence: `<data>/shows/<name>.json`, `<data>/autosave.json`, `<data>/media/`.

use crate::model::{Show, ShowInfo};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Clone)]
pub struct Store {
    pub dir: PathBuf,
}

/// Allow letters, digits, space, `-`, `_`, `.`; reject separators and empty names.
pub fn sanitize_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.len() > 120 {
        return Err("Invalid show name".into());
    }
    if n.starts_with('.') || n.chars().any(|c| !(c.is_alphanumeric() || " -_.()".contains(c))) {
        return Err("Show name may only contain letters, digits, spaces and - _ . ( )".into());
    }
    Ok(n.to_string())
}

fn write_atomic(path: &Path, data: &[u8]) -> std::io::Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data)?;
    std::fs::rename(&tmp, path)
}

impl Store {
    pub fn new(dir: PathBuf) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir.join("shows"))?;
        std::fs::create_dir_all(dir.join("media"))?;
        Ok(Store { dir })
    }

    pub fn media_dir(&self) -> PathBuf {
        self.dir.join("media")
    }

    fn show_path(&self, name: &str) -> Result<PathBuf, String> {
        Ok(self.dir.join("shows").join(format!("{}.json", sanitize_name(name)?)))
    }

    pub fn list(&self) -> Vec<ShowInfo> {
        let mut out = Vec::new();
        if let Ok(rd) = std::fs::read_dir(self.dir.join("shows")) {
            for e in rd.flatten() {
                let p = e.path();
                if p.extension().and_then(|x| x.to_str()) != Some("json") {
                    continue;
                }
                let Some(name) = p.file_stem().and_then(|s| s.to_str()) else { continue };
                let modified = e
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs() as u32)
                    .unwrap_or(0);
                out.push(ShowInfo { name: name.to_string(), modified });
            }
        }
        out.sort_by(|a, b| b.modified.cmp(&a.modified).then(a.name.cmp(&b.name)));
        out
    }

    pub fn save(&self, name: &str, show: &Show) -> Result<(), String> {
        let p = self.show_path(name)?;
        let j = serde_json::to_vec_pretty(show).map_err(|e| e.to_string())?;
        write_atomic(&p, &j).map_err(|e| format!("Cannot write show: {e}"))
    }

    pub fn load(&self, name: &str) -> Result<Show, String> {
        let p = self.show_path(name)?;
        let s = std::fs::read(&p).map_err(|e| format!("Cannot read show '{name}': {e}"))?;
        serde_json::from_slice(&s).map_err(|e| format!("Show '{name}' is invalid: {e}"))
    }

    pub fn autosave(&self, show: &Show) -> Result<(), String> {
        let j = serde_json::to_vec(show).map_err(|e| e.to_string())?;
        write_atomic(&self.dir.join("autosave.json"), &j).map_err(|e| e.to_string())
    }

    pub fn load_autosave(&self) -> Option<Show> {
        let s = std::fs::read(self.dir.join("autosave.json")).ok()?;
        match serde_json::from_slice(&s) {
            Ok(sh) => Some(sh),
            Err(e) => {
                tracing::warn!("autosave unreadable, starting fresh: {e}");
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(sanitize_name("My Show 1").is_ok());
        assert!(sanitize_name("../etc").is_err());
        assert!(sanitize_name("a/b").is_err());
        assert!(sanitize_name("").is_err());
        assert!(sanitize_name(".hidden").is_err());
    }

    #[test]
    fn roundtrip() {
        let d = std::env::temp_dir().join(format!("lumos-persist-{}", std::process::id()));
        let st = Store::new(d.clone()).unwrap();
        let mut s = Show::default();
        s.name = "T".into();
        st.save("T", &s).unwrap();
        assert_eq!(st.load("T").unwrap().name, "T");
        assert_eq!(st.list().len(), 1);
        st.autosave(&s).unwrap();
        assert!(st.load_autosave().is_some());
        let _ = std::fs::remove_dir_all(d);
    }
}
