# ZANPOS — Backup & Restore Operations

**Status:** P0 release gate. A restore drill on real hardware must be completed and signed off before production.

---

## 1. What Gets Backed Up

ZANPOS stores all data in a single SQLite file:

| Location | Path |
|----------|------|
| Windows production | `%APPDATA%\com.super.zanpos\zanpos.db` |
| WAL sidecar files | `zanpos.db-wal`, `zanpos.db-shm` |

The WAL files exist while the app is running, and survive a crash or a killed
process. They are folded into the main `.db` file when the app closes cleanly or
when a `PRAGMA wal_checkpoint(TRUNCATE)` is issued.

**A copy of `zanpos.db` on its own is not a backup.** The uncheckpointed WAL is
not a trailing handful of rows — it is routinely larger than the database itself
(1.7 MB against 1.4 MB on the till used for the §8 drill), and in that drill a
plain file copy recovered **none** of 25 committed sales. Take snapshots with
`VACUUM INTO`, which reads through the WAL; see §2.

---

## 2. Built-in Backup Command

The `db_backup` Tauri command (owner-only) performs:

1. RBAC check — only `owner` role may trigger a backup.
2. Resolves `%APPDATA%\com.super.zanpos\zanpos.db` as the source.
3. Writes a consistent snapshot to:
   - A user-supplied path (if provided), or
   - `Documents\zanpos-backup-YYYYMMDD_HHMMSS.db` (auto-generated).
4. Returns the full destination path on success.

**The command uses `VACUUM INTO`, not a file copy** (`phase10a_commands.rs`,
`db_backup`). This matters more than it looks.

The obvious implementation — `PRAGMA wal_checkpoint(TRUNCATE)`, then copy the
file — is two steps with a gap between them, and a sale can commit in that gap.
Nothing in SQLite promises that a plain read of a live database yields a valid
one; what comes out can be a mixture of two states. `VACUUM INTO` holds a single
read transaction for the whole write, so the file it produces is the database as
it stood at one instant, and it reads *through* the WAL rather than needing it
folded in first.

The off-site encrypted backup in `backup.rs` (`snapshot`) uses the same method
for the same reason.

> **Never back up ZANPOS by copying `zanpos.db` on its own.** Committed sales
> live in `zanpos.db-wal` until a checkpoint moves them, and the WAL is routinely
> larger than the database. A copy that omits it still opens, still has the full
> schema, and is silently missing transactions — see the drill in §8, where this
> method lost every one of 25 committed sales.

---

## 3. Backup Schedule — Recommendation

| Environment | Frequency | Method |
|---|---|---|
| Limited beta | Daily, end of business | Manual via Back Office or script |
| Production | Every shift close + daily snapshot | Automated via scheduled task (see below) |

**Prefer the built-in off-site backup.** `backup.rs` already snapshots, encrypts
(XChaCha20-Poly1305, Argon2id key) and uploads once a day on its own. It needs no
scheduled task and no file share, and it covers the case a network copy does not:
the store PC's disk dying. A hand-rolled task is only for keeping an *additional*
plaintext copy on local infrastructure.

### Windows Scheduled Task (PowerShell)

> Two things about the snippet below are load-bearing. The app data folder is
> `com.super.zanpos`, not `zanpos` — the bundle identifier, from
> `tauri.conf.json`. And the snapshot is taken with `VACUUM INTO` rather than
> `Copy-Item`, because a copy of the `.db` alone drops everything still in the
> WAL. An earlier revision of this document recommended `Copy-Item` against the
> wrong path; it would have failed outright, and had the path been right it would
> have produced backups quietly missing the day's sales.

```powershell
# Run at 23:00 daily. Snapshot locally with VACUUM INTO, then copy the finished
# file to the share — a static file is safe to copy, a live database is not.
$src   = "$env:APPDATA\com.super.zanpos\zanpos.db"
$stamp = Get-Date -Format 'yyyyMMdd_HHmmss'
$tmp   = Join-Path $env:TEMP "zanpos-$stamp.db"
$dest  = "\\fileserver\zanpos-backups\zanpos-$stamp.db"

# sqlite3.exe must be on PATH. VACUUM INTO refuses to overwrite, so $tmp is new.
& sqlite3.exe $src "VACUUM INTO '$($tmp -replace '\\','/')'"
if ($LASTEXITCODE -ne 0) { throw "ZANPOS backup snapshot failed ($LASTEXITCODE)" }

# Fail loudly if the snapshot is not a database — a task nobody watches is
# exactly where a silent bad backup survives until the day it is needed.
$magic = [System.Text.Encoding]::ASCII.GetString(
           [System.IO.File]::ReadAllBytes($tmp)[0..14])
if ($magic -ne 'SQLite format 3') { throw "ZANPOS backup is not a SQLite file" }

Copy-Item $tmp $dest
Remove-Item $tmp
```

Register with Task Scheduler:
```powershell
$action  = New-ScheduledTaskAction -Execute "powershell.exe" -Argument "-File C:\zanpos\backup.ps1"
$trigger = New-ScheduledTaskTrigger -Daily -At 23:00
Register-ScheduledTask -Action $action -Trigger $trigger -TaskName "ZANPOS Daily Backup" -RunLevel Highest
```

---

## 4. Restore Procedure

> **This is the procedure to follow when restoring from a backup. Complete a full drill before going live.**

### Step-by-step

1. **Close ZANPOS** on the POS device completely (check Task Manager — no `zanpos.exe` process).

2. **Locate the backup file** (e.g. `zanpos-backup-20241201_230000.db`).

