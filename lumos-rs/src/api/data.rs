//! REST for reports (PDF), patch CSV and MVR import/export.
//!
//! * `GET  /api/report/{patch|channels|cues|power|scenes}.pdf?lang=es|en`
//! * `GET  /api/export/patch.csv` · `POST /api/import/patch.csv[?replace=1]` (multipart `file`)
//! * `GET  /api/export/show.mvr`  · `POST /api/import/mvr[?replace=1]` (multipart `file`)

use crate::engine::Handle;
use crate::model::*;
use crate::{csv, gdtf, library, mvr, reports};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Path, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use std::collections::HashMap;

pub fn routes() -> Router<Handle> {
    Router::new()
        .route("/api/report/{name}", get(report))
        .route("/api/export/patch.csv", get(export_csv))
        .route("/api/export/show.mvr", get(export_mvr))
        .route("/api/import/patch.csv", post(import_csv).layer(DefaultBodyLimit::max(32 * 1024 * 1024)))
        .route("/api/import/mvr", post(import_mvr).layer(DefaultBodyLimit::max(512 * 1024 * 1024)))
}

#[derive(serde::Deserialize, Default)]
struct Opts {
    lang: Option<String>,
    replace: Option<String>,
}

fn truthy(s: &Option<String>) -> bool {
    matches!(s.as_deref(), Some("1" | "true" | "yes" | "on"))
}

fn file_name(show: &Show, ext: &str) -> String {
    let base: String = show.name.chars().map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' }).collect();
    format!("{}.{ext}", if base.is_empty() { "show".into() } else { base })
}

fn err(code: StatusCode, msg: impl Into<String>) -> Response {
    (code, Json(serde_json::json!({ "error": msg.into() }))).into_response()
}

async fn report(State(h): State<Handle>, Path(name): Path<String>, Query(o): Query<Opts>) -> Response {
    let Some(kind) = reports::kind_from(&name) else {
        return err(StatusCode::NOT_FOUND, "Unknown report (patch, channels, cues, power, scenes)");
    };
    let Some(show) = h.get_show().await else { return err(StatusCode::SERVICE_UNAVAILABLE, "engine stopped") };
    let lang = reports::Lang::parse(o.lang.as_deref());
    let fname = format!("{}-{}", name.trim_end_matches(".pdf"), file_name(&show, "pdf"));
    let pdf = tokio::task::spawn_blocking(move || reports::generate(kind, &show, lang, &reports::today())).await;
    match pdf {
        Ok(bytes) => (
            [
                (header::CONTENT_TYPE, "application/pdf".to_string()),
                (header::CONTENT_DISPOSITION, format!("inline; filename=\"{fname}\"")),
            ],
            bytes,
        )
            .into_response(),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("report failed: {e}")),
    }
}

async fn export_csv(State(h): State<Handle>) -> Response {
    let Some(show) = h.get_show().await else { return err(StatusCode::SERVICE_UNAVAILABLE, "engine stopped") };
    let fname = file_name(&show, "csv");
    (
        [
            (header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"patch-{fname}\"")),
        ],
        csv::export_patch(&show),
    )
        .into_response()
}

fn profile_map(show: &Show) -> HashMap<String, FixtureProfile> {
    let mut m: HashMap<String, FixtureProfile> = library::builtin().into_iter().map(|p| (p.id.clone(), p)).collect();
    for p in &show.profiles {
        m.insert(p.id.clone(), p.clone());
    }
    m
}

async fn export_mvr(State(h): State<Handle>) -> Response {
    let Some(show) = h.get_show().await else { return err(StatusCode::SERVICE_UNAVAILABLE, "engine stopped") };
    let lib_dir = h.store.library_dir();
    let fname = file_name(&show, "mvr");
    let res = tokio::task::spawn_blocking(move || mvr::export(&show, &profile_map(&show), &lib_dir)).await;
    match res {
        Ok(Ok(bytes)) => (
            [
                (header::CONTENT_TYPE, "application/zip".to_string()),
                (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{fname}\"")),
            ],
            bytes,
        )
            .into_response(),
        Ok(Err(e)) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, format!("export failed: {e}")),
    }
}

/// First file field of a multipart body.
async fn first_file(mp: &mut Multipart) -> Result<(String, axum::body::Bytes), Response> {
    loop {
        let field = match mp.next_field().await {
            Ok(Some(f)) => f,
            Ok(None) => return Err(err(StatusCode::BAD_REQUEST, "no file field")),
            Err(e) => return Err(err(StatusCode::BAD_REQUEST, e.to_string())),
        };
        let Some(name) = field.file_name().map(|s| s.to_string()) else { continue };
        return match field.bytes().await {
            Ok(b) => Ok((name, b)),
            Err(e) => Err(err(StatusCode::BAD_REQUEST, e.to_string())),
        };
    }
}

