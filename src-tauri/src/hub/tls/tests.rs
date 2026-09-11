use super::*;

/// SHA-256 of the empty input, and of `abc` — the two vectors every
/// implementation is checked against. Asserting them here means this agrees
/// with `openssl` and `sha256sum` rather than merely agreeing with itself.
const EMPTY_SHA256: &str = "SHA256:E3:B0:C4:42:98:FC:1C:14:9A:FB:F4:C8:99:6F:B9:24:\
27:AE:41:E4:64:9B:93:4C:A4:95:99:1B:78:52:B8:55";
const ABC_SHA256: &str = "SHA256:BA:78:16:BF:8F:01:CF:EA:41:41:40:DE:5D:AE:22:23:\
B0:03:61:A3:96:17:7A:9C:B4:10:FF:61:F2:00:15:AD";

fn strip_continuations(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

#[test]
fn fingerprint_matches_the_published_sha256_vectors() {
    assert_eq!(cert_fingerprint(b""), strip_continuations(EMPTY_SHA256));
    assert_eq!(cert_fingerprint(b"abc"), strip_continuations(ABC_SHA256));
}

#[test]
fn fingerprint_is_formatted_the_way_openssl_prints_it() {
    let fp = cert_fingerprint(b"any certificate bytes");
    let body = fp
        .strip_prefix(FINGERPRINT_PREFIX)
        .expect("carries the algorithm prefix");

    let pairs: Vec<&str> = body.split(':').collect();
    assert_eq!(pairs.len(), 32, "SHA-256 is 32 bytes");
    assert!(
        pairs.iter().all(|p| p.len() == 2),
        "each byte is a zero-padded pair: {body}"
    );
    assert!(
        body.chars().all(|c| c.is_ascii_hexdigit() || c == ':'),
        "hex and colons only: {body}"
    );
    assert_eq!(
        body,
        body.to_ascii_uppercase(),
        "openssl prints uppercase, and an operator compares by eye"
    );
}

#[test]
fn a_leading_zero_byte_is_not_shortened() {
    // Found by construction rather than luck: this input's digest starts 0x04,
    // so a `{:x}` instead of `{:02X}` would silently emit 63 characters.
    let mut input = 0u32;
    let fp = loop {
        let candidate = cert_fingerprint(&input.to_le_bytes());
        if candidate[FINGERPRINT_PREFIX.len()..].starts_with('0') {
            break candidate;
        }
        input += 1;
        assert!(
            input < 10_000,
            "expected a leading-zero digest well before here"
        );
    };
    let body = &fp[FINGERPRINT_PREFIX.len()..];
    assert_eq!(
        body.len(),
        32 * 3 - 1,
        "zero-padded, so length is fixed: {body}"
    );
}

#[test]
fn fingerprint_round_trips_through_parse() {
    let fp = cert_fingerprint(b"a certificate");
    let parsed = parse_fingerprint(&fp).expect("its own output parses");
    assert_eq!(cert_fingerprint(b"a certificate"), fp);
    assert_eq!(
        parsed,
        <[u8; 32]>::from(sha2::Sha256::digest(b"a certificate"))
    );
}

#[test]
fn parse_refuses_anything_that_is_not_a_whole_sha256() {
    for bad in [
        "",
        "SHA256:",
        "E3:B0:C4:42",                         // no prefix
        "SHA1:E3:B0:C4:42",                    // wrong algorithm
        "SHA256:E3:B0:C4",                     // too short
        "SHA256:ZZ:B0:C4:42",                  // not hex
        &format!("{}{}", EMPTY_SHA256, ":00"), // too long
    ] {
        assert!(
            parse_fingerprint(&strip_continuations(bad)).is_none(),
            "must refuse {bad:?} rather than treat it as a usable pin"
        );
    }
}

#[test]
fn matching_ignores_formatting_but_not_content() {
    let fp = cert_fingerprint(b"cert");

    assert!(fingerprints_match(&fp, &fp));
    assert!(
        fingerprints_match(&fp, &fp.to_ascii_lowercase()),
        "case is presentation, not identity"
    );
    // Only the body's separators are dropped. The colon after `SHA256` is the
    // prefix delimiter, not a byte separator — removing it yields `SHA256AA…`,
    // which is malformed, and refusing that is correct rather than lenient.
    let (prefix, body) = fp.split_at(FINGERPRINT_PREFIX.len());
    let unseparated = format!("{prefix}{}", body.replace(':', ""));
    assert!(
        fingerprints_match(&fp, &unseparated),
        "separators between bytes are presentation, not identity"
    );

    assert!(
        !fingerprints_match(&fp, &fp.replace(':', "")),
        "dropping the prefix delimiter makes it malformed, not equivalent"
    );
}

#[test]
fn a_different_certificate_is_refused() {
    // The reason this module exists: an attacker on the LAN presenting their
    // own valid self-signed certificate must not be accepted.
    let genuine = cert_fingerprint(b"the hub's certificate");
    let attacker = cert_fingerprint(b"the attacker's certificate");
    assert_ne!(genuine, attacker);
    assert!(!fingerprints_match(&attacker, &genuine));
}

#[test]
fn a_single_bit_difference_is_refused() {
    let genuine = cert_fingerprint(b"cert");
    let mut bytes = parse_fingerprint(&genuine).expect("parses");
    bytes[31] ^= 0x01;
    let flipped = format!(
        "{FINGERPRINT_PREFIX}{}",
        bytes
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    );
    assert!(
        !fingerprints_match(&flipped, &genuine),
        "one bit is a different certificate"
    );
}

#[test]
fn an_unparseable_pin_matches_nothing() {
    // Failing closed matters more here than anywhere: if a stored pin is
    // corrupt, the connection must be refused rather than silently treated as
    // unpinned, which would downgrade exactly the link this protects.
    let fp = cert_fingerprint(b"cert");
    assert!(!fingerprints_match(&fp, "SHA256:garbage"));
    assert!(!fingerprints_match("SHA256:garbage", &fp));
    assert!(!fingerprints_match("", ""));
}

#[test]
fn the_advertised_form_fits_what_discovery_validates() {
    // `discovery.rs` caps `tls_fingerprint` at 256 characters
    // (`validate_text("tls_fingerprint", fingerprint, 256)`). A fingerprint that
    // could not be advertised would be useless, so hold that boundary here
    // rather than discovering it when a hub refuses to start.
    let fp = cert_fingerprint(b"certificate");
    assert_eq!(fp.len(), FINGERPRINT_PREFIX.len() + 32 * 3 - 1);
    assert!(
        fp.len() <= 256,
        "must fit the discovery field: {}",
        fp.len()
    );
}
