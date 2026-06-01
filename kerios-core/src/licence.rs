//! Ed25519-signed licence wire format: canonical JSON + sign + verify.
//!
//! Generic primitive. Callers supply the `tier` string and the `policy`
//! JSON value and interpret them according to their own domain semantics.
//! This crate is deliberately agnostic about what those fields mean.
//!
//! # Wire format
//!
//! A licence is a JSON object with the fields below plus a `"sig"` field
//! that holds the base64 (standard, padded) encoding of the 64-byte raw
//! Ed25519 signature over the *canonical bytes* of the remaining fields.
//!
//! # Canonical bytes
//!
//! 1. Serialise `LicenceClaims` into a `BTreeMap<String, Value>` (`BTreeMap`
//!    sorts keys lexically, which is deterministic across platforms).
//! 2. Call `serde_json::to_vec(&btreemap)` — minified, no whitespace.
//! 3. The signature covers these bytes. The `"sig"` field is excluded from
//!    the canonical bytes; it is attached to the emitted JSON after signing.

use std::collections::BTreeMap;

use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use ed25519_dalek::{pkcs8::DecodePrivateKey, Signature, Signer, SigningKey, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use time::OffsetDateTime;

// ── Public types ────────────────────────────────────────────────────────────

/// The claims embedded in a licence file.
///
/// All timestamp fields are RFC 3339. The `tier` and `policy` fields are
/// opaque at this layer — downstream consumers interpret them.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenceClaims {
    /// Key identifier. Used by the verifier to look up the right public key.
    pub kid: String,
    /// Unique identifier for the customer (UUID v4 recommended).
    pub customer_id: String,
    /// Contact address for the licence holder.
    pub customer_email: String,
    /// Opaque tier string. The caller decides what values are meaningful.
    pub tier: String,
    /// Optional seat cap. `None` means no enforcement.
    pub seats: Option<u32>,
    /// Timestamp when this licence was issued.
    #[serde(with = "time::serde::rfc3339")]
    pub issued_at: OffsetDateTime,
    /// Earliest moment the licence is valid.
    #[serde(with = "time::serde::rfc3339")]
    pub not_before: OffsetDateTime,
    /// Moment after which the licence is no longer valid.
    #[serde(with = "time::serde::rfc3339")]
    pub valid_until: OffsetDateTime,
    /// Opaque policy blob. The caller decides how to interpret it.
    pub policy: Value,
}

/// Output of a successful [`verify`] call.
#[derive(Debug)]
pub struct VerifiedLicence {
    /// Parsed claims from the licence file.
    pub claims: LicenceClaims,
    /// The `kid` value extracted from the file.
    pub kid: String,
    /// The raw bytes of the licence file as received.
    pub blob_bytes: Vec<u8>,
}

/// Errors produced by [`sign`], [`verify`], and [`canonical_json_bytes`].
#[derive(Debug, thiserror::Error)]
pub enum LicenceError {
    /// The input could not be parsed or the PEM was invalid.
    #[error("malformed licence: {0}")]
    Malformed(String),
    /// The `kid` in the licence has no matching entry in the caller-supplied
    /// public-key list.
    #[error("unknown kid: {0}")]
    UnknownKid(String),
    /// The signature did not verify against the public key for this `kid`.
    #[error("invalid signature")]
    InvalidSignature,
}

// ── Public functions ─────────────────────────────────────────────────────────

/// Produce the canonical JSON bytes for `claims`.
///
/// The bytes are deterministic: keys are sorted lexically and the output is
/// minified (no extra whitespace). These are the exact bytes over which the
/// Ed25519 signature is computed — both `sign` and `verify` must agree on
/// this representation.
///
/// # Errors
///
/// Returns [`LicenceError::Malformed`] if the claims cannot be serialised to
/// an intermediate `BTreeMap` (in practice this should never fail for a
/// well-formed `LicenceClaims`).
pub fn canonical_json_bytes(claims: &LicenceClaims) -> Result<Vec<u8>, LicenceError> {
    // Serialise to Value first, then into a BTreeMap for sorted keys.
    let v = serde_json::to_value(claims)
        .map_err(|e| LicenceError::Malformed(format!("serialise claims: {e}")))?;
    let map: BTreeMap<String, Value> = serde_json::from_value(v)
        .map_err(|e| LicenceError::Malformed(format!("claims to map: {e}")))?;
    serde_json::to_vec(&map).map_err(|e| LicenceError::Malformed(format!("to_vec: {e}")))
}