async fn import_csv(State(h): State<Handle>, Query(o): Query<Opts>, mut mp: Multipart) -> Response {
    let (_, bytes) = match first_file(&mut mp).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let text = String::from_utf8_lossy(&bytes).into_owned();
    let replace = truthy(&o.replace);
    let res = h
        .edit_show(move |show, profs| match csv::import_patch(show, profs, &text, replace) {
            Ok(out) => Ok(serde_json::json!({
                "imported": out.created + out.updated,
                "created": out.created,
                "updated": out.updated,
                "removed": out.removed,
                "errors": [],
            })),
            Err(errs) => Err(serde_json::json!({
                "imported": 0,
                "errors": errs.iter().map(|e| serde_json::json!({ "line": e.line, "error": e.error })).collect::<Vec<_>>(),
            })
            .to_string()),
        })
        .await;
    match res {
        Ok(v) => Json(v).into_response(),
        Err(e) => match serde_json::from_str::<serde_json::Value>(&e) {
            Ok(v) => (StatusCode::UNPROCESSABLE_ENTITY, Json(v)).into_response(),
            Err(_) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
        },
    }
}

async fn import_mvr(State(h): State<Handle>, Query(o): Query<Opts>, mut mp: Multipart) -> Response {
    let (_, bytes) = match first_file(&mut mp).await {
        Ok(x) => x,
        Err(r) => return r,
    };
    let replace = truthy(&o.replace);
    let parsed = match tokio::task::spawn_blocking(move || mvr::parse(&bytes)).await {
        Ok(Ok(p)) => p,
        Ok(Err(e)) => return err(StatusCode::BAD_REQUEST, e),
        Err(e) => return err(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
    };

    // Embedded GDTF files the fixtures refer to: import them (assets on disk) before patching.
    let mut spec_ids: HashMap<String, String> = HashMap::new();
    let mut new_profiles = Vec::new();
    let mut gdtf_errors = Vec::new();
    let mut imported_profiles = Vec::new();
    for (spec, data) in mvr::referenced_gdtf(&parsed) {
        let data = data.to_vec();
        match tokio::task::spawn_blocking(move || gdtf::import_gdtf(&data)).await {
            Ok(Ok(imp)) => {
                let store = h.store.clone();
                let dir = gdtf::asset_dir(&imp.profile.id);
                let files = imp.files;
                if let Err(e) = tokio::task::spawn_blocking(move || store.write_library_files(&dir, &files)).await.unwrap_or_else(|e| Err(e.to_string())) {
                    gdtf_errors.push(serde_json::json!({ "file": spec, "error": e }));
                    continue;
                }
                spec_ids.insert(spec, imp.profile.id.clone());
                imported_profiles.push(serde_json::json!({ "id": imp.profile.id, "name": format!("{} {}", imp.profile.manufacturer, imp.profile.model) }));
                new_profiles.push(imp.profile);
            }
            Ok(Err(e)) => gdtf_errors.push(serde_json::json!({ "file": spec, "error": e })),
            Err(e) => gdtf_errors.push(serde_json::json!({ "file": spec, "error": e.to_string() })),
        }
    }
    if !new_profiles.is_empty() {
        if let Err(e) = h.import_profiles(new_profiles).await {
            return err(StatusCode::INTERNAL_SERVER_ERROR, e);
        }
    }
    let fixtures = parsed.fixtures;
    let res = h
        .edit_show(move |show, profs| {
            let rep = mvr::apply(show, profs, &fixtures, &spec_ids, replace);
            Ok(serde_json::json!({
                "imported": rep.created + rep.updated,
                "created": rep.created,
                "updated": rep.updated,
                "unmatched": rep.unmatched.iter().map(|(f, s)| serde_json::json!({ "fixture": f, "spec": s })).collect::<Vec<_>>(),
                "skipped": rep.skipped.iter().map(|(f, r)| serde_json::json!({ "fixture": f, "reason": r })).collect::<Vec<_>>(),
                "warnings": rep.warnings,
            }))
        })
        .await;
    match res {
        Ok(mut v) => {
            v["profiles"] = serde_json::Value::Array(imported_profiles);
            v["errors"] = serde_json::Value::Array(gdtf_errors);
            Json(v).into_response()
        }
        Err(e) => err(StatusCode::INTERNAL_SERVER_ERROR, e),
    }
}
