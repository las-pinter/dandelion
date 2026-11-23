//! Session cookie format and lifetime.

use super::origin::PublicOrigin;

/// Cookie name for an HTTPS origin. The `__Host-` prefix pins it to the host:
/// the browser refuses it without `Secure`, without `Path=/`, and with any
/// `Domain`.
pub const HTTPS_COOKIE: &str = "__Host-dandelion_session";
/// Cookie name for the loopback development origin, where plain HTTP cannot
/// carry a `Secure` cookie in every browser.
pub const LOOPBACK_COOKIE: &str = "dandelion_session";

/// Sliding session lifetime: 30 days.
pub const SESSION_TTL_MS: i64 = 30 * 24 * 60 * 60 * 1000;
/// How stale last-seen may get before a request rewrites the session.
pub const TOUCH_AFTER_MS: i64 = 60 * 60 * 1000;

/// The cookie name for an origin.
#[must_use]
pub fn cookie_name(origin: &PublicOrigin) -> &'static str {
    if origin.is_loopback() {
        LOOPBACK_COOKIE
    } else {
        HTTPS_COOKIE
    }
}

/// The `Set-Cookie` value that starts a session.
#[must_use]
pub fn set_cookie(origin: &PublicOrigin, token: &str) -> String {
    let name = cookie_name(origin);
    let secure = if origin.is_loopback() { "" } else { "; Secure" };
    format!(
        "{name}={token}; Path=/; HttpOnly{secure}; SameSite=Lax; Max-Age={}",
        SESSION_TTL_MS / 1000
    )
}

/// The `Set-Cookie` value that ends a session in the browser.
#[must_use]
pub fn clear_cookie(origin: &PublicOrigin) -> String {
    let name = cookie_name(origin);
    let secure = if origin.is_loopback() { "" } else { "; Secure" };
    format!("{name}=; Path=/; HttpOnly{secure}; SameSite=Lax; Max-Age=0")
}

/// Reads a session token from a `Cookie` header.
///
/// Only the cookie name of the configured origin is accepted. Honoring the
/// plain loopback name on an HTTPS origin would let a sibling host inject a
/// `Domain`-scoped cookie that shadows the `__Host-` session.
#[must_use]
pub fn token_from_header(expected: &str, header: &str) -> Option<String> {
    for pair in header.split(';') {
        let Some((name, value)) = pair.trim().split_once('=') else {
            continue;
        };
        if name == expected && !value.is_empty() {
            return Some(value.to_owned());
        }
    }
    None
}

/// Unix epoch milliseconds.
#[must_use]
pub fn now_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_millis()).unwrap_or(i64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        HTTPS_COOKIE, LOOPBACK_COOKIE, clear_cookie, cookie_name, set_cookie, token_from_header,
    };
    use crate::auth::origin::PublicOrigin;

    fn origin(raw: &str) -> PublicOrigin {
        PublicOrigin::parse(raw).expect("valid origin")
    }

    #[test]
    fn https_uses_the_host_prefix_and_secure() {
        let cookie = set_cookie(&origin("https://example.test"), "token");

        assert_eq!(cookie_name(&origin("https://example.test")), HTTPS_COOKIE);
        assert!(cookie.starts_with("__Host-dandelion_session=token;"));
        assert!(cookie.contains("; Secure"));
        assert!(cookie.contains("; HttpOnly"));
        assert!(cookie.contains("; SameSite=Lax"));
        assert!(cookie.contains("; Max-Age=2592000"));
    }

    #[test]
    fn loopback_uses_the_plain_name_without_secure() {
        let cookie = set_cookie(&origin("http://localhost:5173"), "token");

        assert_eq!(
            cookie_name(&origin("http://localhost:5173")),
            LOOPBACK_COOKIE
        );
        assert!(cookie.starts_with("dandelion_session=token;"));
        assert!(!cookie.contains("Secure"));
    }

    #[test]
    fn clearing_expires_the_cookie() {
        let cookie = clear_cookie(&origin("https://example.test"));

        assert!(cookie.contains("Max-Age=0"));
        assert!(cookie.contains("__Host-dandelion_session="));
    }

    #[test]
    fn reads_only_the_expected_cookie_name() {
        assert_eq!(
            token_from_header(HTTPS_COOKIE, "a=b; __Host-dandelion_session=one; other=two")
                .as_deref(),
            Some("one")
        );
        assert_eq!(
            token_from_header(HTTPS_COOKIE, "dandelion_session=two"),
            None
        );
        assert_eq!(
            token_from_header(LOOPBACK_COOKIE, "dandelion_session=two").as_deref(),
            Some("two")
        );
        assert_eq!(token_from_header(LOOPBACK_COOKIE, "unrelated=value"), None);
        assert_eq!(
            token_from_header(HTTPS_COOKIE, "__Host-dandelion_session="),
            None
        );
    }
}
