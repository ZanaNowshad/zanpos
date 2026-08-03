interface StoreTabProps {
  name: string; setName: (v: string) => void;
  timezone: string; setTimezone: (v: string) => void;
  address: string; setAddress: (v: string) => void;
  phone: string; setPhone: (v: string) => void;
  taxNumber: string; setTaxNumber: (v: string) => void;
  crNumber: string; setCrNumber: (v: string) => void;
  setSavedStore: (v: boolean) => void;
  timeoutMinutes: number; setTimeoutMinutes: (v: number) => void;
  saveError: string | null; savedStore: boolean; saving: boolean;
  timeoutError: string | null; savedTimeout: boolean; savingTimeout: boolean;
  handleSave: () => void; handleSaveTimeout: () => void;
  TIMEZONES: string[]; TIMEOUT_OPTIONS: number[]; timeoutLabel: (m: number) => string;
}

export default function StoreTab(props: StoreTabProps) {
  const {
    name, setName, timezone, setTimezone, address, setAddress,
    phone, setPhone, taxNumber, setTaxNumber, crNumber, setCrNumber,
    setSavedStore, timeoutMinutes, setTimeoutMinutes,
    saveError, savedStore, saving, timeoutError, savedTimeout, savingTimeout,
    handleSave, handleSaveTimeout, TIMEZONES, TIMEOUT_OPTIONS, timeoutLabel,
  } = props;

  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">Store Identity</h3>

        <label htmlFor="a11y-input-1" className="bo-label">Store Name *</label>
        <input id="a11y-input-1" className="bo-input" type="text" value={name}
          onChange={e => { setName(e.target.value); setSavedStore(false); }} maxLength={60} />

        <label htmlFor="a11y-input-2" className="bo-label">Timezone</label>
        <select id="a11y-input-2" className="bo-select" value={timezone} onChange={e => setTimezone(e.target.value)}>
          {TIMEZONES.map(tz => <option key={tz} value={tz}>{tz}</option>)}
        </select>

        <label htmlFor="a11y-input-3" className="bo-label">Address</label>
        <textarea id="a11y-input-3" className="bo-input" rows={3} value={address}
          onChange={e => { setAddress(e.target.value); setSavedStore(false); }}
          placeholder="Full address printed on receipts" />

        <label htmlFor="a11y-input-4" className="bo-label">Phone Number</label>
        <input id="a11y-input-4" className="bo-input" type="tel" value={phone}
          onChange={e => { setPhone(e.target.value); setSavedStore(false); }}
          placeholder="+973 1234 5678" />

        <div className="bo-row-two" style={{ marginTop: 0 }}>
          <div>
            <label htmlFor="a11y-input-5" className="bo-label">Tax / VAT Registration Number</label>
            <input id="a11y-input-5" className="bo-input" type="text" value={taxNumber}
              onChange={e => { setTaxNumber(e.target.value); setSavedStore(false); }}
              placeholder="e.g. VAT-1234567890" />
          </div>
          <div>
            <label htmlFor="a11y-input-6" className="bo-label">Commercial Register (CR No)</label>
            <input id="a11y-input-6" className="bo-input" type="text" value={crNumber}
              onChange={e => { setCrNumber(e.target.value); setSavedStore(false); }}
              placeholder="e.g. 12345-1" />
          </div>
        </div>
      </section>

      <hr className="settings-page-divider" />

      <section>
        <h3 className="settings-page-title">Security</h3>

        <label htmlFor="a11y-input-7" className="bo-label">Session Timeout</label>
        <div className="settings-timeout-row">
          <select id="a11y-input-7"
            className="bo-select settings-timeout-select"
            value={timeoutMinutes}
            onChange={e => setTimeoutMinutes(Number(e.target.value))}
          >
            {TIMEOUT_OPTIONS.map(m => (
              <option key={m} value={m}>{timeoutLabel(m)}</option>
            ))}
          </select>
        </div>
        <p className="settings-hint">
          {timeoutMinutes === 0
            ? "The session will never lock automatically. Staff stay logged in indefinitely."
            : `Lock the screen after ${timeoutMinutes} minute${timeoutMinutes !== 1 ? "s" : ""} of inactivity.`
          }
        </p>
      </section>

      <div className="settings-page-actions">
        <div className="settings-action-group">
          {saveError && <span className="settings-action-msg settings-action-err">{saveError}</span>}
          {savedStore && <span className="settings-action-msg">✓ Saved</span>}
          <button className="btn-primary" onClick={handleSave} disabled={saving}>
            {saving ? "Saving…" : "Save"}
          </button>
        </div>
        <div className="settings-action-group">
          {timeoutError && <span className="settings-action-msg settings-action-err">{timeoutError}</span>}
          {savedTimeout && <span className="settings-action-msg">✓ Saved</span>}
          <button className="btn-primary" onClick={handleSaveTimeout} disabled={savingTimeout}>
            {savingTimeout ? "Saving…" : "Save Timeout"}
          </button>
        </div>
      </div>
    </div>
  );
}
