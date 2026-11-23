//! The public origin and `WebAuthn` relying-party identity.
//!
//! Dandelion sits behind `tailscale serve`, so the browser sees the tailnet
//! HTTPS origin while the process listens on loopback. The origin is resolved
//! once at startup and never stored in the repository: an explicit
//! `DANDELION_PUBLIC_URL` wins, otherwise it comes from `tailscale status`.

use std::{env, ffi::OsString, time::Duration};

use thiserror::Error;
use url::{Host, Url};

/// How long to wait for `tailscale status --json`.
const TAILSCALE_TIMEOUT: Duration = Duration::from_secs(10);

/// A validated public origin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublicOrigin {
    url: Url,
}

/// Why an origin was rejected or could not be found.
#[derive(Debug, Error)]
pub enum OriginError {
    /// `DANDELION_PUBLIC_URL` was set but empty.
    #[error("DANDELION_PUBLIC_URL is set but empty")]
    Empty,
    /// The value is not a usable origin.
    #[error("invalid public origin `{value}`: {reason}")]
    Invalid {
        /// The offending value.
        value: String,
        /// Why it was rejected.
        reason: &'static str,
    },
    /// Plain HTTP is allowed only for a loopback development origin.
    #[error("refusing plain-HTTP public origin `{0}`; only http://localhost is allowed")]
    NotHttps(String),
    /// The `tailscale` command could not be run.
    #[error("cannot run `tailscale status --json`: {0}")]
    TailscaleUnavailable(String),
    /// Tailscale answered without a DNS name.
    #[error("tailscale reported no DNS name; set DANDELION_PUBLIC_URL")]
    TailscaleMissingName,
}

impl PublicOrigin {
    /// Parses and validates an origin string.
    pub fn parse(raw: &str) -> Result<Self, OriginError> {
        let url = Url::parse(raw.trim()).map_err(|error| OriginError::Invalid {
            value: raw.to_owned(),
            reason: match error {
                url::ParseError::RelativeUrlWithoutBase => "an absolute URL is required",
                _ => "not a valid URL",
            },
        })?;

        if url.cannot_be_a_base() || url.host_str().is_none() {
            return Err(OriginError::Invalid {
                value: raw.to_owned(),
                reason: "a scheme and host are required",
            });
        }
        if !url.username().is_empty() || url.password().is_some() {
            return Err(OriginError::Invalid {
                value: raw.to_owned(),
                reason: "credentials are not part of an origin",
            });
        }
        if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
            return Err(OriginError::Invalid {
                value: raw.to_owned(),
                reason: "an origin has no path, query, or fragment",
            });
        }

        match url.scheme() {
            "https" => {}
            "http" if is_localhost(&url) => {}
            // RFC 8555/WebAuthn: an RP ID must be a domain, so an IP origin
            // cannot host passkeys. Fail at startup rather than at sign-in.
            "http" if is_loopback(&url) => {
                return Err(OriginError::Invalid {
                    value: raw.to_owned(),
                    reason: "WebAuthn needs a hostname; use http://localhost for local development",
                });
            }
            "http" => return Err(OriginError::NotHttps(raw.to_owned())),
            _ => {
                return Err(OriginError::Invalid {
                    value: raw.to_owned(),
                    reason: "the scheme must be https or http",
                });
            }
        }

        Ok(Self { url })
    }

    /// Resolves the origin from the environment or Tailscale.
    ///
    /// `DANDELION_PUBLIC_URL` wins when set. Otherwise the server asks
    /// `tailscale status --json` for its own DNS name.
    pub async fn resolve() -> Result<Self, OriginError> {
        Self::resolve_with(env::var_os("DANDELION_PUBLIC_URL")).await
    }

    async fn resolve_with(explicit: Option<OsString>) -> Result<Self, OriginError> {
        if let Some(value) = explicit {
            let text = value.to_string_lossy();
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Err(OriginError::Empty);
            }
            return Self::parse(trimmed);
        }

        let command = tokio::process::Command::new("tailscale")
            .args(["status", "--json"])
            .output();
        let output = tokio::time::timeout(TAILSCALE_TIMEOUT, command)
            .await
            .map_err(|_| {
                OriginError::TailscaleUnavailable("timed out waiting for tailscale".to_owned())
            })?
            .map_err(|error| OriginError::TailscaleUnavailable(error.to_string()))?;
        if !output.status.success() {
            return Err(OriginError::TailscaleUnavailable(format!(
                "tailscale exited with {}",
                output.status
            )));
        }

        let document = String::from_utf8_lossy(&output.stdout);
        Self::from_tailscale_json(&document)
    }

    /// Reads `Self.DNSName` from a `tailscale status --json` document.
    pub fn from_tailscale_json(document: &str) -> Result<Self, OriginError> {
        #[derive(serde::Deserialize)]
        struct Status {
            #[serde(rename = "Self")]
            this: Option<Device>,
        }
        #[derive(serde::Deserialize)]
        struct Device {
            #[serde(rename = "DNSName")]
            dns_name: Option<String>,
        }

        let status: Status = serde_json::from_str(document).map_err(|error| {
            OriginError::TailscaleUnavailable(format!("unreadable status JSON: {error}"))
        })?;
        let name = status
            .this
            .and_then(|device| device.dns_name)
            .filter(|name| !name.trim().is_empty())
            .ok_or(OriginError::TailscaleMissingName)?;
        let name = name.trim().trim_end_matches('.');

        Self::parse(&format!("https://{name}"))
    }

    /// The origin as the browser serializes it (`scheme://host[:port]`).
    #[must_use]
    pub fn as_str(&self) -> String {
        normalize(&self.url)
    }

    /// The `WebAuthn` relying-party ID: the origin host.
    #[must_use]
    pub fn rp_id(&self) -> &str {
        self.url.host_str().unwrap_or_default()
    }

    /// Whether this is a loopback development origin.
    #[must_use]
    pub fn is_loopback(&self) -> bool {
        is_loopback(&self.url)
    }

    /// Whether a browser `Origin` header belongs to this deployment.
    ///
    /// A loopback origin also accepts other loopback ports so the Vite dev
    /// server can talk to the API; only local processes can reach the
    /// loopback-only server anyway.
    #[must_use]
    pub fn accepts_origin(&self, candidate: &str) -> bool {
        let Ok(url) = Url::parse(candidate) else {
            return false;
        };
        normalize(&url) == self.as_str()
            || (self.is_loopback() && url.scheme() == "http" && is_loopback(&url))
    }

    /// The origin as a parsed URL for the `WebAuthn` builder.
    #[must_use]
    pub fn url(&self) -> &Url {
        &self.url
    }
}

