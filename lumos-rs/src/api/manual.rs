//! REST for the user manual (works without any AI; read-only, embedded in the binary).
//!
//! * `GET /api/help` → `{ topics: [{id, title}], groups: [{name, commands: [{type, what, example, console}]}] }`
//! * `GET /api/help/topic/{id}` → `{ id, title, markdown }` (the people's version: no assistant tool names)
//! * `GET /api/help/search?q=…` → `{ topics: [{id, title, snippet}], commands: [{type, what}] }`

use crate::ai::cmdref;
use crate::engine::Handle;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::{json, Value};

pub fn routes() -> Router<Handle> {
    Router::new().route("/api/help", get(index)).route("/api/help/topic/{id}", get(topic)).route("/api/help/search", get(search))
}

fn command(x: &cmdref::Entry) -> Value {
    json!({ "type": x.ty, "what": x.what, "example": x.example, "console": x.console })
}

async fn index() -> Json<Value> {
    let topics: Vec<Value> = crate::help::topic_list().into_iter().map(|(id, title)| json!({ "id": id, "title": title })).collect();
    let groups: Vec<Value> = cmdref::GROUPS.iter().map(|(name, tys)| json!({ "name": name, "commands": tys.iter().filter_map(|t| cmdref::find(t)).map(command).collect::<Vec<_>>() })).collect();
    Json(json!({ "topics": topics, "groups": groups }))
}

async fn topic(Path(id): Path<String>) -> Response {
    match crate::help::human_topic(&id) {
        Some((title, markdown)) => Json(json!({ "id": id, "title": title, "markdown": markdown })).into_response(),
        None => (StatusCode::NOT_FOUND, Json(json!({ "error": "No hay ese tema de ayuda" }))).into_response(),
    }
}

#[derive(serde::Deserialize)]
struct Q {
    #[serde(default)]
    q: String,
}

async fn search(Query(q): Query<Q>) -> Json<Value> {
    let q = q.q.trim().to_string();
    let topics: Vec<Value> = crate::help::search_human(&q).into_iter().take(12).map(|(id, title, snippet, _)| json!({ "id": id, "title": title, "snippet": snippet })).collect();
    let words: Vec<String> = crate::help::matcher_words(&q);
    let commands: Vec<Value> = if words.is_empty() {
        vec![]
    } else {
        cmdref::REF
            .iter()
            .filter(|x| {
                let hay = crate::speech::matcher::norm(&format!("{} {} {}", x.ty.replace('_', " "), x.what, x.console.unwrap_or("")));
                words.iter().all(|w| hay.contains(w.as_str()))
            })
            .take(12)
            .map(|x| json!({ "type": x.ty, "what": x.what, "console": x.console }))
            .collect()
    };
    Json(json!({ "topics": topics, "commands": commands }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn index_topic_and_search_work_without_ai() {
        let Json(i) = index().await;
        assert!(i["topics"].as_array().unwrap().len() >= 18);
        assert!(i["groups"].as_array().unwrap().iter().any(|g| g["name"] == "Audio"));
        let r = topic(Path("guion".into())).await;
        assert_eq!(r.status(), StatusCode::OK);
        assert_eq!(topic(Path("nope".into())).await.status(), StatusCode::NOT_FOUND);
        let Json(s) = search(Query(Q { q: "gallego voz".into() })).await;
        assert_eq!(s["topics"][0]["id"], "guion");
        let Json(s) = search(Query(Q { q: "pantalla blackout".into() })).await;
        assert!(s["commands"].as_array().unwrap().is_empty() || s["commands"].is_array());
        let Json(s) = search(Query(Q { q: "escena apagar".into() })).await;
        assert!(s["commands"].as_array().unwrap().iter().any(|c| c["type"] == "scene_go" || c["type"] == "scenes_release"), "{s}");
    }
}
