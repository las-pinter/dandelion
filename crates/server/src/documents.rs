//! Document collections from plugin manifests.
//!
//! A plugin drops a JSON manifest in `~/.config/dandelion/plugins/` and its
//! collection appears on the Documents page; dandelion never loads or runs
//! plugin code. Documents are written by tools on this host, so they are
//! untrusted: an HTML document answers only with a CSP that puts it in an
//! opaque origin (no `allow-same-origin`), so its scripts cannot reach the API
//! or read the session cookie, and the page frames it with the matching
//! `sandbox` attribute. Every request path is resolved by the documents crate
//! against the manifest root and its include patterns, so a path that escapes
//! the root or was never listed answers 404.

use axum::{
    Json, Router,
    body::Body,
    extract::{Path as UrlPath, Query, State},
    http::{HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use dandelion_documents::{
    DocumentFile, DocumentKind, Documents, DocumentsError, DocumentsSnapshot,
};
use serde::Deserialize;

use crate::{app::ApiError, auth::middleware::error_response, config, state::AppState};

/// The CSP that renders a document in an opaque origin, with no `allow-same-origin`.
pub const DOCUMENT_CSP: &str = "sandbox allow-scripts; default-src 'none'; \
style-src 'unsafe-inline'; script-src 'unsafe-inline'; img-src data:; \
font-src data:; connect-src 'none'; frame-ancestors 'self'";

/// Routes mounted under `/api/documents`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(snapshot))
        .route("/{id}/file", get(file))
}

/// The document reader for this server's environment.
#[must_use]
pub fn from_env() -> Documents {
    if let (Some(plugins), Some(home)) = (config::plugins_dir(), config::home_dir()) {
        return Documents::new(plugins, home);
    }
    tracing::warn!("no plugin folder or home directory; document collections are off");
    Documents::disabled()
}

/// The query of a document request.
#[derive(Debug, Deserialize)]
pub struct FileQuery {
    /// Path relative to the collection root, as the listing reported it.
    pub path: String,
}

/// Lists every valid collection and every manifest that was skipped.
#[utoipa::path(
    get,
    path = "/api/documents",
    operation_id = "documents_snapshot",
    tag = "documents",
    responses(
        (status = 200, description = "Collections and manifest warnings", body = DocumentsSnapshot),
        (status = 500, description = "The listing task failed", body = ApiError)
    )
)]
pub async fn snapshot(State(state): State<AppState>) -> Response {
    let documents = state.documents.clone();
    match tokio::task::spawn_blocking(move || documents.snapshot()).await {
        Ok(snapshot) => Json(snapshot).into_response(),
        Err(error) => {
            tracing::error!(%error, "documents listing task failed");
            internal_error()
        }
    }
}

/// Serves one listed document, with headers that keep it outside this origin.
#[utoipa::path(
    get,
    path = "/api/documents/{id}/file",
    operation_id = "documents_file",
    tag = "documents",
    params(
        ("id" = String, Path, description = "Collection id (the manifest file stem)"),
        ("path" = String, Query, description = "Path relative to the collection root")
    ),
    responses(
        (status = 200, description = "The document, as an opaque-origin sandbox or plain text"),
        (status = 404, description = "No such collection or listed document", body = ApiError),
        (status = 413, description = "The document is larger than the serving cap", body = ApiError),
        (status = 500, description = "The document could not be read", body = ApiError)
    )
)]
pub async fn file(
    State(state): State<AppState>,
    UrlPath(id): UrlPath<String>,
    Query(query): Query<FileQuery>,
) -> Response {
    let documents = state.documents.clone();
    let path = query.path;
    match tokio::task::spawn_blocking(move || documents.read(&id, &path)).await {
        Ok(Ok(file)) => document_response(file),
        Ok(Err(error)) => documents_error_response(&error),
        Err(error) => {
            tracing::error!(%error, "document read task failed");
            internal_error()
        }
    }
}

fn document_response(file: DocumentFile) -> Response {
    let content_type = HeaderValue::from_static(file.kind.content_type());
    let mut response = Response::new(Body::from(file.bytes));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, content_type);
    headers.insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    headers.insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    headers.insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    if file.kind == DocumentKind::Html {
        headers.insert(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(DOCUMENT_CSP),
        );
    }
    response
}

fn documents_error_response(error: &DocumentsError) -> Response {
    match error {
        DocumentsError::NotFound => error_response(
            StatusCode::NOT_FOUND,
            "not_found",
            "no such document in this collection",
        ),
        DocumentsError::TooLarge => error_response(
            StatusCode::PAYLOAD_TOO_LARGE,
            "too_large",
            "the document is larger than the serving limit",
        ),
        DocumentsError::Io(error) => {
            tracing::error!(%error, "cannot read a document");
            internal_error()
        }
    }
}

fn internal_error() -> Response {
    error_response(
        StatusCode::INTERNAL_SERVER_ERROR,
        "internal",
        "an internal error occurred",
    )
}
