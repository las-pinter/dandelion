//! The embedded web application.
//!
//! `just build` writes the Vite bundle to `web/dist` before the Rust build, so
//! release binaries embed the UI. In debug builds `rust-embed` reads the files
//! from disk, and `allow_missing` keeps the crate compiling when the bundle
//! has not been built yet.

use axum::{
    body::Body,
    http::{StatusCode, Uri, header},
    response::Response,
};
use rust_embed::RustEmbed;

const INDEX: &str = "index.html";

#[derive(RustEmbed)]
#[folder = "../../web/dist/"]
#[allow_missing = true]
struct Assets;

/// Whether the web application is embedded in this binary.
#[must_use]
pub fn bundle_is_embedded() -> bool {
    Assets::get(INDEX).is_some()
}

/// Serves a bundled asset, or `index.html` for client-side routes.
#[allow(clippy::needless_pass_by_value, clippy::unused_async)]
pub async fn spa(uri: Uri) -> Response<Body> {
    let Some(path) = spa_path(uri.path(), |candidate| Assets::get(candidate).is_some()) else {
        return not_built();
    };

    asset_response(path).unwrap_or_else(not_built)
}

/// Chooses the file to serve for a request path.
///
/// `index.html` answers unknown paths so the client-side router owns them.
fn spa_path(requested: &str, exists: impl Fn(&str) -> bool) -> Option<&str> {
    let requested = requested.trim_start_matches('/');
    let requested = if requested.is_empty() {
        INDEX
    } else {
        requested
    };

    if exists(requested) {
        return Some(requested);
    }

    (requested != INDEX && exists(INDEX)).then_some(INDEX)
}

fn asset_response(path: &str) -> Option<Response<Body>> {
    let asset = Assets::get(path)?;
    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .header(header::CACHE_CONTROL, cache_control(path))
        .body(Body::from(asset.data.into_owned()))
        .ok()
}

/// Vite content-hashes the files under `assets/`, so only they may be cached
/// forever. Everything else (`index.html`, `favicon.svg`, future manifests and
/// service workers) must revalidate.
fn cache_control(path: &str) -> &'static str {
    if path.starts_with("assets/") {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    }
}

fn not_built() -> Response<Body> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CONTENT_TYPE, "text/plain; charset=utf-8")
        .body(Body::from(
            "web UI not built; run `just build` or use the Vite dev server\n",
        ))
        .expect("the static response builder is valid")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{Assets, INDEX, cache_control, spa_path};

    #[test]
    fn serves_an_existing_asset() {
        let chosen = spa_path("/assets/app.js", |candidate| candidate == "assets/app.js");

        assert_eq!(chosen, Some("assets/app.js"));
    }

    #[test]
    fn falls_back_to_index_for_a_client_route() {
        let chosen = spa_path("/dashboard", |candidate| candidate == INDEX);

        assert_eq!(chosen, Some(INDEX));
    }

    #[test]
    fn root_serves_index() {
        let chosen = spa_path("/", |candidate| candidate == INDEX);

        assert_eq!(chosen, Some(INDEX));
    }

    #[test]
    fn returns_none_when_the_bundle_is_missing() {
        let chosen = spa_path("/dashboard", |_| false);

        assert_eq!(chosen, None);
    }

    #[test]
    fn content_hashed_assets_are_immutable() {
        assert_eq!(
            cache_control("assets/index-abc123.js"),
            "public, max-age=31536000, immutable"
        );
    }

    #[test]
    fn top_level_files_revalidate() {
        assert_eq!(cache_control(INDEX), "no-cache");
        assert_eq!(cache_control("favicon.svg"), "no-cache");
    }

    #[test]
    fn embeds_the_bundle_when_it_is_present() {
        let index = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../web/dist/index.html");
        if index.is_file() {
            assert!(
                Assets::get(INDEX).is_some(),
                "web/dist is present but the embedded bundle is empty"
            );
        }
    }
}
