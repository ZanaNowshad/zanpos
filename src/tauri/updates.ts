import { invoke } from "@tauri-apps/api/core";

export interface CriticalUpdateInfo {
  version: string;
  critical: boolean;
  notes?: string | null;
}

/**
 * Advisory-only peek at the release manifest for the `critical` flag. Never
 * throws to the caller on failure (backend already resolves every failure
 * mode to `null`); this must never be wired into anything that installs an
 * update — that stays exclusively on the signed check_for_updates /
 * download_and_install_update path.
 */
export async function checkCriticalUpdate(): Promise<CriticalUpdateInfo | null> {
  try {
    return await invoke<CriticalUpdateInfo | null>("check_critical_update");
  } catch {
    return null;
  }
}
