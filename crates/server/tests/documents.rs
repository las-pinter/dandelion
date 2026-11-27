//! Documents endpoint acceptance tests.
//!
//! These build a home directory with a plugin manifest and a collection root,
//! then drive the real router: authentication, the listing, the sandbox
//! headers, plain-text Markdown, path escapes, symlinks, and the size cap.

use std::{
    fs,
    path::{Path, PathBuf},
};

use axum::{
    body::Body,
    http::{Method, Request, StatusCode, header},
    response::Response,
};
use dandelion_documents::Documents;
use dandelion_server::{
    agents::Agents,
    app::build_router,
    audit::AuditLog,
    auth::{
        Auth,
        origin::PublicOrigin,
        session::now_millis,
        token::{hash_token, random_token},
    },
    claude::ClaudeMonitor,
    containers::Containers,
    documents::DOCUMENT_CSP,
    metrics::Metrics,
    services::Services,
    state::AppState,
    storage,
    ws::hub::Hub,
};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use tempfile::TempDir;
use tower::ServiceExt;

const ORIGIN: &str = "https://example.test";
const COOKIE: &str = "__Host-dandelion_session";

/// A home directory, its plugin folder, and a collection root.
struct Fixture {
    _temp: TempDir,
    home: PathBuf,
    root: PathBuf,
    plugins: PathBuf,
}

fn fixture() -> Fixture {
    let temp = TempDir::new().expect("a temp dir");
    let home = temp.path().join("home");
    let root = home.join("notes");
    let plugins = home.join(".config/dandelion/plugins");
    fs::create_dir_all(&root).expect("the collection root");
    fs::create_dir_all(&plugins).expect("the plugin folder");
    let fixture = Fixture {
        _temp: temp,
        home,
        root,
        plugins,
    };
    fs::write(
        fixture.plugins.join("notes.json"),
        json!({
            "schema": 1,
            "id": "notes",
            "name": "Notes",
            "root": fixture.root.to_string_lossy(),
            "include": ["**/*.{html,md}"],
            "group": [],
        })
        .to_string(),
    )
    .expect("the manifest");
    fixture
}

fn write(fixture: &Fixture, relative: &str, body: &[u8]) -> PathBuf {
    let path = fixture.root.join(relative);
    fs::create_dir_all(path.parent().expect("a parent")).expect("the tree");
    fs::write(&path, body).expect("the file");
    path
}

async fn state_with(documents: Documents) -> AppState {
    let pool = storage::open_in_memory().await.expect("in-memory database");
    let origin = PublicOrigin::parse(ORIGIN).expect("test origin");
    let auth = Auth::new(pool.clone(), origin).expect("auth services");

    AppState::new(
        AuditLog::new(pool.clone()),
        auth,
        Hub::default(),
        Metrics::new(pool),
        Services::disabled(),
        Containers::disabled(),
        Agents::disabled(),
        ClaudeMonitor::disabled(),
        documents,
    )
}

async fn session_cookie(state: &AppState) -> String {
    let now = now_millis();
    let owner = state.auth.store().ensure_owner(now).await.expect("owner");
    let token = random_token().expect("session token");
    state
        .auth
        .store()
        .insert_session(&hash_token(&token), &owner.id, now, now + 3_600_000)
        .await
        .expect("session");

    format!("{COOKIE}={token}")
}

async fn get(state: &AppState, uri: &str, cookie: Option<&str>) -> Response {
    let mut builder = Request::builder().method(Method::GET).uri(uri);
    if let Some(cookie) = cookie {
        builder = builder.header(header::COOKIE, cookie);
    }

    build_router(state.clone())
        .oneshot(builder.body(Body::empty()).expect("request"))
        .await
        .expect("response")
}

fn header_text(response: &Response, name: header::HeaderName) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

async fn body_bytes(response: Response) -> Vec<u8> {
    response
        .into_body()
        .collect()
        .await
        .expect("body")
        .to_bytes()
        .to_vec()
}