fn is_localhost(url: &Url) -> bool {
    matches!(url.host(), Some(Host::Domain("localhost")))
}

fn is_loopback(url: &Url) -> bool {
    match url.host() {
        Some(Host::Domain("localhost")) => true,
        Some(Host::Ipv4(address)) => address.is_loopback(),
        Some(Host::Ipv6(address)) => address.is_loopback(),
        _ => false,
    }
}

fn normalize(url: &Url) -> String {
    let host = url.host_str().unwrap_or_default();
    match url.port() {
        Some(port) => format!("{}://{host}:{port}", url.scheme()),
        None => format!("{}://{host}", url.scheme()),
    }
}

#[cfg(test)]
mod tests {
    use super::{OriginError, PublicOrigin};

    fn parse(raw: &str) -> Result<PublicOrigin, OriginError> {
        PublicOrigin::parse(raw)
    }

    #[test]
    fn accepts_an_https_origin() {
        let origin = parse("https://example.test").expect("valid origin");

        assert_eq!(origin.as_str(), "https://example.test");
        assert_eq!(origin.rp_id(), "example.test");
        assert!(!origin.is_loopback());
    }

    #[test]
    fn normalizes_a_trailing_slash() {
        let origin = parse("https://example.test/").expect("valid origin");

        assert_eq!(origin.as_str(), "https://example.test");
    }

    #[test]
    fn keeps_a_non_default_port() {
        let origin = parse("https://example.test:8443").expect("valid origin");

        assert_eq!(origin.as_str(), "https://example.test:8443");
    }

    #[test]
    fn accepts_http_localhost_but_rejects_other_http() {
        assert!(parse("http://localhost:5173").is_ok());
        assert!(matches!(
            parse("http://127.0.0.1:5173"),
            Err(OriginError::Invalid { .. })
        ));
        assert!(matches!(
            parse("http://[::1]:5173"),
            Err(OriginError::Invalid { .. })
        ));
        assert!(matches!(
            parse("http://example.test"),
            Err(OriginError::NotHttps(_))
        ));
    }

    #[test]
    fn rejects_paths_queries_and_credentials() {
        assert!(parse("https://example.test/setup").is_err());
        assert!(parse("https://example.test?token=1").is_err());
        assert!(parse("https://user@example.test").is_err());
    }

    #[tokio::test]
    async fn an_empty_override_is_an_error() {
        use std::ffi::OsString;

        let result = PublicOrigin::resolve_with(Some(OsString::new())).await;

        assert!(matches!(result, Err(OriginError::Empty)));
    }

    #[test]
    fn rejects_other_schemes_and_empty_values() {
        assert!(parse("ftp://example.test").is_err());
        assert!(parse("").is_err());
        assert!(matches!(
            PublicOrigin::parse("   "),
            Err(OriginError::Invalid { .. })
        ));
    }

    #[test]
    fn reads_the_tailscale_dns_name_and_strips_the_dot() {
        let document = r#"{"Self":{"DNSName":"host.example.ts.net."}}"#;

        let origin = PublicOrigin::from_tailscale_json(document).expect("tailscale origin");

        assert_eq!(origin.as_str(), "https://host.example.ts.net");
    }

    #[test]
    fn reports_a_missing_tailscale_name() {
        let document = r#"{"Self":{"DNSName":null}}"#;

        assert!(matches!(
            PublicOrigin::from_tailscale_json(document),
            Err(OriginError::TailscaleMissingName)
        ));
    }

    #[test]
    fn reports_unreadable_tailscale_json() {
        assert!(matches!(
            PublicOrigin::from_tailscale_json("not json"),
            Err(OriginError::TailscaleUnavailable(_))
        ));
    }

    #[test]
    fn accepts_only_matching_or_loopback_origins() {
        let public = parse("https://example.test").expect("valid origin");
        let local = parse("http://localhost:7443").expect("valid origin");

        assert!(public.accepts_origin("https://example.test"));
        assert!(!public.accepts_origin("https://evil.test"));
        assert!(!public.accepts_origin("not a url"));
        assert!(local.accepts_origin("http://localhost:5173"));
        assert!(local.accepts_origin("http://127.0.0.1:5173"));
        assert!(!local.accepts_origin("https://example.test"));
    }
}
