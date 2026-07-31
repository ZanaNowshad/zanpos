import React from "react";
import ReactDOM from "react-dom/client";
import App from "./App";
import { logDiagnostic } from "./tauri/diagnostics";

window.addEventListener("error", (e) => {
  logDiagnostic("js_error", e.message, e.error?.stack).catch(() => {});
});
window.addEventListener("unhandledrejection", (e) => {
  const reason = e.reason as { message?: string; stack?: string } | undefined;
  logDiagnostic("js_error", String(reason?.message ?? reason), reason?.stack).catch(() => {});
});

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
);
