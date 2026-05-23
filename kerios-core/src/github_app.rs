//! GitHub App installation auth for git.
//!
//! Three primitives:
//! 1. [`sign_jwt`] — RS256 JWT signed with the App's RSA private key.
//!    The JWT proves the caller is the App.
//! 2. [`fetch_installation_token`] — exchanges the JWT against
//!    `POST /app/installations/<id>/access_tokens` for a short-lived
//!    (~1 hour) installation access token. The token can act on the
//!    repos the installation was granted.
//! 3. [`https_url_with_token`] — rewrites
//!    `https://github.com/org/repo.git` to
//!    `https://x-access-token:<token>@github.com/org/repo.git` for the
//!    duration of a single `git fetch`.
//!
//! P4a lives in `kerios-core`; the daemon-side wiring (token caching,
//! URL rewriting on each tick) lands in P4b under `kerios::commands`.

use std::time::Duration;

use chrono::Utc;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};

/// JWT claims for a GitHub App. Issued by the App for itself, audience
/// is GitHub. Lifetime capped at 10 minutes per GitHub policy; we use 9
/// to absorb clock skew.
#[derive(Debug, Serialize, Deserialize)]
struct AppClaims {
    /// Issued-at (Unix seconds). GitHub recommends a 60 s buffer
    /// backwards to compensate for clock drift on the issuer.
    iat: i64,
    /// Expiration (Unix seconds).
    exp: i64,
    /// The App's numeric ID.
    iss: String,
}

/// Mint a GitHub App JWT signed with `private_key_pem`.
///
/// `now_unix_secs` is the current Unix time; passed in so callers can
/// freeze the clock in tests. `iat` is `now - 60` (clock-skew buffer);
/// `exp` is `now + 540` (9 minutes, under GitHub's 10-minute cap).
///
/// # Errors
/// Returns [`GitHubAppError::SignJwt`] if the PEM is unreadable or the
/// key type is not supported.
pub fn sign_jwt(
    app_id: &str,
    private_key_pem: &[u8],
    now_unix_secs: i64,
) -> Result<String, GitHubAppError> {
    let key = EncodingKey::from_rsa_pem(private_key_pem)
        .map_err(|e| GitHubAppError::SignJwt(format!("decoding RSA private key: {e}")))?;
    let claims = AppClaims {
        iat: now_unix_secs - 60,
        exp: now_unix_secs + 540,
        iss: app_id.to_string(),
    };
    encode(&Header::new(Algorithm::RS256), &claims, &key)
        .map_err(|e| GitHubAppError::SignJwt(e.to_string()))
}

/// Response from `POST /app/installations/<id>/access_tokens`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstallationToken {
    pub token: String,
    /// RFC3339 timestamp.
    pub expires_at: String,
}

impl InstallationToken {
    /// Returns `expires_at` parsed into a Unix timestamp. Returns `None`
    /// if the field cannot be parsed.
    #[must_use]
    pub fn expires_at_unix(&self) -> Option<i64> {
        chrono::DateTime::parse_from_rfc3339(&self.expires_at)
            .ok()
            .map(|d| d.timestamp())
    }

    /// True if the token expires within `slack_secs` seconds from `now_unix_secs`.
    #[must_use]
    pub fn is_near_expiry(&self, now_unix_secs: i64, slack_secs: i64) -> bool {
        match self.expires_at_unix() {
            Some(t) => t - now_unix_secs <= slack_secs,
            None => true,
        }
    }
}

const GITHUB_API: &str = "https://api.github.com";

/// Exchange a freshly-minted App JWT for an installation access token.
///
/// # Errors
/// Returns [`GitHubAppError::Http`] on network failure or non-2xx,
/// [`GitHubAppError::Parse`] if the response body is not the expected JSON.
pub fn fetch_installation_token(
    jwt: &str,
    installation_id: &str,
) -> Result<InstallationToken, GitHubAppError> {
    fetch_installation_token_against(GITHUB_API, jwt, installation_id)
}

/// Same as [`fetch_installation_token`] but lets the test inject a base URL.
/// Exposed because GitHub's API endpoint is hard-coded and we mock it in tests.
///
/// # Errors
/// See [`fetch_installation_token`].
pub fn fetch_installation_token_against(
    api_base: &str,
    jwt: &str,
    installation_id: &str,
) -> Result<InstallationToken, GitHubAppError> {
    let url = format!("{api_base}/app/installations/{installation_id}/access_tokens");
    let resp = ureq::post(&url)
        .timeout(Duration::from_secs(15))
        .set("Authorization", &format!("Bearer {jwt}"))
        .set("Accept", "application/vnd.github+json")
        .set("X-GitHub-Api-Version", "2022-11-28")
        .set("User-Agent", "kerios")
        .call()
        .map_err(|e| GitHubAppError::Http(e.to_string()))?;
    let body = resp
        .into_string()
        .map_err(|e| GitHubAppError::Http(e.to_string()))?;
    serde_json::from_str(&body).map_err(|e| GitHubAppError::Parse(e.to_string()))
}

