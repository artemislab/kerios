# Security and Threat Model

This document describes what Kerios protects, what it does NOT protect, and how secrets are handled at rest. It is the source of truth — if the code disagrees, that is a bug.

## Reporting a vulnerability

Please email **security@artemislab.io**. Do not open a public GitHub issue for security reports. We acknowledge within 5 business days.

---

## What Kerios does with secrets

The OSS build's only secret is the SSH private key used to clone / pull the config repo. Two configuration paths:

1. **`[auth].ssh_key_path`** — a path to an existing key file. Devops (or the user) provisions it once; Kerios reads it via `GIT_SSH_COMMAND='ssh -i <path>'`.
2. **`[auth].secret_url`** — a URL (`https://` or `gs://`) fetched **once at `kerios enroll`**. The bytes are written to `~/.kerios/secrets/ssh_key`, mode `0600`, and `ssh_key_path` is set to that path in the saved config. The `secret_url` is then **dropped** from the persisted config; only the on-disk reference remains.

Both end up the same way: an SSH key at a path on disk, owned by the user running the daemon, mode `0600`.

## At-rest protection model

| Layer | Mechanism | Defends against |
|-------|-----------|-----------------|
| Kerios | File mode `0600` on `~/.kerios/{config.toml,secrets/*}` | Other unprivileged users on the same machine; accidental world-readable copies |
| OS | FileVault (macOS) / LUKS / dm-crypt (Linux) / BitLocker (Windows) | Lost / stolen laptop with disk pulled and read offline |

That's it. **Kerios does not implement application-level encryption-at-rest.** Adding application-level AES with the decryption key stored next to the ciphertext (e.g. in another mode-0600 file under `~/.kerios/`) provides no meaningful additional protection — an attacker who can read one can read the other. We chose to be honest about this rather than ship security theater.

A future release may integrate with the OS-native secret stores (macOS Keychain Services, Linux Secret Service via `gnome-keyring` / `kwallet`, Windows Credential Manager) to provide real process-isolated secret access. The `[auth].ssh_key_in_keychain` field is reserved in the schema for that purpose but is **not implemented yet**. See the "Roadmap" section below.

## In-transit protection

| Channel | Protection |
|---------|------------|
| `kerios enroll` fetch of `bootstrap.toml` | TLS via `ureq` + `rustls` (no OpenSSL). Cert pinning is **not** done — the daemon trusts the system root store. |
| `kerios enroll` fetch of `secret_url` (https) | Same as above |
| `kerios enroll` fetch of `secret_url` (gs://) | Whatever `gsutil` does (Google Cloud auth + TLS to `storage.googleapis.com`) |
| Daemon ↔ git remote | Whatever the git remote and `ssh` / `https` do — Kerios shells out to the system `git` binary and inherits its trust chain |

## What Kerios does NOT protect against

Listed explicitly so we are not implicitly claiming more than we deliver.

- **Root or `sudo` on the developer machine.** Anyone with root reads any file Kerios touches.
- **Compromised user process.** Code running as the user can read `~/.kerios/secrets/` directly.
- **Compromised config source.** If an attacker pushes to the git repo Kerios pulls from, every connected agent picks up the change at the next tick. Source-side protection (branch protection, code review, signed commits) is the operator's job, not Kerios's.
- **Compromised `secret_url` endpoint.** Same as above for the bootstrap blob.
- **Side-channel attacks** (timing, cache, etc.). Not in scope.
- **Supply chain on the binary itself.** Mitigated by reproducible builds (planned) and SHA-256 sums on each release artifact (already shipped — see the `*.sha256` files alongside each `.tar.gz` on the Releases page).

## Privacy

- The OSS daemon **never calls home**. Outbound network traffic is exclusively: (a) the `git pull` against the configured source, and (b) the one-shot `secret_url` fetch during `kerios enroll`. No telemetry, no usage reports, no analytics.
- File contents under `~/.claude/`, `~/.codex/` etc. are **read once** to detect drift (SHA-256 hash compared to last applied) and written when the bundle changes them. They never leave the machine.

## Roadmap (security-relevant items)

These are tracked but not implemented:

1. **OS keychain integration** for `[auth].ssh_key_in_keychain`.
   - macOS Keychain Services: high value, mature.
   - Linux Secret Service: only works on machines with a running keyring daemon (gnome-keyring, kwallet); not useful on headless servers.
   - Windows Credential Manager: similar story.
2. **Cert pinning for the bootstrap endpoint** so a compromised root CA cannot serve a malicious `bootstrap.toml`.
3. **Reproducible builds** end-to-end so the Release artifacts can be verified against source.
4. **Signed bundles** — the agent would verify a signature on the merged bundle before writing it to disk. Today, signed git commits + branch protection on the source repo are the recommended substitute.

## Why this document exists

A clear and honest threat model is more valuable than a list of cryptographic primitives. If you operate Kerios in an environment where the items above are insufficient, please file an issue describing the gap. We would rather close the doc gap than ship code that does not actually protect what it claims to protect.
