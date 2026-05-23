//! E2E tests for `kerios enroll`.
//!
//! A tiny in-process HTTP server serves a `bootstrap.toml`; the test
//! spawns the real `kerios` binary with `HOME=tempdir` and asserts the
//! resulting `~/.kerios/config.toml`.

use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

fn kerios_binary() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_kerios"))
}

/// Stand up a small HTTP server that maps `path -> body` for `n_requests`
/// total, then exits. Returns the base URL (`http://127.0.0.1:PORT`).
///
/// The mpsc channel signals when the server is inside `accept()` — that
/// eliminates the timing race between this function returning and the
/// test's first request, without `thread::sleep`.
fn serve_paths(routes: Vec<(&'static str, &'static str)>, n_requests: usize) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let base = format!("http://127.0.0.1:{port}");
    let (ready_tx, ready_rx) = mpsc::channel::<()>();

    thread::spawn(move || {
        ready_tx.send(()).unwrap();
        for _ in 0..n_requests {
            let (mut stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut request_line = String::new();
            reader.read_line(&mut request_line).unwrap();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap() == 0 {
                    break;
                }
                if line == "\r\n" {
                    break;
                }
            }
            let path = request_line.split_whitespace().nth(1).unwrap_or("/");
            let body = routes
                .iter()
                .find_map(|(p, b)| if *p == path { Some(*b) } else { None })
                .unwrap_or("");
            let status = if body.is_empty() {
                "404 Not Found"
            } else {
                "200 OK"
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{body}",
                body.len(),
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    ready_rx.recv().unwrap();
    base
}

fn serve_once(body: &'static str) -> String {
    let base = serve_paths(vec![("/bootstrap.toml", body)], 1);
    format!("{base}/bootstrap.toml")
}

#[test]
fn enroll_fetches_bootstrap_and_writes_config() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let body = r#"
[source]
type = "git"
repo_url = "git@github.com:acme/cfg.git"
cache_dir = "/var/cache/kerios"

[sync]
interval_secs = 30
drift_policy = "enforce"
"#;
    let url = serve_once(body);

    let output = Command::new(kerios_binary())
        .arg("enroll")
        .arg(&url)
        .arg("--team")
        .arg("backend")
        .arg("--user")
        .arg("alice")
        .env("HOME", &home)
        .output()
        .expect("running kerios enroll");

    assert!(
        output.status.success(),
        "enroll failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let config_path = home.join(".kerios/config.toml");
    let written = std::fs::read_to_string(&config_path).unwrap();
    assert!(
        written.contains("git@github.com:acme/cfg.git"),
        "got: {written}"
    );
    assert!(written.contains("interval_secs = 30"), "got: {written}");
    assert!(
        written.contains("drift_policy = \"enforce\""),
        "got: {written}"
    );
    assert!(written.contains("team = \"backend\""), "got: {written}");
    assert!(written.contains("user = \"alice\""), "got: {written}");

    // Should be 0600 on Unix
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&config_path)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600, "config.toml mode is {mode:o}, expected 600");
    }
}

#[test]
fn enroll_fetches_secret_url_and_writes_local_key_file() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let bootstrap_body = r#"
[source]
type = "git"
repo_url = "git@github.com:acme/cfg.git"
cache_dir = "/var/cache/kerios"

[auth]
secret_url = "BASEURL/secret"
"#;
    let secret_body =
        "-----BEGIN OPENSSH PRIVATE KEY-----\nfake-key-bytes\n-----END OPENSSH PRIVATE KEY-----\n";

    // The bootstrap body references the URL of the SECRET server. Bind
    // a first server that will handle the /secret request (one request,
    // exit), discover its base URL, then stamp it into the bootstrap and
    // bind a second server for /bootstrap.toml.
    let secret_base = serve_paths(vec![("/secret", secret_body)], 1);
    let bootstrap_with_url = bootstrap_body.replace("BASEURL", &secret_base);
    let bootstrap_with_url: &'static str = Box::leak(bootstrap_with_url.into_boxed_str());
    let bootstrap_base = serve_paths(vec![("/bootstrap.toml", bootstrap_with_url)], 1);

    let output = Command::new(kerios_binary())
        .arg("enroll")
        .arg(format!("{bootstrap_base}/bootstrap.toml"))
        .arg("--team")
        .arg("backend")
        .env("HOME", &home)
        .output()
        .expect("running kerios enroll");

    assert!(
        output.status.success(),
        "enroll failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // The fetched secret should be on disk.
    let key_path = home.join(".kerios/secrets/ssh_key");
    assert!(key_path.is_file(), "expected {key_path:?} to exist");
    assert_eq!(std::fs::read_to_string(&key_path).unwrap(), secret_body);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&key_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "ssh_key mode is {mode:o}, expected 600");
    }

    // The written config.toml should point ssh_key_path at the local
    // file and MUST NOT carry secret_url through.
    let written = std::fs::read_to_string(home.join(".kerios/config.toml")).unwrap();
    assert!(
        written.contains("ssh_key_path"),
        "config should reference ssh_key_path, got: {written}"
    );
    assert!(
        written.contains(".kerios/secrets/ssh_key"),
        "config should reference the local secret path, got: {written}"
    );
    assert!(
        !written.contains("secret_url"),
        "secret_url must not leak into saved config, got: {written}"
    );
}

