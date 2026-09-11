# 06 — Hub TLS with fingerprint pinning (D02)

**Status:** design, not implemented. **Severity:** P0. **Effort:** M–L.
**Base:** `upgrade/zanpos-open-source` @ `9951d3c`.

Closes ledger finding **D02**, the last open P0.

---

## 1. The problem, with evidence

The LAN hub binds every interface and speaks plaintext HTTP:

- `hub/mod.rs:107` — `SocketAddr::from(([0, 0, 0, 0], port))`
- `hub/mod.rs:115-118` — `axum::serve(listener, …)`, no TLS layer
- `hub/rest.rs:19-22` — routes `GET|POST /rest/v1/{table}`

`pairing.rs:3-7` already states the exposure in its own module docs: *"The hub
binds `0.0.0.0:8923`, so anything on the shop WiFi can reach it."*

Authentication has since been done properly — per-device tokens, only SHA-256
digests stored (`pairing.rs:9-10`), constant-time comparison, revocation without
re-keying the shop, `hub_unauthorized` audit events. **None of that survives a
plaintext transport.** The bearer token is sent in a header on every request, so
anyone passively sniffing the shop WiFi captures it and can then replay it
against `POST /rest/v1/users` or `/rest/v1/audit_logs`. Every synced row —
customer names, phone numbers, every sale — also crosses in cleartext.

The intended fix is already on record in the code. `hub/discovery.rs` carries a
`tls_fingerprint` field through the whole discovery path — struct (`:24`, `:60`),
validation (`:68-70`), mDNS advertisement (`:81-83`), and parsing back out
(`:124`) — and **nothing ever populates it**. The tests even fix the wire format
as `SHA256:…` (`discovery.rs:364,446`). This proposal fills in what that field
was left waiting for.

## 2. Threat model

| Actor | Capability today | After this change |
|---|---|---|
| Passive sniffer on shop WiFi | Reads every token and every synced row | Sees only TLS ciphertext |
| Active attacker on the LAN | Impersonates the hub; tills hand over tokens and accept forged rows | Cert fingerprint mismatch; till refuses to connect |
| Guest-network device | Reaches `0.0.0.0:8923` and brute-forces tokens | Still reachable (see §8) but cannot read traffic |
| Stolen/lost till | Already handled — revoke that device's pairing | Unchanged |

**Explicit non-goals.** Not a PKI. No CA, no rotation service, no OCSP. A single
self-signed certificate per hub install, pinned by the tills that pair with it.
That is the correct weight for one shop on one LAN.

## 3. Design

### 3.1 Certificate — minted once, stable thereafter

On first `start_hub`, mint a self-signed cert for the hub's identity and persist
it. Stability matters more than freshness: the fingerprint is the pin, so
regenerating it on every boot would break every paired till.

- **Storage:** `<app_data>/hub/` — `cert.pem` (public) and `key.pem` (private,
  created `0600` on Unix; on Windows rely on the per-user AppData ACL, same
  posture the SQLite file already has).
- **Not in the keyring.** `secure_store` holds short secrets; a PEM key is the
  wrong shape for it, and the DB beside it has the same protection.
- **SANs:** every address from `lan_ips()` (`hub/mod.rs:140`), plus `localhost`,
  `127.0.0.1`, and the hub's `instance_id` as a DNS name. Validity 10 years —
  expiry adds no security here (the pin is the trust anchor) and an expired cert
  would silently strand a shop.
- **Regeneration** only on explicit operator action, which must re-pair every
  till. Surface it as "Reset hub identity", not as a silent repair.

### 3.2 Serving

Wrap the existing listener in a rustls acceptor. `start_hub`'s shape is
unchanged — same `HubState`, same router, same graceful shutdown — so
`rest.rs`, `pairing.rs` and the auth path are untouched by this work.

### 3.3 Advertisement

Populate the field that already exists. `HubDiscoveryConfig.tls_fingerprint`
becomes `Some("SHA256:<base16-upper-colon-separated>")`, computed as the SHA-256
of the certificate's DER bytes — the standard fingerprint, matching what
`openssl x509 -fingerprint -sha256` prints, so an operator can verify by hand.
`advertised_metadata()` (`discovery.rs:74-85`) already carries it, and the test
at `:448-450` already asserts no secret leaks into that map; the fingerprint is
public by construction and does not violate it.

### 3.4 Pinning — trust on first use, at pairing only

The pin is captured **exactly once, at pairing**, which is the only moment a
human is present and can compare a code:

1. Operator picks the hub in the join UI. Discovery has already supplied the
   advertised fingerprint.
2. The till connects, reads the certificate actually presented, and computes its
   fingerprint.
3. **It must equal the advertised one.** If not, abort — that is an active
   attacker, and this is the one moment the check is free.
4. Show the operator a short confirmation code — the first 8 hex characters,
   grouped — visible on both the hub screen and the joining till. They confirm
   the two match. This is what defeats an attacker who controls mDNS, since the
   attacker cannot make their fingerprint match the hub's own display.
5. Store it: `app_config` key `hub_tls_fingerprint`, beside the existing
   `hub_url`. Public data, so `app_config` is right and `secure_store` is not.
   It must **not** be added to `ALLOWED_CONFIG_KEYS` — a pin is per-hub and
   device-local, and syncing it would let a compromised hub push its own pin.

Thereafter every request verifies the presented cert against the stored pin and
refuses on mismatch. No CA validation, no hostname validation — the pin is the
whole trust decision, which is why it can be self-signed.

