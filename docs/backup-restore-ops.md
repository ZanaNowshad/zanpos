# ZANPOS — Backup & Restore Operations

**Status:** P0 release gate. A restore drill on real hardware must be completed and signed off before production.

---

## 1. What Gets Backed Up

ZANPOS stores all data in a single SQLite file:

| Location | Path |
|----------|------|
| Windows production | `%APPDATA%\com.super.zanpos\zanpos.db` |
| WAL sidecar files | `zanpos.db-wal`, `zanpos.db-shm` |

The WAL files exist while the app is running. They are flushed into the main `.db` file when the app closes cleanly or when a `PRAGMA wal_checkpoint(TRUNCATE)` is issued. **A backup copy of only `zanpos.db` without a prior checkpoint may be missing the last few committed transactions.**

---

## 2. Built-in Backup Command

The `db_backup` Tauri command (owner-only) performs:

1. RBAC check — only `owner` role may trigger a backup.
2. Resolves `%APPDATA%\com.super.zanpos\zanpos.db` as the source.
3. Copies the file to:
   - A user-supplied path (if provided), or
   - `Documents\zanpos-backup-YYYYMMDD_HHMMSS.db` (auto-generated).
4. Returns the full destination path on success.

**The command issues `PRAGMA wal_checkpoint(TRUNCATE)` before copying**
(`phase10a_commands.rs:82`), flushing all committed WAL transactions into the main
`.db` file first. The resulting backup is therefore complete as of the moment the
command runs — no manual checkpoint or clean shutdown is required.

The daily prune in `sync/worker.rs` also issues `PRAGMA wal_checkpoint(TRUNCATE)`
each day as an additional safety net.

---

## 3. Backup Schedule — Recommendation

| Environment | Frequency | Method |
|---|---|---|
| Limited beta | Daily, end of business | Manual via Back Office or script |
| Production | Every shift close + daily snapshot | Automated via scheduled task (see below) |

### Windows Scheduled Task (PowerShell)

```powershell
# Run at 23:00 daily; copies latest DB to a network share
$src  = "$env:APPDATA\zanpos\zanpos.db"
$dest = "\\fileserver\zanpos-backups\zanpos-$(Get-Date -Format 'yyyyMMdd_HHmmss').db"
Copy-Item $src $dest
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

4. **Rename (not delete) the live DB** as a safety net:
   ```powershell
   Rename-Item "$env:APPDATA\zanpos\zanpos.db" "zanpos.db.broken-$(Get-Date -Format 'yyyyMMdd')"
   ```

5. **Copy the backup into place:**
   ```powershell
   Copy-Item "zanpos-backup-20241201_230000.db" "$env:APPDATA\zanpos\zanpos.db"
   ```

6. **Delete any stale WAL files** (they belong to the broken DB, not the restored one):
   ```powershell
   Remove-Item "$env:APPDATA\zanpos\zanpos.db-wal" -ErrorAction SilentlyContinue
   Remove-Item "$env:APPDATA\zanpos\zanpos.db-shm" -ErrorAction SilentlyContinue
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
| `test_wal_checkpoint_before_backup_captures_all_writes` | WAL checkpoint flushes committed data |

These run in CI on every push to `main`/`master`.
