# Releasing ZANPOS, and how a till updates itself

Two commands, in this order, from the repository root:

```bash
npm run ship      # gate + signed release build (~60 min)
npm run release   # publish, then verify a till could read it
```

Nothing else. No USB stick, no visiting terminals. Tills check on launch, are
prompted between shifts, and install with the owner PIN.

---

## What each command is for

`npm run ship` is the gate that decides whether a build may reach a store. It
runs the frontend checks, `cargo check`, the 500-line file rule, the version
consistency check, the cfg-gate check, a signing pre-flight, and finally the
fat-LTO release build. It refuses to continue on the first hard failure.

`npm run release` takes what the gate produced and publishes it. **It cannot
build.** A publisher that could also build would be a second route to an
installer — one that skips the gate — so it only ever uploads artifacts that
already exist.

Useful flags:

```bash
npm run ship -- --no-build       # everything except the hour-long build
npm run release -- --dry-run     # print exactly what would be uploaded
npm run release -- --critical    # mark it critical: tills refuse a new shift
```

---

## Cutting a version

Bump all three, or the gate stops you:

| File | Why it matters |
|---|---|
| `src-tauri/tauri.conf.json` | **authoritative** — stamped on the installer, advertised in the manifest |
| `package.json` | frontend build metadata |
| `src-tauri/Cargo.toml` | `env!("CARGO_PKG_VERSION")` — what a till *reports* as its installed version |

A lagging `Cargo.toml` is not cosmetic. That value is written into the
`update_applied` audit row and shown as the terminal's version, so drift makes a
till claim a version it is not running — on the screen used to check exactly
that.

Versions must be plain `x.y.z`. The updater compares them numerically.

---

## The trust model

Updates are signed with a **minisign** key. The public half is compiled into
every build (`plugins.updater.pubkey`); the private half never leaves the dev
machine and is never committed.

```
key file   C:/Users/super/ZAN/zanpos/zanpos-updater-2026.key
referenced .env → TAURI_SIGNING_PRIVATE_KEY   (path; ship.mjs resolves it to contents)
password   none — the key FILE is the secret. Back it up.
```

A till verifies the signature before installing anything. An unsigned or
tampered package is refused, which is why an unsigned build is worse than a
failed one: it looks fine here and is rejected everywhere.

**If the key is lost**, no future release can be signed for the existing fleet.
Recovery is: generate a new key, put its public half in `tauri.conf.json`, build,
and install that build by hand on every till once. The 2025 key
(`zanpos.key`) was retired exactly this way after its password was lost.

Only one signing variable may be set. `TAURI_SIGNING_PRIVATE_KEY` and
`TAURI_SIGNING_PRIVATE_KEY_PATH` are mutually exclusive and the CLI refuses both;
setting only `_PATH` is worse, because the bundler ignores it and the build
succeeds with no signature. The gate's signing pre-flight catches both in about
two seconds rather than at minute 56.

---

## Where releases live

```
bucket        zanpos-releases            (Cloudflare R2)
manifest      {R2_PUBLIC_BASE}/latest.json
installer     {R2_PUBLIC_BASE}/{version}/ZANPOS_{version}_x64-setup.exe
```

`R2_PUBLIC_BASE` in `.env` **must match** `plugins.updater.endpoints` in
`tauri.conf.json`. The publisher refuses to upload when they disagree, because
publishing to a URL no till asks for produces a release that looks successful
here and reaches nobody — worse than an error, since the shop believes it
updated.

Changing that URL requires a rebuild *and* a manual install round, because
installed tills only ever ask the address compiled into them. To avoid that,
`update_endpoint` in `app_config` overrides it at runtime and syncs from the hub:
set it once there and every till follows.

The `r2.dev` address is Cloudflare's development endpoint — rate-limited, and
throughput-throttled. Fine for a handful of tills. Attaching a custom domain to
the bucket removes both that and the risk of the address ever changing.

---

## Order of operations, and why

The publisher uploads the **installer before the manifest**. A till reads the
manifest and immediately fetches the URL inside it, so the reverse order opens a
window where the manifest names a file that does not exist yet — and that
failure looks like a broken release rather than a race.

Cache headers follow from the same reasoning: the installer is immutable for a
given version and cached for a year; the manifest is `no-cache`, because a
cached manifest means a release silently does not arrive.

---

## What a till does

1. Checks on launch, periodically, and on demand from Settings
2. Compares the manifest version against its own
3. Waits for a safe moment — never mid-cart, never mid-shift
4. Downloads, verifies the signature, installs, restarts
5. Records `update_applied` with the from/to versions

A `critical` release additionally blocks starting a new shift until it is taken.

**"Check for Updates" saying *up to date* does not prove connectivity.**
`check_for_updates` deliberately swallows network errors and returns "no update",
so a till with no internet looks identical to one that reached the manifest. That
is correct for a POS — a network problem must never block a sale — but useless as
a test. To test reachability, open the manifest URL in a browser on that till.

---

## Verifying a release landed

```bash
curl -s {R2_PUBLIC_BASE}/latest.json
curl -sI {R2_PUBLIC_BASE}/{version}/ZANPOS_{version}_x64-setup.exe
```

The second should return `200` with a `Content-Length` matching the local file
byte-for-byte. The publisher already checks the manifest is publicly readable —
every upload step can succeed against a bucket that is not public, which looks
clean from the publishing side and serves no till at all.

---

## Failure modes

| Symptom | Cause |
|---|---|
| Build succeeds, no `.sig` beside the installer | signing key not in the environment |
| `--private-key cannot be used with --private-key-path` | both signing variables set |
| Publisher: "No endpoint matches" | `R2_PUBLIC_BASE` disagrees with the compiled endpoint |
| Publisher: "not publicly readable" | bucket public access is off |
| Tills never see a release | manifest cached, or they are on an older compiled endpoint |
| Over wrangler's 315 MB ceiling | switch the upload step to rclone or an S3 client |

`.env` must be LF or CRLF — both work now, but note that JavaScript's `.` does
not match `\r`, which silently dropped every valued line under CRLF and produced
an unsigned build with no error.

---

## Not covered yet

Release channels (dev/beta/stable), download-progress UI states, rollback to a
previous version, CI-driven releases, and per-terminal version visibility in the
roster. Rollback in particular: there is no supported downgrade path, and the
updater will not offer one.
