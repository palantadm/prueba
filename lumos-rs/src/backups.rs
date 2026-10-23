//! Show backups in `<data>/backups/`: rolling automatic snapshots (last N kept) and manual, labelled ones.
//! Files: `<id>.json` (the show) + `<id>.meta.json` (`{label, kind, at}`); `id = <unix ms>-<auto|manual>[-n]`.

use crate::model::{BackupInfo, Show};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub const KEEP_AUTO: usize = 20;

#[derive(Clone)]
pub struct Backups {
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct Meta {
    label: String,
    kind: String,
    at: u64,
}

fn valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_digit() || c == '-' || c.is_ascii_lowercase())
}

fn now_ms() -> u128 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0)
}

impl Backups {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(&dir);
        Backups { dir }
    }

    fn show_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
    fn meta_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.meta.json"))
    }

    /// `kind`: `"auto"` or `"manual"`.
    pub fn create(&self, show: &Show, label: &str, kind: &str) -> Result<BackupInfo, String> {
        let kind = if kind == "manual" { "manual" } else { "auto" };
        let ms = now_ms();
        let mut id = format!("{ms}-{kind}");
        let mut n = 1;
        while self.show_path(&id).exists() {
            id = format!("{ms}-{kind}-{n}");
            n += 1;
        }
        let json = serde_json::to_vec(show).map_err(|e| e.to_string())?;
        std::fs::write(self.show_path(&id), &json).map_err(|e| format!("Cannot write backup: {e}"))?;
        let meta = Meta { label: label.trim().chars().take(120).collect(), kind: kind.into(), at: (ms / 1000) as u64 };
        std::fs::write(self.meta_path(&id), serde_json::to_vec(&meta).map_err(|e| e.to_string())?)
            .map_err(|e| format!("Cannot write backup: {e}"))?;
        if kind == "auto" {
            self.prune_auto(KEEP_AUTO);
        }
        Ok(BackupInfo { id, label: meta.label, at: meta.at as u32, kind: meta.kind, size: json.len().min(u32::MAX as usize) as u32 })
    }

    /// Newest first.
    pub fn list(&self) -> Vec<BackupInfo> {
        let mut out = Vec::new();
        let Ok(rd) = std::fs::read_dir(&self.dir) else { return out };
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let Some(id) = name.strip_suffix(".meta.json") else { continue };
            if !valid_id(id) {
                continue;
            }
            let Ok(raw) = std::fs::read(e.path()) else { continue };
            let Ok(meta) = serde_json::from_slice::<Meta>(&raw) else { continue };
            let size = std::fs::metadata(self.show_path(id)).map(|m| m.len()).unwrap_or(0);
            if size == 0 {
                continue;
            }
            out.push(BackupInfo { id: id.to_string(), label: meta.label, at: meta.at as u32, kind: meta.kind, size: size.min(u32::MAX as u64) as u32 });
        }
        out.sort_by(|a, b| b.at.cmp(&a.at).then(b.id.cmp(&a.id)));
        out
    }

    pub fn load(&self, id: &str) -> Result<Show, String> {
        if !valid_id(id) {
            return Err("Invalid backup id".into());
        }
        let raw = std::fs::read(self.show_path(id)).map_err(|_| format!("Backup '{id}' not found"))?;
        serde_json::from_slice(&raw).map_err(|e| format!("Backup '{id}' is invalid: {e}"))
    }

    pub fn delete(&self, id: &str) -> Result<(), String> {
        if !valid_id(id) {
            return Err("Invalid backup id".into());
        }
        if !self.show_path(id).exists() {
            return Err(format!("Backup '{id}' not found"));
        }
        let _ = std::fs::remove_file(self.meta_path(id));
        std::fs::remove_file(self.show_path(id)).map_err(|e| e.to_string())
    }

    /// Delete the oldest automatic backups beyond `keep`. Manual ones are never touched.
    pub fn prune_auto(&self, keep: usize) {
        let autos: Vec<BackupInfo> = self.list().into_iter().filter(|b| b.kind == "auto").collect();
        for b in autos.into_iter().skip(keep) {
            let _ = self.delete(&b.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp() -> PathBuf {
        use std::sync::atomic::{AtomicU32, Ordering};
        static N: AtomicU32 = AtomicU32::new(0);
        std::env::temp_dir().join(format!("lumos-bk-{}-{}", std::process::id(), N.fetch_add(1, Ordering::SeqCst)))
    }

    fn show(name: &str) -> Show {
        let mut s = Show::default();
        s.name = name.into();
        s
    }

    #[test]
    fn create_list_load_delete() {
        let b = Backups::new(tmp());
        let a = b.create(&show("uno"), "  primera  ", "manual").unwrap();
        let c = b.create(&show("dos"), "", "auto").unwrap();
        assert_ne!(a.id, c.id);
        let l = b.list();
        assert_eq!(l.len(), 2);
        assert_eq!(l.iter().find(|x| x.id == a.id).unwrap().label, "primera");
        assert_eq!(b.load(&a.id).unwrap().name, "uno");
        assert_eq!(b.load(&c.id).unwrap().name, "dos");
        b.delete(&a.id).unwrap();
        assert!(b.load(&a.id).is_err());
        assert_eq!(b.list().len(), 1);
        assert!(b.delete("nope").is_err());
        assert!(b.load("../etc/passwd").is_err());
        assert!(b.delete("../x").is_err());
    }

    #[test]
    fn same_millisecond_ids_are_unique_and_auto_pruned_manual_kept() {
        let b = Backups::new(tmp());
        let m = b.create(&show("keep"), "manual", "manual").unwrap();
        for i in 0..(KEEP_AUTO + 7) {
            b.create(&show(&format!("a{i}")), "", "auto").unwrap();
        }
        let l = b.list();
        let autos = l.iter().filter(|x| x.kind == "auto").count();
        assert_eq!(autos, KEEP_AUTO);
        assert!(l.iter().any(|x| x.id == m.id), "manual backup survives pruning");
        let ids: std::collections::HashSet<_> = l.iter().map(|x| x.id.clone()).collect();
        assert_eq!(ids.len(), l.len());
        // the newest automatic backup is the last one written
        let newest = l.iter().filter(|x| x.kind == "auto").next().unwrap();
        assert_eq!(b.load(&newest.id).unwrap().name, format!("a{}", KEEP_AUTO + 6));
    }
}