3. **Verify the backup integrity:**
   ```powershell
   # File must be at least 4 KB and start with SQLite magic
   $f = Get-Item "zanpos-backup-20241201_230000.db"
   Write-Output "Size: $($f.Length) bytes"
   $header = [System.IO.File]::ReadAllBytes($f.FullName)[0..15]
   [System.Text.Encoding]::ASCII.GetString($header)
   # Expected output: "SQLite format 3"
   ```

4. **Rename (not delete) the live DB** as a safety net — **and its WAL**, which
   is where any sales not yet checkpointed still live. Keeping the set together
   is what makes a partial recovery possible if the backup turns out to be older
   than you thought:
   ```powershell
   $dir = "$env:APPDATA\com.super.zanpos"
   $tag = Get-Date -Format 'yyyyMMdd_HHmmss'
   Get-ChildItem "$dir\zanpos.db*" | ForEach-Object {
     Rename-Item $_.FullName "$($_.Name).broken-$tag"
   }
   ```

5. **Copy the backup into place:**
   ```powershell
   Copy-Item "zanpos-backup-20241201_230000.db" "$env:APPDATA\com.super.zanpos\zanpos.db"
   ```

6. **Confirm no stale WAL remains.** Step 4 moved it aside; this catches the case
   where the app was restarted in between and wrote a new one. A WAL left next to
   a restored database belongs to the *old* one, and SQLite will apply it:
   ```powershell
   Remove-Item "$env:APPDATA\com.super.zanpos\zanpos.db-wal" -ErrorAction SilentlyContinue
   Remove-Item "$env:APPDATA\com.super.zanpos\zanpos.db-shm" -ErrorAction SilentlyContinue
   ```

7. **Start ZANPOS** and verify:
   - Login works.
   - Products are visible.
   - Last known receipts appear in Recent Sales.
   - No error banner about DB corruption.

8. **Sign off** in the restore drill log (see template below).

---

## 5. Restore Drill Sign-Off Template

Complete this for each drill before production:

```
ZANPOS Restore Drill
--------------------
Date:              _______________
Performed by:      _______________
Device:            _______________
Backup file used:  _______________
Backup file size:  _______________
Backup date/time:  _______________

Verification checks:
[ ] SQLite magic bytes confirmed
[ ] App starts without error
[ ] Login with cashier account works
[ ] At least one product visible on POS
[ ] Recent sales from backup date visible
[ ] Last known receipt number matches expected

Data loss assessment:
  Transactions since backup that are NOT in restored DB: ___

Sign-off:          _______________
```

---

## 6. What Cannot Be Recovered from a Local Backup

- Transactions that occurred **after** the backup timestamp.
- Sync queue entries (`sync_queue` table) for events not yet pushed to Supabase.

If Supabase sync was active and the `sync_status = 'synced'` flag was set, the central Supabase database holds a complete ledger of all synced events — a partial recovery is possible by re-pulling from Supabase after restoring.

---

## 7. Automated CI Verification

The backup/restore logic is covered by 5 integration tests in
`src-tauri/src/commands/phase10a_commands.rs::tests`:

| Test | What it proves |
|------|----------------|
| `test_backup_file_is_valid_sqlite` | Copied file starts with SQLite magic and is ≥ 4096 bytes |
| `test_backup_restores_original_data` | Data written before backup is readable after restore |
| `test_backup_to_invalid_path_errors_not_panics` | Bad destination returns `Err`, not panic |
| `test_backup_is_point_in_time` | Writes after backup are NOT in the backup file |
| `test_wal_checkpoint_before_backup_captures_all_writes` | Committed WAL data is in the snapshot |

Two further invariants live in `src-tauri/src/db/invariants/backup.rs`:
`a_backup_restores_into_a_working_store` and
`a_corrupt_backup_is_detectable_before_it_is_trusted`.

These run in the `rust` CI job on every push and pull request, not only on
`main`/`master`.

---

## 8. Bench Drill Record — 2026-09-08

A drill run against a copy of a real till database. **This does not close the
production gate**, which requires the procedure in §4 to be carried out on shop
hardware, by a named person, with the §5 template signed. It closes a narrower
question: whether the documented procedure is correct and the restored database
comes up clean.

Source: `%APPDATA%\com.super.zanpos\zanpos.db` (1,449,984 bytes) with a
**1,726,312-byte WAL** — larger than the database, left behind by a process that
was killed rather than closed. The live database was copied, never opened
writable.

**Part 1 — the documented procedure, end to end.** Snapshot via `VACUUM INTO`,
verify magic bytes and size, move the old set aside, restore, drop the stale WAL.
Result: `integrity_check: ok`, `foreign_key_check: 0 violations`, 80 tables and
64 migrations intact, receipt sequence preserved, **no rows lost**.

That database held 3 users and 1 product but **no sales, payments or stock
movements**, so on its own it exercised structure and not transactional recovery.

**Part 2 — transactional recovery, with the hazard reproduced.** 25 clearly
synthetic sales and 25 payments (all prefixed `DRILL-FAKE-`) were committed into
the working copy and deliberately left uncheckpointed in the WAL, then both
backup methods were compared:

| Method | Fake sales recovered |
|---|---|
| `Copy-Item` of `zanpos.db` (the method §3 used to recommend) | **0 of 25** |
| `VACUUM INTO` (what `db_backup` and `backup.rs` actually do) | **25 of 25** |

The restored `VACUUM INTO` backup verified clean: `integrity_check: ok`, zero
foreign-key violations, zero payments orphaned from their sale.

This is why §2 and §3 were corrected. The code was already right; the
documentation described the unsafe method that the code had deliberately moved
away from, and recommended scheduling it. A backup that omits the WAL still
opens and still has the full schema — the loss is invisible until a restore.

**Still required before production:** the on-hardware drill, including the steps
no bench run can cover — that the app launches against the restored file, that a
cashier can log in, and that expected receipts appear in Recent Sales.
