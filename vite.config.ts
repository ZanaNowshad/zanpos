import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

// @ts-expect-error process is a nodejs global
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig(async () => ({
  plugins: [react()],

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
  build: {
    chunkSizeWarningLimit: 200,
    // F-MED-07: Strip console.* and debugger in production to prevent
    // debug output in production builds and minor stack trace leakage.
    minify: "esbuild",
    rollupOptions: {
      output: {
        manualChunks(id) {
          // Vendor: React core (the object form wasn't splitting these
          // properly — use a function that matches resolved paths).
          if (id.includes("node_modules/react-dom/") || id.includes("node_modules/react-dom.") ||
              id.includes("node_modules/react/") || id.includes("node_modules/react.")) {
            return "vendor";
          }
          // Icon library — tree-shaken, but still sizeable when many icons are used.
          if (id.includes("node_modules/lucide-react/")) {
            return "icons";
          }
          // Barcode library (only used by BarcodesPrintModal — keep it lazy).
          if (id.includes("node_modules/jsbarcode/")) {
            return "jsbarcode";
          }
          // Tauri IPC runtime — shared across all chunks.
          if (id.includes("node_modules/@tauri-apps/")) {
            return "tauri-api";
          }
          // SetupWizard and its sub-components — already lazy‑loaded by App.tsx,
          // but the function form ensures its deps stay in its own chunk.
          if (id.includes("/pages/SetupWizard") || id.includes("/setup/")) {
            return "setup-wizard";
          }
          // MigrationAgent — lazy-loaded, keep isolated.
          if (id.includes("/pages/MigrationAgentPage")) {
            return "migration";
          }
          // Admin/back-office tabs and modals — loaded via BackOfficeModal.
          // Splitting these keeps the main POS chunk lean (<200 kB).
          // BackOffice tabs, modals, and POS utility modals.
          // All in one chunk to avoid circular imports between modal ↔ tab components.
          // Keeps the main POS chunk under 200 kB.
          if (id.includes("/components/ProductsTab") ||
              id.includes("/components/CategoriesTab") ||
              id.includes("/components/ProductFormModal") ||
              id.includes("/components/BulkStockTakeModal") ||
              id.includes("/components/BarcodesPrintModal") ||
              id.includes("/components/ReportsTab") ||
              id.includes("/components/CashierReportTab") ||
              id.includes("/components/EodCashupTab") ||
              id.includes("/components/TodayReportModal") ||
              id.includes("/components/XReportModal") ||
              id.includes("/components/RecentSalesModal") ||
              id.includes("/components/UsersTab") ||
              id.includes("/components/SettingsTab") ||
              id.includes("/components/AuditLogTab") ||
              id.includes("/components/CustomersTab") ||
              id.includes("/components/DevicesTab") ||
              id.includes("/components/InventoryTab") ||
              id.includes("/components/SyncQueueModal") ||
              id.includes("/components/DeliveriesTab") ||
              id.includes("/components/DeliveryForm") ||
              id.includes("/components/RefundModal") ||
              id.includes("/components/HoldModal") ||
              id.includes("/components/DiscountModal") ||
              id.includes("/components/LineDiscountModal") ||
              id.includes("/components/LineEditModal") ||
              id.includes("/components/CustomItemModal") ||
              id.includes("/components/PriceInputModal") ||
              id.includes("/components/PaymentModal") ||
              id.includes("/components/ReceiptPreview")) {
            return "backoffice";
          }
        },
      },
    },
  },
  esbuild: {
    drop: ["console", "debugger"],
  },
}));
