//! License file parsing, validation, and Ed25519 signature verification.
//!
//! Split out of the parent module so the cryptographic seam is small enough to
//! read in one sitting — this is the code that decides whether a license is
//! genuine, and it should not be buried among database and command plumbing.

use super::{LicenseError, LicenseFile, LICENSE_PUBLIC_KEY_HEX};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::{DateTime, Utc};

pub(super) fn parse_rfc3339(s: &str) -> Result<DateTime<Utc>, LicenseError> {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.with_timezone(&Utc))
        .map_err(|_| LicenseError::InvalidTimestamp(s.to_string()))
}

fn decode_public_key_hex(hex_str: &str) -> Result<[u8; 32], LicenseError> {
    let bytes =
        hex::decode(hex_str).map_err(|e| LicenseError::InvalidPublicKeyEncoding(e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| LicenseError::InvalidPublicKeyEncoding("expected 32 bytes".into()))
}

fn decode_signature_b64(sig: &str) -> Result<[u8; 64], LicenseError> {
    let bytes = STANDARD
        .decode(sig)
        .map_err(|e| LicenseError::InvalidSignatureEncoding(e.to_string()))?;
    bytes
        .try_into()
        .map_err(|_| LicenseError::InvalidSignatureEncoding("expected 64 bytes".into()))
}

/// Verifies a raw Ed25519 signature via `ed25519-dalek`.
///
/// Two deliberate strictnesses:
///
/// 1. The all-zero placeholder key is rejected outright. A build that shipped
///    without [`LICENSE_PUBLIC_KEY_HEX`] being replaced must verify nothing at
///    all, rather than quietly trusting whatever a low-order key accepts.
/// 2. `verify_strict` rather than `verify` — it rejects low-order public keys
///    and non-canonical encodings, closing the malleability class of attack
///    where one signature validates under several keys.
fn verify_ed25519_signature(
    public_key: &[u8; 32],
    message: &[u8],
    signature: &[u8; 64],
) -> Result<bool, LicenseError> {
    if public_key.iter().all(|byte| *byte == 0) {
        return Err(LicenseError::VerificationUnavailable);
    }
    let verifying_key = ed25519_dalek::VerifyingKey::from_bytes(public_key)
        .map_err(|_| LicenseError::VerificationUnavailable)?;
    let signature = ed25519_dalek::Signature::from_bytes(signature);
    Ok(verifying_key.verify_strict(message, &signature).is_ok())
}

/// Boundary validation, before any cryptographic check or DB write.
pub(super) fn validate_license_file(file: &LicenseFile) -> Result<(), LicenseError> {
    if file.license_key.trim().is_empty() {
        return Err(LicenseError::EmptyKey);
    }
    parse_rfc3339(&file.issued_at)?;
    if let Some(expires) = &file.expires_at {
        parse_rfc3339(expires)?;
    }
    decode_signature_b64(&file.signature)?;
    Ok(())
}

/// Parses and validates raw license file JSON. Does not verify the
/// signature — see [`verify_license_file`] / [`import_license_file`].
pub fn parse_license_file(raw_json: &str) -> Result<LicenseFile, LicenseError> {
    if raw_json.trim().is_empty() {
        return Err(LicenseError::Malformed("license file is empty".into()));
    }
    let file: LicenseFile =
        serde_json::from_str(raw_json).map_err(|e| LicenseError::Malformed(e.to_string()))?;
    validate_license_file(&file)?;
    Ok(file)
}

/// Verifies `file`'s signature against [`LICENSE_PUBLIC_KEY_HEX`]. `Ok(())`
/// only on a genuinely valid signature.
pub fn verify_license_file(file: &LicenseFile) -> Result<(), LicenseError> {
    let public_key = decode_public_key_hex(LICENSE_PUBLIC_KEY_HEX)?;
    let signature = decode_signature_b64(&file.signature)?;
    if verify_ed25519_signature(&public_key, &file.signing_payload(), &signature)? {
        Ok(())
    } else {
        Err(LicenseError::SignatureInvalid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// RFC 8032 §7.1 test vector 1 — a known-good Ed25519 key/signature pair
    /// over the empty message. Proves the verification path really validates
    /// signatures rather than merely compiling.
    const RFC8032_PUBLIC_KEY: &str =
        "d75a980182b10ab7d54bfed3c964073a0ee172f3daa62325af021a68f707511a";
    const RFC8032_SIGNATURE_HEX: &str = concat!(
        "e5564300c360ac729086e2cc806e828a84877f1eb8e5d974d873e0652249015",
        "55fb8821590a33bacc61e39701cf9b46bd25bf5f0595bbe24655141438e7a100b",
    );

    fn hex32(s: &str) -> [u8; 32] {
        let mut out = [0u8; 32];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex");
        }
        out
    }

    fn hex64(s: &str) -> [u8; 64] {
        let mut out = [0u8; 64];
        for (i, byte) in out.iter_mut().enumerate() {
            *byte = u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex");
        }
        out
    }

    #[test]
    fn accepts_a_known_good_rfc8032_signature() {
        let verified = verify_ed25519_signature(
            &hex32(RFC8032_PUBLIC_KEY),
            b"",
            &hex64(RFC8032_SIGNATURE_HEX),
        );
        assert!(verified.expect("verification ran"));
    }

    #[test]
    fn rejects_a_tampered_message_under_a_valid_key() {
        let verified = verify_ed25519_signature(
            &hex32(RFC8032_PUBLIC_KEY),
            b"tampered",
            &hex64(RFC8032_SIGNATURE_HEX),
        );
        assert!(!verified.expect("verification ran"));
    }

    /// A build that forgot to replace the placeholder key must verify nothing.
    #[test]
    fn placeholder_public_key_never_verifies() {
        let result = verify_ed25519_signature(
            &hex32(LICENSE_PUBLIC_KEY_HEX),
            b"anything",
            &hex64(RFC8032_SIGNATURE_HEX),
        );
        assert!(matches!(result, Err(LicenseError::VerificationUnavailable)));
    }
}
