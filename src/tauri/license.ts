import { invoke } from "@tauri-apps/api/core";

export type LicenseTier = "core" | "plus";

/**
 * Mirrors `license::LicenseState`, which serialises with `#[serde(tag = "state")]`
 * flattened onto the entitlement — hence the discriminated union rather than a
 * nested object.
 */
export type LicenseState =
  | { state: "active" }
  | { state: "in_grace"; days_remaining: number }
  | { state: "lapsed" };

export type Entitlement = { tier: LicenseTier | null } & LicenseState;

export interface LicenseRecord {
  license_key: string;
  tier: string;
  store_name: string | null;
  issued_at: string;
  expires_at: string | null;
  last_validated_at: string | null;
  grace_until: string | null;
}

export const licenseGetEntitlement = (): Promise<Entitlement> =>
  invoke("license_get_entitlement");

/** Installs a license from raw file content. Fails closed on a bad signature —
 *  nothing is written unless the signature verifies. */
export const licenseImportFile = (content: string): Promise<LicenseRecord> =>
  invoke("license_import_file", { content });