/// Sign `claims` with the Ed25519 private key given as a PKCS#8 PEM string.
///
/// Returns the complete licence as a JSON string, including the `"sig"` field.
///
/// The caller is responsible for keeping the private key material outside of
/// source code. A typical usage pattern is to read the PEM from a password
/// manager or secret store at operation time, call this function, then
/// securely erase the PEM from memory.
///
/// # Errors
///
/// - [`LicenceError::Malformed`] if `private_key_pem` is not a valid PKCS#8
///   PEM or if claims serialisation fails.
pub fn sign(claims: &LicenceClaims, private_key_pem: &str) -> Result<String, LicenceError> {
    let signing_key = SigningKey::from_pkcs8_pem(private_key_pem)
        .map_err(|e| LicenceError::Malformed(format!("parse private key: {e}")))?;

    let canonical = canonical_json_bytes(claims)?;
    let signature: Signature = signing_key.sign(&canonical);
    let sig_b64 = B64.encode(signature.to_bytes());

    // Rebuild as BTreeMap and inject the sig field.
    let v = serde_json::to_value(claims)
        .map_err(|e| LicenceError::Malformed(format!("serialise for output: {e}")))?;
    let mut map: BTreeMap<String, Value> = serde_json::from_value(v)
        .map_err(|e| LicenceError::Malformed(format!("claims to map for output: {e}")))?;
    map.insert("sig".to_string(), Value::String(sig_b64));

    serde_json::to_string(&map)
        .map_err(|e| LicenceError::Malformed(format!("final to_string: {e}")))
}

