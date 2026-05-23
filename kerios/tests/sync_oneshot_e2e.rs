//! E2E tests for `kerios sync` (the one-shot subcommand).
//!
//! Per the project's testing rule ("E2E tests must simulate a real user"),
//! these tests build the actual binary and invoke it as a subprocess
//! against a real on-disk git repo and a real tempdir HOME. No mocks.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn run_git(cwd: &Path, args: &[&str]) {
    let st = Command::new("git")
        .current_dir(cwd)
        .args(args)
        .status()
        .unwrap();
    assert!(st.success(), "git {args:?} failed");
}

fn kerios_binary() -> std::path::PathBuf {
    // Cargo sets CARGO_BIN_EXE_<name> for integration tests of the binary.
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_kerios"))
}

#[test]
fn kerios_sync_writes_bundled_files_into_provider_dirs() {
    let tmp = tempfile::tempdir().unwrap();

    // Seed an upstream bare repo with a Claude agent + a Codex config.
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    run_git(&work, &["init", "-q", "-b", "main"]);
    let claude_path = work.join("org/claude/agents/security.md");
    std::fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
    std::fs::write(&claude_path, "the rule").unwrap();
    let codex_path = work.join("org/codex/config.toml");
    std::fs::create_dir_all(codex_path.parent().unwrap()).unwrap();
    std::fs::write(&codex_path, "model = \"o1\"\n").unwrap();
    run_git(&work, &["add", "."]);
    run_git(
        &work,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "seed",
        ],
    );
    let bare = tmp.path().join("upstream.git");
    run_git(
        tmp.path(),
        &[
            "clone",
            "--bare",
            "-q",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );

    // Tempdir HOME with .claude/ pre-existing so Claude::detect returns true.
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".kerios")).unwrap();
    std::fs::create_dir(home.join(".claude")).unwrap();
    std::fs::create_dir(home.join(".codex")).unwrap();
    let config = format!(
        r#"
[source]
type = "git"
repo_url = "file://{}"
cache_dir = "{}"

[sync]
mode = "pull"
interval_secs = 60
"#,
        bare.display(),
        home.join(".kerios/cache").display()
    );
    std::fs::write(home.join(".kerios/config.toml"), config).unwrap();

    // Real user simulation: invoke the actual compiled binary as a subprocess.
    let output = Command::new(kerios_binary())
        .arg("sync")
        .env("HOME", &home)
        .output()
        .expect("running kerios sync");

    assert!(
        output.status.success(),
        "kerios sync exited with {:?}\nstdout: {}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    // The user-facing report on stdout.
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("claude=1"), "stdout was: {stdout}");
    assert!(stdout.contains("codex=1"), "stdout was: {stdout}");

    // The actual files on disk — what a user would check next.
    assert_eq!(
        std::fs::read_to_string(home.join(".claude/agents/security.md")).unwrap(),
        "the rule"
    );
    assert_eq!(
        std::fs::read_to_string(home.join(".codex/config.toml")).unwrap(),
        "model = \"o1\"\n"
    );
}