#[test]
fn enroll_fetches_github_app_private_key_from_secret_url() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let bootstrap_body = r#"
[source]
type = "git"
repo_url = "https://github.com/acme/cfg.git"
cache_dir = "/var/cache/kerios"

[auth]
[auth.github_app]
app_id = "123456"
installation_id = "78901234"
private_key_secret_url = "BASEURL/app.pem"
"#;
    let pem_body = "-----BEGIN PRIVATE KEY-----\nfake-rsa-bytes\n-----END PRIVATE KEY-----\n";

    let pem_base = serve_paths(vec![("/app.pem", pem_body)], 1);
    let bootstrap_with_url = bootstrap_body.replace("BASEURL", &pem_base);
    let bootstrap_with_url: &'static str = Box::leak(bootstrap_with_url.into_boxed_str());
    let bootstrap_base = serve_paths(vec![("/bootstrap.toml", bootstrap_with_url)], 1);

    let output = Command::new(kerios_binary())
        .arg("enroll")
        .arg(format!("{bootstrap_base}/bootstrap.toml"))
        .arg("--team")
        .arg("backend")
        .env("HOME", &home)
        .output()
        .expect("running kerios enroll");

    assert!(
        output.status.success(),
        "enroll failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // PEM landed on disk where the daemon will read it.
    let pem_path = home.join(".kerios/secrets/github-app.pem");
    assert!(pem_path.is_file(), "expected {pem_path:?} to exist");
    assert_eq!(std::fs::read_to_string(&pem_path).unwrap(), pem_body);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(&pem_path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600);
    }

    // Saved config references the local PEM and does NOT carry the secret URL.
    let written = std::fs::read_to_string(home.join(".kerios/config.toml")).unwrap();
    assert!(written.contains("[auth.github_app]"), "got: {written}");
    assert!(written.contains("app_id = \"123456\""), "got: {written}");
    assert!(
        written.contains("installation_id = \"78901234\""),
        "got: {written}"
    );
    assert!(
        written.contains("private_key_path"),
        "config should reference private_key_path, got: {written}"
    );
    assert!(
        written.contains(".kerios/secrets/github-app.pem"),
        "config should point at local PEM, got: {written}"
    );
    assert!(
        !written.contains("private_key_secret_url"),
        "secret URL must not leak into saved config, got: {written}"
    );
}

#[test]
fn enroll_refuses_to_overwrite_existing_config_without_force() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".kerios")).unwrap();
    std::fs::write(home.join(".kerios/config.toml"), "# existing\n").unwrap();

    let url = serve_once("[source]\ntype = \"git\"\nrepo_url = \"x\"\ncache_dir = \"y\"\n");

    let output = Command::new(kerios_binary())
        .arg("enroll")
        .arg(&url)
        .env("HOME", &home)
        .output()
        .unwrap();

    assert!(!output.status.success(), "should refuse without --force");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("already exists"), "stderr was: {stderr}");
    assert!(stderr.contains("--force"), "stderr was: {stderr}");

    // Original file untouched
    let still = std::fs::read_to_string(home.join(".kerios/config.toml")).unwrap();
    assert_eq!(still, "# existing\n");
}

#[test]
fn enroll_with_unsupported_scheme_fails_fast() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();

    let output = Command::new(kerios_binary())
        .arg("enroll")
        .arg("gs://acme/bootstrap.toml")
        .env("HOME", &home)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("unsupported URL scheme"),
        "stderr was: {stderr}"
    );

    // No config file should have been written
    assert!(!home.join(".kerios/config.toml").exists());

    // Silence unused-variable warning from mpsc (kept for symmetry with
    // potential future tests that wait for server readiness).
    let (_, _) = mpsc::channel::<()>();
}
