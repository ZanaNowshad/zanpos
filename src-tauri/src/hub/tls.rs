//! Certificate fingerprints for the LAN hub.
//!
//! Step 1 of `docs/proposals/06-hub-tls-fingerprint-pinning.md` (ledger D02).
//! The hub currently serves plaintext HTTP on `0.0.0.0`, so the per-device
//! tokens `pairing.rs` is careful to store only as digests still cross the shop
//! WiFi in the clear. The fix is TLS with the certificate pinned at pairing
//! rather than a CA — one shop, one LAN, no PKI.
//!
//! This module is the trust primitive that sits under all of it: turning a
//! certificate into the string that gets advertised, pinned and compared.
//! `discovery.rs` has carried a `tls_fingerprint` field since before this work
//! (`:24`, `:60`, `:81-83`, `:124`) with nothing populating it, and its tests
//! already fix the wire format as `SHA256:…` — this produces that format.
//!
//! Deliberately free of new dependencies: `sha2` and `hex` are already in the
//! tree, so nothing here changes the dependency surface.

use sha2::{Digest, Sha256};

/// Prefix identifying the digest algorithm, matching the format
/// `hub::discovery` already advertises and parses.
pub const FINGERPRINT_PREFIX: &str = "SHA256:";

/// Fingerprint a certificate's DER bytes.
///
/// The output is `SHA256:` followed by uppercase hex pairs separated by colons
/// — byte for byte what `openssl x509 -fingerprint -sha256` prints after its
/// own `SHA256 Fingerprint=` label. That is deliberate: an operator comparing a
/// hub's advertised fingerprint against the certificate on disk should be able
/// to run the obvious command and see the same characters, without knowing
/// anything about this code.
pub fn cert_fingerprint(der: &[u8]) -> String {
    let digest = Sha256::digest(der);
    let mut out = String::with_capacity(FINGERPRINT_PREFIX.len() + 32 * 3 - 1);
    out.push_str(FINGERPRINT_PREFIX);
    for (i, byte) in digest.iter().enumerate() {
        if i > 0 {
            out.push(':');
        }
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

/// Parse a `SHA256:AA:BB:…` fingerprint back into its 32 digest bytes.
///
/// Returns `None` for anything that is not exactly 32 well-formed bytes under
/// the expected prefix. Rejecting rather than repairing matters here: a
/// fingerprint that does not parse is a pin that cannot be honoured, and
/// treating it as "no pin" would silently downgrade the connection it was
/// supposed to protect.
pub fn parse_fingerprint(value: &str) -> Option<[u8; 32]> {
    // The prefix is matched case-insensitively. It is an algorithm label, not
    // secret material, and the pairing flow has an operator reading a
    // fingerprint off one screen and comparing it against another — refusing
    // `sha256:` while accepting `SHA256:` would reject a correct pin over the
    // case of a label. The digest itself is still compared byte for byte.
    if value.len() < FINGERPRINT_PREFIX.len() {
        return None;
    }
    let (prefix, body) = value.split_at(FINGERPRINT_PREFIX.len());
    if !prefix.eq_ignore_ascii_case(FINGERPRINT_PREFIX) {
        return None;
    }
    let hex_only: String = body.chars().filter(|c| *c != ':').collect();
    if hex_only.len() != 64 {
        return None;
    }
    let bytes = hex::decode(hex_only).ok()?;
    <[u8; 32]>::try_from(bytes.as_slice()).ok()
}

/// Compare two fingerprints in constant time.
///
/// Both are parsed first, so formatting differences — a lowercase hex pair, a
/// missing colon — compare equal when the underlying digests are equal, and an
/// unparseable value never matches anything.
///
/// The comparison itself accumulates differences rather than returning early.
/// Against a digest this is close to theoretical, since an attacker cannot
/// invert SHA-256 from timing, but this runs on every request to a
/// network-facing service and an early-return compare here would be the kind of
/// thing a later reader copies somewhere it does matter.
pub fn fingerprints_match(presented: &str, pinned: &str) -> bool {
    let (Some(a), Some(b)) = (parse_fingerprint(presented), parse_fingerprint(pinned)) else {
        return false;
    };
    let mut diff = 0u8;
    for i in 0..32 {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

#[cfg(test)]
mod tests;
