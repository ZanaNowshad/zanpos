import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { logDiagnostic } from "./tauri/diagnostics";
import { uiMockEnabled } from "./dev/uiMockFlag";

window.addEventListener("error", (e) => {
  logDiagnostic("js_error", e.message, e.error?.stack).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  const reason = e.reason as { message?: string; stack?: string } | undefined;
  logDiagnostic("js_error", String(reason?.message ?? reason), reason?.stack).catch(() => {});
});

function render() {
  ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
    <React.StrictMode>
      <App />
    </React.StrictMode>,
  );
}

// The mock must be installed before React mounts, because App loads its config
// on first render. Dynamic import keeps the mock (and its sample data) out of
// production bundles entirely — see src/dev/uiMockFlag.ts.
if (uiMockEnabled()) {
  import("./dev/uiMock").then(m => { m.installUiMock(); render(); });
} else {
  render();
}