/// Rewrite an HTTPS git remote URL with an installation token embedded as
/// HTTP basic auth using GitHub's special `x-access-token` username.
/// Falls back to returning the URL unchanged if it does not look like an
/// `https://` URL.
///
/// The result is suitable for `git clone <result>` or as a one-shot
/// `git -c http.extraheader="Authorization: ..."` substitute.
#[must_use]
pub fn https_url_with_token(repo_url: &str, token: &str) -> String {
    if let Some(rest) = repo_url.strip_prefix("https://") {
        return format!("https://x-access-token:{token}@{rest}");
    }
    repo_url.to_string()
}

/// Errors produced by the GitHub App auth helpers.
#[derive(Debug, thiserror::Error)]
pub enum GitHubAppError {
    #[error("could not sign JWT: {0}")]
    SignJwt(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("could not parse GitHub response: {0}")]
    Parse(String),
}

/// Convenience: current Unix time. Wrap to keep tests deterministic by
/// avoiding `std::time::SystemTime` calls scattered across the module.
#[must_use]
pub fn now_unix() -> i64 {
    Utc::now().timestamp()
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{decode, DecodingKey, Validation};

    // Hard-coded 2048-bit RSA key for tests. NEVER use in production.
    const TEST_PRIVATE_KEY_PEM: &[u8] = include_bytes!("../tests/fixtures/test_rsa_2048.pem");
    const TEST_PUBLIC_KEY_PEM: &[u8] = include_bytes!("../tests/fixtures/test_rsa_2048.pub.pem");

    #[derive(Debug, Deserialize)]
    struct TestClaims {
        iat: i64,
        exp: i64,
        iss: String,
    }

    #[test]
    fn sign_jwt_produces_a_valid_rs256_jwt_with_expected_claims() {
        let now = 1_700_000_000;
        let jwt = sign_jwt("123456", TEST_PRIVATE_KEY_PEM, now).unwrap();

        // Three base64 chunks separated by dots.
        assert_eq!(jwt.matches('.').count(), 2);

        let mut validation = Validation::new(Algorithm::RS256);
        validation.validate_exp = false;
        validation.required_spec_claims.clear();
        let decoded = decode::<TestClaims>(
            &jwt,
            &DecodingKey::from_rsa_pem(TEST_PUBLIC_KEY_PEM).unwrap(),
            &validation,
        )
        .unwrap();
        assert_eq!(decoded.claims.iss, "123456");
        assert_eq!(decoded.claims.iat, now - 60);
        assert_eq!(decoded.claims.exp, now + 540);
    }

    #[test]
    fn sign_jwt_errors_on_garbage_pem() {
        let result = sign_jwt("1", b"not a pem", 0);
        assert!(matches!(result, Err(GitHubAppError::SignJwt(_))));
    }

    #[test]
    fn installation_token_near_expiry() {
        let t = InstallationToken {
            token: "ghs_xxx".into(),
            expires_at: "2026-05-21T10:00:00Z".into(),
        };
        let expiry = t.expires_at_unix().unwrap();
        assert!(t.is_near_expiry(expiry - 30, 60), "30 s before expiry");
        assert!(!t.is_near_expiry(expiry - 600, 60), "10 min before expiry");
    }

    #[test]
    fn https_url_with_token_rewrites_https_only() {
        assert_eq!(
            https_url_with_token("https://github.com/acme/repo.git", "ghs_T0K3N"),
            "https://x-access-token:ghs_T0K3N@github.com/acme/repo.git"
        );
        // ssh url unchanged
        assert_eq!(
            https_url_with_token("git@github.com:acme/repo.git", "ghs_T0K3N"),
            "git@github.com:acme/repo.git"
        );
    }

    #[test]
    fn fetch_installation_token_parses_a_mock_response() {
        use std::io::{BufRead, BufReader, Write};
        use std::net::TcpListener;
        use std::sync::mpsc;
        use std::thread;

        // `bind("127.0.0.1:0")` is already synchronous: the OS has the
        // socket listening before we read `local_addr()`. The mpsc here
        // signals when the accept loop has *entered* — eliminating the
        // last sliver of race with the test's connect() call.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let (ready_tx, ready_rx) = mpsc::channel::<()>();

        thread::spawn(move || {
            ready_tx.send(()).unwrap();
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 || line == "\r\n" {
                    break;
                }
            }
            let body = r#"{"token":"ghs_FAKE","expires_at":"2026-05-21T11:00:00Z"}"#;
            let response = format!(
                "HTTP/1.1 201 Created\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).unwrap();
        });
        ready_rx.recv().unwrap();

        let base = format!("http://127.0.0.1:{port}");
        let token = fetch_installation_token_against(&base, "fake-jwt", "789").unwrap();
        assert_eq!(token.token, "ghs_FAKE");
        assert_eq!(token.expires_at, "2026-05-21T11:00:00Z");
    }
}
