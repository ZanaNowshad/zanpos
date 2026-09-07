import { useCallback, useEffect, useRef, useState } from "react";
import { DEVICE } from "../../../types";
import {
  settingsGetBranch,
  settingsUpdateBranch,
  appConfigGetTimeout,
  appConfigSetTimeout,
  appConfigLoad,
} from "../../../tauri/commands";
import StoreTab from "../../../components/settings/StoreTab";
import type { SessionToken } from "../../../types";

const TIMEZONES = [
  "Asia/Bahrain", "Asia/Riyadh", "Asia/Dubai", "Asia/Kuwait", "Asia/Muscat",
  "Asia/Qatar", "Africa/Cairo", "Europe/London", "America/New_York",
  "America/Los_Angeles", "Asia/Singapore",
];
const TIMEOUT_OPTIONS = [0, 1, 2, 5, 10, 15, 30, 60];
function timeoutLabel(m: number) { return m === 0 ? "Off — Never lock" : `${m} minute${m !== 1 ? "s" : ""}`; }

interface Props { sessionToken: SessionToken; sessionRole?: string; }

export default function StoreIdentityPage({ sessionToken, sessionRole }: Props) {
  const [name, setName] = useState("");
  const [timezone, setTimezone] = useState("Asia/Bahrain");
  const [address, setAddress] = useState("");
  const [phone, setPhone] = useState("");
  const [taxNumber, setTaxNumber] = useState("");
  const [crNumber, setCrNumber] = useState("");
  const [timeoutMinutes, setTimeoutMinutes] = useState(5);
  const [saving, setSaving] = useState(false);
  const [savingTimeout, setSavingTimeout] = useState(false);
  const [savedStore, setSavedStore] = useState(false);
  const [savedTimeout, setSavedTimeout] = useState(false);
  const [saveError, setSaveError] = useState<string | null>(null);
  const [timeoutError, setTimeoutError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const timerRefs = useRef<ReturnType<typeof setTimeout>[]>([]);
  const registerTimer = useCallback((t: ReturnType<typeof setTimeout>) => { timerRefs.current.push(t); }, []);
  useEffect(() => () => { timerRefs.current.forEach(clearTimeout); timerRefs.current = []; }, []);

  useEffect(() => {
    Promise.all([settingsGetBranch(sessionToken), appConfigGetTimeout().catch(() => 5)])
      .then(([branch, mins]) => {
        setName(branch.name); setTimezone(branch.timezone);
        setAddress(branch.address ?? ""); setPhone(branch.phone ?? "");
        setTaxNumber(branch.tax_number ?? ""); setCrNumber(branch.cr_number ?? "");
        setTimeoutMinutes(mins as number);
      })
      .catch(() => setSaveError("Failed to load settings"))
      .finally(() => setLoading(false));
  }, [sessionToken]);

  const handleSave = async () => {
    if (!name.trim()) { setSaveError("Store name is required"); return; }
    setSaving(true); setSaveError(null);
    try {
      await settingsUpdateBranch({
        name: name.trim(), timezone, address: address.trim() || undefined,
        phone: phone.trim() || undefined, tax_number: taxNumber.trim() || undefined,
        cr_number: crNumber.trim() || undefined,
      }, sessionToken);
      // Pull the saved values back into the in-memory DEVICE singleton.
      //
      // It was populated once at startup and never again, so renaming the store
      // mid-shift left the old name on the POS top bar, on every shift and
      // cash-drawer receipt, and in outbound WhatsApp messages until the app was
      // restarted — while Settings showed the new one and reported "Saved".
      // `usePosReceipt` already documents this discipline for the sale receipt;
      // this applies it to the rest.
      try {
        DEVICE.init(await appConfigLoad());
      } catch {
        // The save succeeded; a failed refresh is cosmetic until the next load.
      }
      setSavedStore(true); registerTimer(setTimeout(() => setSavedStore(false), 3000));
    } catch (e: unknown) { setSaveError(typeof e === "string" ? e : "Failed to save"); }
    finally { setSaving(false); }
  };

  const handleSaveTimeout = async () => {
    setSavingTimeout(true); setTimeoutError(null);
    try { await appConfigSetTimeout(timeoutMinutes, sessionToken); setSavedTimeout(true); registerTimer(setTimeout(() => setSavedTimeout(false), 3000)); }
    catch (e: unknown) { setTimeoutError(typeof e === "string" ? e : "Failed to save"); }
    finally { setSavingTimeout(false); }
  };

  if (loading) return <div className="bo-empty">Loading store settings…</div>;
  return (
    <StoreTab
      name={name} setName={setName} timezone={timezone} setTimezone={setTimezone}
      address={address} setAddress={setAddress} phone={phone} setPhone={setPhone}
      taxNumber={taxNumber} setTaxNumber={setTaxNumber} crNumber={crNumber} setCrNumber={setCrNumber}
      setSavedStore={setSavedStore} timeoutMinutes={timeoutMinutes} setTimeoutMinutes={setTimeoutMinutes}
      saveError={saveError} savedStore={savedStore} saving={saving}
      timeoutError={timeoutError} savedTimeout={savedTimeout} savingTimeout={savingTimeout}
      handleSave={handleSave} handleSaveTimeout={handleSaveTimeout}
      TIMEZONES={TIMEZONES} TIMEOUT_OPTIONS={TIMEOUT_OPTIONS} timeoutLabel={timeoutLabel}
      canEditBranch={sessionRole === "owner"}
    />
  );
}
