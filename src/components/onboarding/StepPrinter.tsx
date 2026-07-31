import { useCallback, useEffect, useState } from "react";
import type { ThermalConfig } from "../../types";
import { thermalListPorts, thermalGetConfig, thermalSetConfig, thermalPrintTest, type PortEntry } from "../../tauri/commands";
import PrinterTab from "../settings/PrinterTab";

interface Props {
  ownerUserId: string;
  onDone: () => void;
}

/** Step 5 — printer setup. Renders the existing PrinterTab presentational
 * component, wired up the same way SettingsTab does, plus an explicit
 * "no printer" choice as required by the onboarding spec. */
export default function StepPrinter({ ownerUserId, onDone }: Props) {
  const [thermal, setThermal] = useState<ThermalConfig>({ enabled: false, port: "", baud: "9600" });
  const [availablePorts, setAvailablePorts] = useState<PortEntry[]>([]);
  const [portsLoading, setPortsLoading] = useState(false);
  const [savingThermal, setSavingThermal] = useState(false);
  const [savedThermal, setSavedThermal] = useState(false);
  const [testingPrint, setTestingPrint] = useState(false);
  const [printTestMsg, setPrintTestMsg] = useState<string | null>(null);

  const loadPorts = useCallback(async () => {
    setPortsLoading(true);
    try {
      const ports = await thermalListPorts();
      setAvailablePorts(ports);
      setThermal(prev => {
        if (prev.port === "") {
          const def = ports.find(p => p.is_default) ?? ports[0];
          return def ? { ...prev, port: def.port } : prev;
        }
        return prev;
      });
    } finally {
      setPortsLoading(false);
    }
  }, []);

  useEffect(() => {
    thermalGetConfig(ownerUserId)
      .then(tc => setThermal(tc))
      .catch(() => { /* keep defaults */ })
      .finally(() => loadPorts());
  }, [ownerUserId, loadPorts]);

  const saveThermal = async (config: ThermalConfig) => {
    setSavingThermal(true);
    setPrintTestMsg(null);
    try {
      await thermalSetConfig(ownerUserId, config);
      setThermal(config);
      setSavedThermal(true);
      setTimeout(() => setSavedThermal(false), 3000);
    } catch (e: unknown) {
      setPrintTestMsg(typeof e === "string" ? e : "Failed to save printer settings");
    } finally {
      setSavingThermal(false);
    }
  };

  const handleSaveThermal = () => { void saveThermal(thermal); };

  const handleTestPrint = async () => {
    setTestingPrint(true);
    setPrintTestMsg(null);
    try {
      const msg = await thermalPrintTest(ownerUserId);
      setPrintTestMsg(msg);
    } catch (e: unknown) {
      setPrintTestMsg(typeof e === "string" ? e : "Test failed");
    } finally {
      setTestingPrint(false);
    }
  };

  const handleNoPrinter = async () => {
    await saveThermal({ enabled: false, port: "", baud: thermal.baud });
    onDone();
  };

  return (
    <div className="setup-content">
      <h2 className="setup-title">Receipt Printer</h2>
      <p className="setup-body">Pick a printer and print a test receipt, or skip if you don't have one yet.</p>

      <PrinterTab
        thermal={thermal} setThermal={setThermal}
        availablePorts={availablePorts} portsLoading={portsLoading}
        savingThermal={savingThermal} savedThermal={savedThermal}
        testingPrint={testingPrint} printTestMsg={printTestMsg}
        handleSaveThermal={handleSaveThermal} handleTestPrint={handleTestPrint}
        loadPorts={loadPorts}
      />

      <div className="setup-actions">
        <button className="setup-btn-secondary" onClick={handleNoPrinter} disabled={savingThermal}>
          No printer — skip for now
        </button>
        <button className="setup-btn-primary" onClick={onDone}>Continue <span className="icon-directional" aria-hidden="true">→</span></button>
      </div>
    </div>
  );
}
