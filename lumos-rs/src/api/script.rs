//! REST for the running-order feature.
//!
//! * `POST /api/script/import` (multipart `file`: .pdf .txt .md .fountain) → draft [`ScriptImport`] (not saved);
//!   a PDF is kept in `<data>/media` and its URL returned as `pdf`. 415 unknown type, 422 scanned / unreadable.
//! * `POST /api/script/suggest` (JSON `{ "script_id": N }` or `{ "blocks": [...] }`) → `{ "suggestions": [...] }`.

use crate::engine::Handle;
use crate::model::*;
use crate::script_import::{self, ImportError};
use axum::{
    extract::{DefaultBodyLimit, Multipart, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};

pub fn routes() -> Router<Handle> {
    Router::new()
        .route("/api/script/import", post(import).layer(DefaultBodyLimit::max(64 * 1024 * 1024)))
        .route("/api/script/suggest", post(suggest).layer(DefaultBodyLimit::max(16 * 1024 * 1024)))
}

fn err(code: StatusCode, msg: impl Into<String>, kind: &str) -> Response {
    (code, Json(serde_json::json!({ "error": msg.into(), "code": kind }))).into_response()
}

async fn import(State(h): State<Handle>, mut mp: Multipart) -> Response {
    let (fname, bytes) = loop {
        match mp.next_field().await {
            Ok(Some(f)) => {
                let Some(name) = f.file_name().map(|s| s.to_string()) else { continue };
                match f.bytes().await {
                    Ok(b) => break (name, b),
                    Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string(), "bad_request"),
                }
            }
            Ok(None) => return err(StatusCode::BAD_REQUEST, "no file field", "bad_request"),
            Err(e) => return err(StatusCode::BAD_REQUEST, e.to_string(), "bad_request"),
        }
    };
    let name = fname.clone();
    let data = bytes.clone();
    let res = tokio::task::spawn_blocking(move || script_import::import(&name, &data)).await;
    let mut draft = match res {
        Ok(Ok(d)) => d,
        Ok(Err(e)) => {
            let (code, kind) = match &e {
                ImportError::Unsupported(_) => (StatusCode::UNSUPPORTED_MEDIA_TYPE, "unsupported"),
                ImportError::Scanned => (StatusCode::UNPROCESSABLE_ENTITY, "scanned_pdf"),
                ImportError::Unreadable(_) => (StatusCode::UNPROCESSABLE_ENTITY, "unreadable"),
            };
            return err(code, e.to_string(), kind);
        }
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, format!("import failed: {e}"), "internal"),
    };
    if fname.to_ascii_lowercase().ends_with(".pdf") {
        let ms = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        let base: String = fname
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or("guion.pdf")
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' })
            .take(80)
            .collect();
        let file = format!("{ms}-{}", base.trim_start_matches('.'));
        let path = h.store.media_dir().join(&file);
        match tokio::fs::write(&path, &bytes).await {
            Ok(()) => draft.pdf = Some(format!("/media/{file}")),
            Err(e) => draft.warnings.push(format!("The original PDF could not be stored: {e}")),
        }
    }
    Json(draft).into_response()
}

#[derive(serde::Deserialize)]
struct SuggestReq {
    script_id: Option<u32>,
    #[serde(default)]
    blocks: Vec<ScriptBlock>,
}

async fn suggest(State(h): State<Handle>, Json(req): Json<SuggestReq>) -> Response {
    let blocks = match req.script_id {
        Some(id) => {
            let Some(show) = h.get_show().await else { return err(StatusCode::SERVICE_UNAVAILABLE, "engine stopped", "internal") };
            match show.scripts.into_iter().find(|s| s.id == id) {
                Some(s) => s.blocks,
                None => return err(StatusCode::NOT_FOUND, format!("script {id} does not exist"), "not_found"),
            }
        }
        None => req.blocks,
    };
    let suggestions = tokio::task::spawn_blocking(move || script_import::suggest(&blocks)).await.unwrap_or_default();
    Json(serde_json::json!({ "suggestions": suggestions })).into_response()
}
