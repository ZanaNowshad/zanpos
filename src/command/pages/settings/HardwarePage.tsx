import { useCallback, useEffect, useState } from "react";
import type { ThermalConfig } from "../../../types";
import type { PortEntry } from "../../../tauri/commands";
import { thermalListPorts, thermalGetConfig, thermalSetConfig, thermalPrintTest } from "../../../tauri/commands";
import PrinterTab from "../../../components/settings/PrinterTab";
import type { SessionToken } from "../../../types";

interface Props { sessionToken: SessionToken; }

export default function HardwarePage({ sessionToken }: Props) {
  const [thermal, setThermal] = useState<ThermalConfig>({ enabled: false, port: "", baud: "9600" });
  const [ports, setPorts] = useState<PortEntry[]>([]);
  const [portsLoading, setPortsLoading] = useState(false);
  const [saving, setSaving] = useState(false); const [saved, setSaved] = useState(false);
  const [testing, setTesting] = useState(false); const [testMsg, setTestMsg] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  const loadPorts = useCallback(async () => {
    setPortsLoading(true);
    try {
      const p = await thermalListPorts(); setPorts(p);
      setThermal(prev => { if (prev.port === "") { const d = p.find(x => x.is_default) ?? p[0]; return d ? { ...prev, port: d.port } : prev; } return prev; });
    } finally { setPortsLoading(false); }
  }, []);

  useEffect(() => {
    thermalGetConfig(sessionToken).then(tc => setThermal(tc)).catch(() => {});
    loadPorts().finally(() => setLoading(false));
  }, [sessionToken, loadPorts]);

  const handleSave = async () => {
    setSaving(true); setTestMsg(null);
    try { await thermalSetConfig(sessionToken, thermal); setSaved(true); setTimeout(() => setSaved(false), 3000); }
    catch (e: unknown) { setTestMsg(typeof e === "string" ? e : "Failed to save"); }
    finally { setSaving(false); }
  };

  const handleTest = async () => {
    setTesting(true); setTestMsg(null);
    try { setTestMsg(await thermalPrintTest(sessionToken)); }
    catch (e: unknown) { setTestMsg(typeof e === "string" ? e : "Test failed"); }
    finally { setTesting(false); }
  };

  if (loading) return <div className="bo-empty">Loading printer settings…</div>;
  return (
    <PrinterTab thermal={thermal} setThermal={setThermal} availablePorts={ports} portsLoading={portsLoading}
      savingThermal={saving} savedThermal={saved} testingPrint={testing} printTestMsg={testMsg}
      handleSaveThermal={handleSave} handleTestPrint={handleTest} loadPorts={loadPorts} />
  );
}