/// Verify `blob` against the caller-supplied public key list.
///
/// `pubkeys` is a slice of `(kid, raw_32_byte_pubkey)` pairs. The function
/// looks up the `kid` found in the blob to select the right key.
///
/// Validity window enforcement (not-before / valid-until) is intentionally
/// NOT done here — that is a state-machine concern for the caller.
///
/// # Errors
///
/// - [`LicenceError::Malformed`] if `blob` is not valid JSON or is missing
///   required fields.
/// - [`LicenceError::UnknownKid`] if the `kid` in the blob has no entry in
///   `pubkeys`.
/// - [`LicenceError::InvalidSignature`] if the signature does not verify.
pub fn verify(blob: &[u8], pubkeys: &[(&str, &[u8])]) -> Result<VerifiedLicence, LicenceError> {
    // Must have at least enough bytes to be a JSON object.
    if blob.len() < 2 {
        return Err(LicenceError::Malformed("blob too short".to_string()));
    }

    // Parse as a generic map to extract kid + sig before reconstructing claims.
    let mut map: BTreeMap<String, Value> = serde_json::from_slice(blob)
        .map_err(|e| LicenceError::Malformed(format!("parse JSON: {e}")))?;

    let kid = map
        .get("kid")
        .and_then(Value::as_str)
        .ok_or_else(|| LicenceError::Malformed("missing 'kid' field".to_string()))?
        .to_string();

    let sig_b64 = map
        .remove("sig")
        .and_then(|v| {
            if let Value::String(s) = v {
                Some(s)
            } else {
                None
            }
        })
        .ok_or_else(|| LicenceError::Malformed("missing or non-string 'sig' field".to_string()))?;

    // Look up the public key for this kid.
    let pubkey_bytes = pubkeys
        .iter()
        .find(|(k, _)| *k == kid)
        .map(|(_, b)| *b)
        .ok_or_else(|| LicenceError::UnknownKid(kid.clone()))?;

    let verifying_key = VerifyingKey::from_bytes(
        pubkey_bytes
            .try_into()
            .map_err(|_| LicenceError::Malformed("pubkey must be 32 bytes".to_string()))?,
    )
    .map_err(|_| LicenceError::Malformed("invalid pubkey bytes".to_string()))?;

    // Canonical bytes are the map WITHOUT the sig field (already removed).
    let canonical = serde_json::to_vec(&map)
        .map_err(|e| LicenceError::Malformed(format!("re-serialise for verify: {e}")))?;

    let sig_bytes = B64
        .decode(sig_b64)
        .map_err(|e| LicenceError::Malformed(format!("base64 decode sig: {e}")))?;
    let sig_arr: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| LicenceError::Malformed("signature must be 64 bytes".to_string()))?;
    let signature = Signature::from_bytes(&sig_arr);

    verifying_key
        .verify_strict(&canonical, &signature)
        .map_err(|_| LicenceError::InvalidSignature)?;

    // Now deserialise the claims from the map (sig already removed).
    let claims: LicenceClaims = serde_json::from_value(Value::Object(map.into_iter().collect()))
        .map_err(|e| LicenceError::Malformed(format!("deserialise claims: {e}")))?;

    Ok(VerifiedLicence {
        kid,
        claims,
        blob_bytes: blob.to_vec(),
    })
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::EncodePrivateKey;
    use rand::rngs::OsRng;
    use serde_json::json;
    use time::macros::datetime;

    fn test_claims(policy: Value) -> LicenceClaims {
        LicenceClaims {
            kid: "v1.0".to_string(),
            customer_id: "00000000-0000-0000-0000-000000000001".to_string(),
            customer_email: "test@example.com".to_string(),
            tier: "standard".to_string(),
            seats: Some(10),
            issued_at: datetime!(2026-06-01 12:00:00 UTC),
            not_before: datetime!(2026-06-01 12:00:00 UTC),
            valid_until: datetime!(2026-06-08 12:00:00 UTC),
            policy,
        }
    }

    fn generate_signing_key() -> SigningKey {
        SigningKey::generate(&mut OsRng)
    }

    fn key_to_pem(key: &SigningKey) -> String {
        key.to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
            .expect("to_pkcs8_pem should not fail for a valid key")
            .to_string()
    }

    fn pubkey_bytes(key: &SigningKey) -> Vec<u8> {
        key.verifying_key().to_bytes().to_vec()
    }

    #[test]
    fn canonical_json_is_deterministic_for_same_claims() {
        let claims = test_claims(json!({"x": 1}));
        let a = canonical_json_bytes(&claims).expect("canonical_json_bytes");
        let b = canonical_json_bytes(&claims).expect("canonical_json_bytes second call");
        assert_eq!(a, b);
    }

    #[test]
    fn canonical_json_excludes_sig_field() {
        let claims = test_claims(json!({"x": 1}));
        let bytes = canonical_json_bytes(&claims).expect("canonical_json_bytes");
        let as_str = String::from_utf8(bytes).expect("utf8");
        assert!(
            !as_str.contains("\"sig\""),
            "canonical bytes must not contain a 'sig' field, got: {as_str}"
        );
    }

    #[test]
    fn sign_then_verify_roundtrip_succeeds_with_matching_key() {
        let sk = generate_signing_key();
        let pem = key_to_pem(&sk);
        let pk = pubkey_bytes(&sk);
        let claims = test_claims(json!({"feature_x": true, "nested": {"k": 1}}));

        let blob = sign(&claims, &pem).expect("sign");
        let verified = verify(blob.as_bytes(), &[("v1.0", &pk)]).expect("verify");

        assert_eq!(verified.kid, "v1.0");
        assert_eq!(verified.claims.customer_id, claims.customer_id);
        assert_eq!(verified.claims.tier, claims.tier);
    }

    #[test]
    fn verify_rejects_tampered_payload() {
        let sk = generate_signing_key();
        let pem = key_to_pem(&sk);
        let pk = pubkey_bytes(&sk);
        let claims = test_claims(json!({"x": 1}));

        let blob = sign(&claims, &pem).expect("sign");
        // Flip one byte in the customer_email value.
        let tampered = blob.replace("test@example.com", "tampered@example.com");

        let result = verify(tampered.as_bytes(), &[("v1.0", &pk)]);
        assert!(
            matches!(result, Err(LicenceError::InvalidSignature)),
            "expected InvalidSignature, got: {result:?}"
        );
    }

    #[test]
    fn verify_rejects_unknown_kid() {
        let sk = generate_signing_key();
        let pem = key_to_pem(&sk);
        let pk = pubkey_bytes(&sk);
        // Sign with kid v1.0, but pubkeys map only has v9.
        let claims = test_claims(json!({}));

        let blob = sign(&claims, &pem).expect("sign");
        let result = verify(blob.as_bytes(), &[("v9", &pk)]);
        assert!(
            matches!(result, Err(LicenceError::UnknownKid(ref k)) if k == "v1.0"),
            "expected UnknownKid(v1.0), got: {result:?}"
        );
    }

    #[test]
    fn verify_rejects_wrong_signature() {
        let sk_a = generate_signing_key();
        let sk_b = generate_signing_key();
        let pem_a = key_to_pem(&sk_a);
        let pk_b = pubkey_bytes(&sk_b);
        let claims = test_claims(json!({}));

        let blob = sign(&claims, &pem_a).expect("sign with key A");
        let result = verify(blob.as_bytes(), &[("v1.0", &pk_b)]);
        assert!(
            matches!(result, Err(LicenceError::InvalidSignature)),
            "expected InvalidSignature, got: {result:?}"
        );
    }

    #[test]
    fn policy_value_round_trips_arbitrary_json() {
        let policy = json!({"a": 1, "b": [true, null], "c": {"deep": "value"}});
        let sk = generate_signing_key();
        let pem = key_to_pem(&sk);
        let pk = pubkey_bytes(&sk);
        let claims = test_claims(policy.clone());

        let blob = sign(&claims, &pem).expect("sign");
        let verified = verify(blob.as_bytes(), &[("v1.0", &pk)]).expect("verify");

        assert_eq!(
            verified.claims.policy, policy,
            "policy must round-trip without mutation"
        );
    }

    #[test]
    fn sign_with_invalid_pem_returns_malformed_error() {
        let claims = test_claims(json!({}));
        let result = sign(&claims, "not a pem");
        assert!(
            matches!(result, Err(LicenceError::Malformed(_))),
            "expected Malformed, got: {result:?}"
        );
    }

    #[test]
    fn verify_blob_too_short_returns_malformed_error() {
        let result = verify(b"{}", &[]);
        assert!(
            matches!(result, Err(LicenceError::Malformed(_))),
            "expected Malformed for empty object, got: {result:?}"
        );
    }
}
