import { invoke } from "@tauri-apps/api/core";
import type { SessionToken } from "../types";

export interface BackupStatus {
  /** False when no storefront worker is configured — nothing is backed up. */
  configured: boolean;
  /** True when the key derives from an imported licence, so the vendor can
   *  help recover after the machine itself is gone. False means the key lives
   *  only in this machine's credential store — and dies with it. */
  recoverable_from_license: boolean;
}

export const backupStatus = (actorUserId: string): Promise<BackupStatus> =>
  invoke("backup_status", { actorUserId });

export const backupRunNow = (actorUserId: string): Promise<boolean> =>
  invoke("backup_run_now", { actorUserId });

/**
 * Decrypts a downloaded backup to `destPath`.
 *
 * Writes to a NEW file and refuses to overwrite an existing one: a restore
 * that clobbers a working store because someone picked the wrong file is a
 * worse outage than the one it was meant to fix. Swapping the file in is a
 * separate, deliberate step taken with the app closed.
 */
export const backupRestoreFile = (
  sessionToken: SessionToken | null,
  encryptedPath: string,
  destPath: string,
  licenseKey?: string,
): Promise<string> =>
  invoke("backup_restore_file", { sessionToken, encryptedPath, destPath, licenseKey });