#[tokio::test]
async fn documents_require_a_session() {
    let fixture = fixture();
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;

    let response = get(&state, "/api/documents", None).await;

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn the_listing_serves_collections_and_manifest_warnings() {
    let fixture = fixture();
    write(&fixture, "d/one-pager.html", b"<p>hi</p>");
    fs::write(fixture.plugins.join("broken.json"), "{ not json").expect("a bad manifest");
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;
    let cookie = session_cookie(&state).await;

    let response = get(&state, "/api/documents", Some(&cookie)).await;

    assert_eq!(response.status(), StatusCode::OK);
    let body: Value = serde_json::from_slice(&body_bytes(response).await).expect("JSON body");
    assert_eq!(body["collections"][0]["id"], "notes");
    assert_eq!(
        body["collections"][0]["groups"][0]["documents"][0]["path"],
        "d/one-pager.html"
    );
    assert_eq!(body["warnings"][0]["file"], "broken.json");
}

#[tokio::test]
async fn an_html_document_answers_in_an_opaque_origin_sandbox() {
    let fixture = fixture();
    write(&fixture, "d/one-pager.html", b"<!doctype html><p>hi</p>");
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;
    let cookie = session_cookie(&state).await;

    let response = get(
        &state,
        "/api/documents/notes/file?path=d%2Fone-pager.html",
        Some(&cookie),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_text(&response, header::CONTENT_TYPE).as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(
        header_text(&response, header::CONTENT_SECURITY_POLICY).as_deref(),
        Some(DOCUMENT_CSP)
    );
    assert!(DOCUMENT_CSP.contains("sandbox allow-scripts"));
    assert!(!DOCUMENT_CSP.contains("allow-same-origin"));
    assert_eq!(
        header_text(&response, header::X_CONTENT_TYPE_OPTIONS).as_deref(),
        Some("nosniff")
    );
    assert_eq!(
        header_text(&response, header::REFERRER_POLICY).as_deref(),
        Some("no-referrer")
    );
    assert_eq!(body_bytes(response).await, b"<!doctype html><p>hi</p>");
}

#[tokio::test]
async fn a_markdown_document_is_plain_text_without_the_sandbox() {
    let fixture = fixture();
    write(&fixture, "d/notes.md", b"# title");
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;
    let cookie = session_cookie(&state).await;

    let response = get(
        &state,
        "/api/documents/notes/file?path=d%2Fnotes.md",
        Some(&cookie),
    )
    .await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_text(&response, header::CONTENT_TYPE).as_deref(),
        Some("text/plain; charset=utf-8")
    );
    assert!(
        header_text(&response, header::CONTENT_SECURITY_POLICY).is_none(),
        "a fetched text file needs no sandbox"
    );
    assert_eq!(body_bytes(response).await, b"# title");
}

#[tokio::test]
async fn escapes_symlinks_and_files_outside_the_listing_answer_not_found() {
    let fixture = fixture();
    write(&fixture, "d/notes.md", b"# title");
    fs::write(fixture.home.join("secret.md"), b"secret").expect("the secret");
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        fixture.home.join("secret.md"),
        fixture.root.join("d/link.md"),
    )
    .expect("the symlink");
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;
    let cookie = session_cookie(&state).await;

    for path in [
        "..%2Fsecret.md",
        "%2E%2E%2Fsecret.md",
        "%2Fetc%2Fpasswd",
        "d%2Flink.md",
        "d%2Fnotes.txt",
    ] {
        let response = get(
            &state,
            &format!("/api/documents/notes/file?path={path}"),
            Some(&cookie),
        )
        .await;
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "{path} must be refused"
        );
    }
    let unknown = get(
        &state,
        "/api/documents/missing/file?path=d%2Fnotes.md",
        Some(&cookie),
    )
    .await;
    assert_eq!(unknown.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn an_oversized_document_answers_too_large() {
    let fixture = fixture();
    let path = Path::new(&write(&fixture, "big.md", b"")).to_path_buf();
    fs::File::create(&path)
        .expect("the file")
        .set_len(8 * 1024 * 1024 + 1)
        .expect("a sparse file");
    let state = state_with(Documents::new(
        fixture.plugins.clone(),
        fixture.home.clone(),
    ))
    .await;
    let cookie = session_cookie(&state).await;

    let response = get(
        &state,
        "/api/documents/notes/file?path=big.md",
        Some(&cookie),
    )
    .await;

    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}
