//! Whisper (ggml) model catalogue, storage and download. Models are fetched on demand into `<data>/models/whisper/`
//! so the installer stays small; a custom `.bin` (e.g. a Galician fine-tune converted to ggml) can be used instead.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

pub struct ModelDef {
    pub id: &'static str,
    pub label: &'static str,
    pub file: &'static str,
    pub bytes: u64,
}

pub const BASE_URL: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/";

pub const MODELS: &[ModelDef] = &[
    ModelDef { id: "tiny", label: "Tiny — 75 MB, muy rápido, poco preciso", file: "ggml-tiny.bin", bytes: 77_691_713 },
    ModelDef { id: "base-q5_1", label: "Base — 57 MB, rápido", file: "ggml-base-q5_1.bin", bytes: 59_707_625 },
    ModelDef { id: "small-q5_1", label: "Small — 181 MB, equilibrado (recomendado)", file: "ggml-small-q5_1.bin", bytes: 190_085_487 },
    ModelDef { id: "medium-q5_0", label: "Medium — 514 MB, preciso (PC potente)", file: "ggml-medium-q5_0.bin", bytes: 539_212_467 },
    ModelDef { id: "large-v3-turbo-q5_0", label: "Large v3 turbo — 547 MB, el más preciso", file: "ggml-large-v3-turbo-q5_0.bin", bytes: 574_041_195 },
];

pub fn find(id: &str) -> Option<&'static ModelDef> {
    MODELS.iter().find(|m| m.id == id)
}

pub fn dir(data: &Path) -> PathBuf {
    data.join("models").join("whisper")
}

pub fn path_of(data: &Path, id: &str) -> Option<PathBuf> {
    find(id).map(|m| dir(data).join(m.file))
}

pub fn installed(data: &Path, id: &str) -> bool {
    match (find(id), path_of(data, id)) {
        (Some(m), Some(p)) => std::fs::metadata(p).map(|md| md.len() == m.bytes).unwrap_or(false),
        _ => false,
    }
}

/// Resolve the model file to load: the custom path when set (must exist), else the catalogue model when installed.
pub fn resolve(data: &Path, id: &str, custom: &str) -> Result<PathBuf, String> {
    let custom = custom.trim();
    if !custom.is_empty() {
        let p = PathBuf::from(custom);
        return if p.is_file() { Ok(p) } else { Err(format!("No existe el modelo personalizado «{custom}».")) };
    }
    if find(id).is_none() {
        return Err(format!("Modelo «{id}» desconocido."));
    }
    if installed(data, id) {
        Ok(path_of(data, id).expect("known model"))
    } else {
        Err(format!("El modelo «{id}» no está descargado todavía."))
    }
}

pub fn remove(data: &Path, id: &str) -> Result<(), String> {
    let p = path_of(data, id).ok_or("Modelo desconocido")?;
    match std::fs::remove_file(&p) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

/// Download `id` with progress in permille (0..=1000) into `progress`. Writes to a `.part` file and renames it only
/// when the size matches, so an interrupted download never leaves a broken model behind.
pub async fn download(data: &Path, id: &str, base_url: &str, progress: Arc<AtomicU32>) -> Result<(), String> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;
    let m = find(id).ok_or("Modelo desconocido")?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let d = dir(data);
    tokio::fs::create_dir_all(&d).await.map_err(|e| e.to_string())?;
    let dest = d.join(m.file);
    let part = d.join(format!("{}.part", m.file));
    let resp = reqwest::Client::builder().connect_timeout(std::time::Duration::from_secs(20)).build().map_err(|e| e.to_string())?.get(format!("{base_url}{}", m.file)).send().await.map_err(|e| format!("No se pudo conectar para descargar el modelo: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("La descarga falló (HTTP {}).", resp.status().as_u16()));
    }
    let total = resp.content_length().unwrap_or(m.bytes).max(1);
    let mut file = tokio::fs::File::create(&part).await.map_err(|e| e.to_string())?;
    let mut got: u64 = 0;
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| format!("Descarga interrumpida: {e}"))?;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        got += chunk.len() as u64;
        progress.store(((got * 1000) / total).min(1000) as u32, Ordering::Relaxed);
    }
    file.flush().await.map_err(|e| e.to_string())?;
    drop(file);
    if got != m.bytes {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(format!("Descarga incompleta ({got} de {} bytes). Vuelve a intentarlo.", m.bytes));
    }
    tokio::fs::rename(&part, &dest).await.map_err(|e| e.to_string())?;
    progress.store(1000, Ordering::Relaxed);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lumos-models-{tag}-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn resolve_prefers_custom_and_reports_missing() {
        let d = tmp("resolve");
        assert!(resolve(&d, "small-q5_1", "").unwrap_err().contains("no está descargado"));
        assert!(resolve(&d, "nope", "").unwrap_err().contains("desconocido"));
        let c = d.join("gl.bin");
        assert!(resolve(&d, "tiny", c.to_str().unwrap()).unwrap_err().contains("No existe"));
        std::fs::write(&c, b"x").unwrap();
        assert_eq!(resolve(&d, "tiny", c.to_str().unwrap()).unwrap(), c);
        // a catalogue file of the wrong size does not count as installed
        std::fs::create_dir_all(dir(&d)).unwrap();
        std::fs::write(path_of(&d, "tiny").unwrap(), b"short").unwrap();
        assert!(!installed(&d, "tiny"));
        assert!(remove(&d, "tiny").is_ok() && remove(&d, "tiny").is_ok());
    }

    /// A tiny local HTTP server plays Hugging Face: progress reaches 1000 and a truncated body is rejected.
    #[tokio::test]
    async fn download_checks_size_and_reports_progress() {
        use axum::{routing::get, Router};
        let d = tmp("dl");
        // pretend the catalogue's `tiny` is 100 bytes by serving the wrong size first
        let app = Router::new().route("/ggml-tiny.bin", get(|| async { vec![7u8; 100] }));
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}/", l.local_addr().unwrap());
        tokio::spawn(async move {
            let _ = axum::serve(l, app).await;
        });
        let p = Arc::new(AtomicU32::new(0));
        let e = download(&d, "tiny", &base, p.clone()).await.unwrap_err();
        assert!(e.contains("incompleta"), "{e}");
        assert!(!dir(&d).join("ggml-tiny.bin").exists() && !dir(&d).join("ggml-tiny.bin.part").exists(), "no leftovers");
        assert!(p.load(Ordering::Relaxed) > 0);
        assert!(download(&d, "nope", &base, p.clone()).await.is_err());
        assert!(download(&d, "tiny", "http://127.0.0.1:1/", p).await.unwrap_err().contains("No se pudo conectar"));
    }
}
