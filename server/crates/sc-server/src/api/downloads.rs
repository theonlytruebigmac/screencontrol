//! Agent binary download endpoint.
//!
//! Serves pre-built agent binaries from the `./agent-builds/` directory.
//! No authentication required — agents need to download before they can register.

use axum::{
    body::Body,
    extract::Path,
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};

/// Build the downloads router.
pub fn router() -> Router {
    Router::new().route("/{filename}", get(download_agent))
}

/// Serve an agent binary file.
///
/// Only files matching `sc-agent-*` in `./agent-builds/` are served.
/// Returns 404 for missing files or filenames that don't match the allowlist.
async fn download_agent(Path(filename): Path<String>) -> Response {
    // Security: only allow filenames starting with "sc-agent-"
    if !filename.starts_with("sc-agent-") || filename.contains("..") || filename.contains('/') {
        return (StatusCode::NOT_FOUND, "Not found").into_response();
    }

    let file_path = std::path::PathBuf::from("./agent-builds").join(&filename);

    let data = match tokio::fs::read(&file_path).await {
        Ok(d) => d,
        Err(_) => {
            tracing::warn!("Agent binary not found: {}", file_path.display());
            return (StatusCode::NOT_FOUND, "Agent binary not found").into_response();
        }
    };

    let len = data.len();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, "application/octet-stream")
        .header(
            header::CONTENT_DISPOSITION,
            format!("attachment; filename=\"{}\"", filename),
        )
        .header(header::CONTENT_LENGTH, len)
        .body(Body::from(data))
        .unwrap()
        .into_response()
}
