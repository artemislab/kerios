//! Fetch a secret (the SSH key, today) from a remote URL during
//! `kerios enroll`. The bytes are returned to the caller, which is
//! responsible for writing them to disk under `~/.kerios/secrets/`.
//!
//! P2 supports two schemes:
//! - `https://` — direct GET via ureq + rustls.
//! - `gs://` — shells out to `gsutil cp <url> -`. Uses whatever ambient
//!   Google Cloud auth the host has (ADC, service account, gcloud login).

use std::process::{Command, Stdio};
use std::time::Duration;

/// Fetch the secret at `url`.
///
/// # Errors
/// Returns [`SecretError`] for unsupported schemes, network failures,
/// non-2xx responses, or `gsutil` subprocess failures.
pub fn fetch_secret(url: &str) -> Result<Vec<u8>, SecretError> {
    if url.starts_with("https://") || url.starts_with("http://") {
        fetch_https(url)
    } else if url.starts_with("gs://") {
        fetch_gs(url, "gsutil")
    } else {
        Err(SecretError::UnsupportedScheme(url.to_string()))
    }
}

fn fetch_https(url: &str) -> Result<Vec<u8>, SecretError> {
    let resp = ureq::get(url)
        .timeout(Duration::from_secs(15))
        .call()
        .map_err(|e| SecretError::Http(e.to_string()))?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .read_to_end(&mut bytes)
        .map_err(|e| SecretError::Http(e.to_string()))?;
    Ok(bytes)
}

/// Visible for tests: lets us override the binary name with a fake.
///
/// # Errors
/// Returns [`SecretError::Gsutil`] when the subprocess fails to spawn or
/// exits non-zero.
pub fn fetch_gs(url: &str, gsutil_bin: &str) -> Result<Vec<u8>, SecretError> {
    let out = Command::new(gsutil_bin)
        .args(["cp", url, "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| SecretError::Gsutil(format!("spawning {gsutil_bin}: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
        return Err(SecretError::Gsutil(format!(
            "{gsutil_bin} exited with status {:?}: {stderr}",
            out.status.code()
        )));
    }
    Ok(out.stdout)
}

use std::io::Read;

/// Errors produced by [`fetch_secret`].
#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("unsupported URL scheme: {0} (supported: https://, gs://)")]
    UnsupportedScheme(String),
    #[error("http error: {0}")]
    Http(String),
    #[error("gsutil error: {0}")]
    Gsutil(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsupported_scheme() {
        let result = fetch_secret("s3://bucket/key");
        assert!(matches!(result, Err(SecretError::UnsupportedScheme(_))));
    }

    #[test]
    fn fetch_gs_uses_the_named_binary() {
        // Point at a binary that does not exist so the error message is
        // deterministic and we exercise the spawn-fail path.
        let result = fetch_gs("gs://anything/key", "kerios-fake-gsutil-does-not-exist");
        assert!(matches!(result, Err(SecretError::Gsutil(_))));
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("kerios-fake-gsutil-does-not-exist"),
            "got: {msg}"
        );
    }
}
