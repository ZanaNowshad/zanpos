import type { ToolCallEntry, ToolMetaEntry } from "./officeAiTypes";
import { Settings } from "lucide-react";

export function toolMeta(name: string): ToolMetaEntry {
  return { Icon: Settings, label: name.replace(/_/g, " ").replace(/\b\w/g, c => c.toUpperCase()), color: "var(--text-dim)" };
}

export function ToolCallCard({ entry }: { entry: ToolCallEntry }) {
  const meta = toolMeta(entry.name);
  const Icon = meta.Icon;
  return <div className="ai-tool-card"><Icon size={14} style={{ color: meta.color }} /><span className="ai-tool-label">{meta.label}</span></div>;
}

export function LiveActivityBar({ calls }: { calls: ToolCallEntry[] }) {
  if (calls.length === 0) return null;
  return <div className="ai-live-bar">{calls.map(c => <ToolCallCard key={c.id} entry={c} />)}</div>;
}