### 3.5 Client

`HttpSyncClient::new` (`sync_v2/client.rs:44`) gains the pinned fingerprint and
builds its `reqwest::Client` with a rustls config whose `ServerCertVerifier`
compares the SHA-256 of the presented leaf against the pin in constant time.

Unrelated but adjacent: `client.rs:48` is `.expect("reqwest client")` — one of
only four production `.expect()` sites in the tree. Building a TLS config is
more fallible than building a plain client, so this call must return `Result`
as part of the same change rather than gaining a new way to panic.

## 4. Compatibility — the part that must not break a shop

`pairing.rs:12-15` sets the precedent: *"a store that upgrades mid-shift must not
lose its second till."* Same discipline here. Three states, by `app_config` key
`hub_transport`:

| Value | Hub | Till |
|---|---|---|
| `plaintext` (default on upgrade) | HTTP only, as today | HTTP, no pinning |
| `tls-preferred` | HTTPS **and** HTTP on the legacy port | HTTPS if a pin exists, else HTTP, and log every plaintext fall-back |
| `tls-required` | HTTPS only | HTTPS only; refuse unpinned |

Upgrade lands on `plaintext` — **identical to current behaviour, so no shop
breaks on update.** The operator moves to `tls-preferred`, re-pairs each till
(pairing already supports re-pairing to recover a lost token, `pairing.rs:63-64`,
so no new mechanism is needed), watches the plaintext-fall-back count reach
zero, then switches to `tls-required`. The settings screen should show that
count, because it is the only evidence that flipping the switch is safe.

`tls-required` is the goal state and should be the default for **new**
installs — a shop setting up today should never run plaintext.

## 5. Dependencies

Four additions. Per project rule, each names the alternative rejected.

| Crate | Why | Rejected alternative |
|---|---|---|
| `rustls` | Pure-Rust TLS; `ServerCertVerifier` is a public trait, which is what makes pinning expressible at all | `native-tls`/SChannel — pinning would mean fighting the platform store, and the behaviour would differ per OS |
| `tokio-rustls` | Async acceptor for the existing `tokio::net::TcpListener`; keeps `start_hub`'s structure | `axum-server` — pulls its own runtime glue to save ~30 lines |
| `rcgen` | Mints the self-signed cert in-process | Shipping `openssl.exe` or a pre-generated cert — a shared private key across installs is not a design |
| `reqwest` feature `rustls-tls` | Lets the client use the same verifier | — (feature flag, not a new crate) |

`sha2` is already present (`Cargo.toml:96`). All four are Windows-compatible and
belong under the existing `[target.'cfg(windows)'.dependencies]` discipline.

## 6. Schema

One migration, additive only.

```sql
-- 00NN_hub_tls.sql
-- Records which transport each paired device last authenticated over, so the
-- operator can see whether every till has moved to TLS before requiring it.
-- Nullable: rows predating this migration have no observation yet, which is
-- different from "observed plaintext" and must not be conflated.
ALTER TABLE hub_paired_devices ADD COLUMN last_transport TEXT;
```

No backfill. No change to `token_digest`, `revoked_at`, or the auth path.
`hub_transport` and `hub_tls_fingerprint` are `app_config` rows, needing no DDL.

## 7. Test plan

Every item runs on Windows (Linux cannot build this crate — `Cargo.toml:92-123`).

**Unit**
1. Fingerprint of a known DER equals the `openssl` value — pins the format.
2. Minting is idempotent: second `start_hub` reuses the cert, fingerprint unchanged.
3. Verifier accepts the matching cert.
4. Verifier **rejects** a different valid self-signed cert — the MITM case.
5. Verifier rejects an empty/malformed chain.
6. Comparison is constant-time.

**Integration**
7. Till pairs over TLS, pins, syncs a row, reads it back.
8. Hub re-mints; the pinned till now refuses. *Should fail closed.*
9. `tls-required` refuses an unpinned till.
10. `tls-preferred` serves both, and the plaintext path increments the counter.
11. A revoked device is still refused over TLS — pairing and transport are independent.

**Regression**
12. `plaintext` behaves exactly as today; the whole existing `rest_tests.rs` suite passes unchanged.

Gate: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test --lib`, `npm run check` — all green, run locally, because CI is
billing-blocked (ledger OPS1).

## 8. Known residue

- **Still binds `0.0.0.0`.** TLS fixes confidentiality, not reachability. Binding
  only the chosen interface is a separate, smaller change worth doing alongside.
- **No rate limit on auth failures.** Token brute-force over the LAN stays
  possible. Independent of transport; should be its own finding.
- **First pairing is trust-on-first-use.** The confirmation code (§3.4 step 4) is
  what reduces it from blind TOFU to an operator-verified pin. Skipping that step
  makes the whole design blind, so it is not optional.

## 9. Sequence

1. Fingerprint helper + tests 1, 6. *(S)*
2. `rcgen` minting, persistence, idempotence — test 2. *(S)*
3. rustls acceptor behind `hub_transport`; regression test 12. *(M)*
4. Populate `tls_fingerprint` in discovery. *(S)*
5. Client verifier + `HttpSyncClient::new` returning `Result`; tests 3, 4, 5. *(M)*
6. Pairing captures the pin, with the confirmation code; tests 7, 8. *(M)*
7. Migration, transport counter, settings UI; tests 9, 10, 11. *(M)*
8. Default new installs to `tls-required`; document the upgrade path. *(S)*

Steps 1–2 are self-contained and land with no behaviour change.