#[test]
fn kerios_status_reflects_last_sync_outcome() {
    let tmp = tempfile::tempdir().unwrap();
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    run_git(&work, &["init", "-q", "-b", "main"]);
    let p = work.join("org/claude/agents/security.md");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "the rule").unwrap();
    run_git(&work, &["add", "."]);
    run_git(
        &work,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "seed",
        ],
    );
    let bare = tmp.path().join("upstream.git");
    run_git(
        tmp.path(),
        &[
            "clone",
            "--bare",
            "-q",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );

    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".kerios")).unwrap();
    std::fs::create_dir(home.join(".claude")).unwrap();
    let config = format!(
        r#"
[source]
type = "git"
repo_url = "file://{}"
cache_dir = "{}"
"#,
        bare.display(),
        home.join(".kerios/cache").display()
    );
    std::fs::write(home.join(".kerios/config.toml"), config).unwrap();

    // 1) Before any sync: status should report "never".
    let status1 = Command::new(kerios_binary())
        .arg("status")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(status1.status.success());
    let s1 = String::from_utf8_lossy(&status1.stdout);
    assert!(s1.contains("last sync:           (never)"), "got: {s1}");

    // 2) Run one sync.
    let sync = Command::new(kerios_binary())
        .arg("sync")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(sync.status.success(), "sync failed: {sync:?}");

    // 3) After sync: status should report the result.
    let status2 = Command::new(kerios_binary())
        .arg("status")
        .env("HOME", &home)
        .output()
        .unwrap();
    assert!(status2.status.success());
    let s2 = String::from_utf8_lossy(&status2.stdout);
    assert!(s2.contains("last result:         ok"), "got: {s2}");
    assert!(s2.contains("claude     1"), "got: {s2}");
    assert!(s2.contains("last source:         git"), "got: {s2}");

    // 4) The state file is on disk where a user would expect.
    assert!(
        home.join(".kerios/state.toml").is_file(),
        "expected state.toml on disk"
    );
}

#[test]
fn kerios_sync_skips_when_lockfile_is_already_held() {
    use std::sync::mpsc;

    let tmp = tempfile::tempdir().unwrap();

    // Real bare repo so the inner `kerios sync` body would otherwise succeed.
    let work = tmp.path().join("work");
    std::fs::create_dir(&work).unwrap();
    run_git(&work, &["init", "-q", "-b", "main"]);
    let p = work.join("org/claude/agents/x.md");
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(&p, "hello").unwrap();
    run_git(&work, &["add", "."]);
    run_git(
        &work,
        &[
            "-c",
            "user.email=t@t",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "seed",
        ],
    );
    let bare = tmp.path().join("upstream.git");
    run_git(
        tmp.path(),
        &[
            "clone",
            "--bare",
            "-q",
            work.to_str().unwrap(),
            bare.to_str().unwrap(),
        ],
    );

    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".kerios")).unwrap();
    std::fs::create_dir(home.join(".claude")).unwrap();
    let config = format!(
        r#"
[source]
type = "git"
repo_url = "file://{}"
cache_dir = "{}"
"#,
        bare.display(),
        home.join(".kerios/cache").display()
    );
    std::fs::write(home.join(".kerios/config.toml"), config).unwrap();

    // Hold the lock from this thread before invoking the binary.
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(home.join(".kerios/sync.lock"))
        .unwrap();
    let mut lock = fd_lock::RwLock::new(lock_file);
    let _guard = lock.try_write().expect("lock should be free at start");

    // Spawn `kerios sync` in another thread (we can't use blocking
    // `Command::output` here because it would deadlock with us holding
    // the lock — well, our impl uses try_write, so it won't deadlock).
    let kbin = kerios_binary();
    let home_for_thread = home.clone();
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let out = Command::new(kbin)
            .arg("sync")
            .env("HOME", &home_for_thread)
            .output()
            .unwrap();
        tx.send(out).unwrap();
    });
    let out = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("kerios sync should exit quickly when lock is held");

    // sync exits 0 with a "skipped" message and writes NO files.
    assert!(
        out.status.success(),
        "expected ok exit (skip is not an error)"
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("sync.lock held"), "stderr was: {stderr}");
    assert!(
        !home.join(".claude/agents/x.md").exists(),
        "no files should have been written"
    );
}

#[test]
fn kerios_sync_errors_when_no_source_is_configured() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("home");
    std::fs::create_dir_all(home.join(".kerios")).unwrap();
    std::fs::write(home.join(".kerios/config.toml"), "").unwrap();

    let output = Command::new(kerios_binary())
        .arg("sync")
        .env("HOME", &home)
        .output()
        .expect("running kerios sync");

    assert!(!output.status.success(), "expected non-zero exit");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("no source configured"),
        "stderr was: {stderr}"
    );
}
