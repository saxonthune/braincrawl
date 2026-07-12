//! GET /api/l3/graph — the research graph, lifted fresh from the configured
//! L3 root on every request. See `.rhidoc/01-product/03-web-ui.md`.

use axum::{
    extract::State,
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
};
use sha2::{Digest, Sha256};

use crate::AppState;

pub async fn handler_l3_graph(State(state): State<AppState>, headers: HeaderMap) -> Response {
    let Some(root) = state.l3_root.clone() else {
        return (
            StatusCode::NOT_FOUND,
            "BRAINCRAWL_L3_ROOT is not configured on this server",
        )
            .into_response();
    };

    let body = tokio::task::spawn_blocking(move || {
        let (graph, _warnings) = l3::parse(&root);
        serde_json::to_vec(&graph).expect("Graph serialization is infallible")
    })
    .await
    .expect("blocking task panicked");

    let etag = format!("\"{}\"", hex_digest(&body));

    if headers
        .get(axum::http::header::IF_NONE_MATCH)
        .and_then(|v| v.to_str().ok())
        == Some(etag.as_str())
    {
        return (
            StatusCode::NOT_MODIFIED,
            [(axum::http::header::ETAG, etag)],
        )
            .into_response();
    }

    (
        StatusCode::OK,
        [
            (axum::http::header::CONTENT_TYPE, "application/json".to_string()),
            (axum::http::header::ETAG, etag),
        ],
        body,
    )
        .into_response()
}

fn hex_digest(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}
