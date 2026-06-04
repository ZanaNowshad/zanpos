import type { Dispatch, SetStateAction } from "react";
import type { ThermalConfig } from "../../types";
import type { PortEntry } from "../../tauri/commands";

interface PrinterTabProps {
  thermal: ThermalConfig; setThermal: Dispatch<SetStateAction<ThermalConfig>>;
  availablePorts: PortEntry[]; portsLoading: boolean;
  savingThermal: boolean; savedThermal: boolean;
  testingPrint: boolean; printTestMsg: string | null;
  handleSaveThermal: () => void; handleTestPrint: () => void;
  loadPorts: () => void;
}

export default function PrinterTab(props: PrinterTabProps) {
  const {
    thermal, setThermal, availablePorts, portsLoading,
    savingThermal, savedThermal, testingPrint, printTestMsg,
    handleSaveThermal, handleTestPrint, loadPorts,
  } = props;

  return (
    <div className="settings-page">
      <section>
        <h3 className="settings-page-title">Receipt Printer (ESC/POS)</h3>
        <p className="settings-hint">
          Select a port from the printers detected on this device.
          Click <strong>Scan</strong> if your printer isn't showing — make sure it's powered on and connected.
        </p>

        <label className="bo-checkbox-label" style={{ marginBottom: "14px" }}>
          <input type="checkbox" checked={thermal.enabled}
            onChange={e => setThermal(t => ({ ...t, enabled: e.target.checked }))} />
          Enable thermal printing
        </label>

        <div className="bo-row-two">
          <div>
            <label className="bo-label">
              Printer
              <button type="button" className="printer-refresh-btn" onClick={loadPorts} disabled={portsLoading}
                title="Scan for connected printers">
                {portsLoading ? "…" : "↺ Scan"}
              </button>
            </label>
            {availablePorts.length === 0 && !portsLoading ? (
              <div className="printer-no-ports">
                No printers detected.<br />
                <span className="rpt-dim">Make sure printer is on and connected, then click ↺ Scan.</span>
              </div>
            ) : (
              <select className="bo-select" value={thermal.port}
                onChange={e => setThermal(t => ({ ...t, port: e.target.value }))}
                disabled={!thermal.enabled || portsLoading}>
                <option value="">— select printer —</option>
                {availablePorts.map(p => (
                  <option key={p.port} value={p.port}>{p.label}</option>
                ))}
                {thermal.port && !availablePorts.some(p => p.port === thermal.port) && (
                  <option value={thermal.port}>{thermal.port} ⚠ (saved — not detected)</option>
                )}
              </select>
            )}
          </div>
          <div>
            <label className="bo-label">
              Baud Rate
              <span className="printer-baud-hint">(serial only)</span>
            </label>
            <select className="bo-select" value={thermal.baud}
              onChange={e => setThermal(t => ({ ...t, baud: e.target.value }))}
              disabled={!thermal.enabled}>
              <option value="9600">9600</option>
              <option value="19200">19200</option>
              <option value="38400">38400</option>
              <option value="115200">115200</option>
            </select>
          </div>
        </div>

        {thermal.port && (() => {
          const entry = availablePorts.find(p => p.port === thermal.port);
          return (
            <div className="printer-selected-badge">
              ✓ Selected: <strong>{entry?.label ?? thermal.port}</strong>
              {entry?.is_default && <span className="printer-default-tag">System Default</span>}
            </div>
          );
        })()}
      </section>

      <div className="settings-page-actions">
        <div className="settings-action-group">
          <button className="btn-primary" onClick={handleSaveThermal} disabled={savingThermal}>
            {savingThermal ? "Saving…" : savedThermal ? "✓ Saved" : "Save"}
          </button>
        </div>
        <div className="settings-action-group">
          <button className="btn-secondary" onClick={handleTestPrint}
            disabled={testingPrint || !thermal.enabled || !thermal.port}>
            {testingPrint ? "Testing…" : "Test Print"}
          </button>
        </div>
        {printTestMsg && <pre className="thermal-test-msg">{printTestMsg}</pre>}
      </div>
    </div>
  );
}
