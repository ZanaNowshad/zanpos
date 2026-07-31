import { invoke } from "@tauri-apps/api/core";

// Client-side throttle so a tight render-error loop can't flood the backend
// with identical log_diagnostic calls before the Rust-side rate limiter even
// sees them — same signature idea (kind + message prefix), 2s window.
const recent = new Map<string, number>();
function throttled(key: string, windowMs = 2000): boolean {
  const last = recent.get(key);
  const now = Date.now();
  if (last && now - last < windowMs) return true;
  recent.set(key, now);
  if (recent.size > 200) {
    const oldest = recent.keys().next().value;
    if (oldest !== undefined) recent.delete(oldest);
  }
  return false;
}

/** Never throws — telemetry reporting must not affect the caller. */
export async function logDiagnostic(
  kind: string,
  message: string,
  stack?: string,
  extraJson?: string,
): Promise<void> {
  const key = `${kind}:${message.slice(0, 120)}`;
  if (throttled(key)) return;
  try {
    await invoke("log_diagnostic", { kind, message, stack, extraJson });
  } catch {
    // never let telemetry reporting throw
  }
}

export async function flushDiagnosticsNow(): Promise<{ ok: boolean; message: string }> {
  return invoke("flush_diagnostics_now");
}
