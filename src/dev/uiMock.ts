/**
 * Dev-only IPC mock for visual QA in a plain browser.
 *
 * ZANPOS is a Tauri app: every data call goes through the Rust backend, so
 * opening the Vite dev server in a browser normally stops at "Failed to
 * initialise the database". That makes it impossible to screenshot or inspect
 * the UI without the native window, which in turn makes layout regressions
 * expensive to catch.
 *
 * Activating this mock stubs `window.__TAURI_INTERNALS__.invoke` with canned
 * responses so the real components render against realistic data.
 *
 * Activate with:  http://localhost:1420/?uimock=1
 * Guarded by `import.meta.env.DEV` — it is stripped from production builds and
 * cannot be switched on in a packaged app.
 */

import {
  BRANCH_ID,
  CUSTOMERS,
  DEVICE_ID,
  HANDLERS,
  MOCK_SESSION,
  PRODUCTS,
  cartLine,
} from "./uiMockData";
import type { MockCart } from "./uiMockData";

/** Commands whose names imply a list, so an unknown one should yield []. */
function emptyFor(cmd: string): unknown {
  // Fidelity matters more than convenience here: the previous fallback
  // returned `null` for anything it did not recognise, so an unstubbed
  // array-returning command (sync_stock_drift_report) handed `null` to a
  // component that correctly assumed `StockDriftRow[]`, and the workspace
  // crashed on `.length`. Production never returns null for those commands.
  // Widened patterns cover report/summary/history/queue style names too.
  if (/_list$|^list_|_rows$|_all$|_report$|_history$|_queue$|_items$|s$/.test(cmd)) return [];
  return null;
}

export function installUiMock(): void {
  const w = window as unknown as {
    __TAURI_INTERNALS__?: Record<string, unknown>;
    __zpMockSession?: unknown;
  };
  w.__zpMockSession = MOCK_SESSION;

  let callbackId = 0;

  // The event API unsubscribes through this global, which the Tauri runtime
  // installs but a hand-rolled mock does not. Without it every `listen()`
  // cleanup threw "Cannot read properties of undefined (reading
  // 'unregisterListener')" on unmount — a page error that exists only under the
  // mock and would otherwise be mistaken for an application defect.
  const listeners = new Map<string, Set<number>>();
  (w as unknown as Record<string, unknown>).__TAURI_EVENT_PLUGIN_INTERNALS__ = {
    registerListener: (event: string, id: number) => {
      if (!listeners.has(event)) listeners.set(event, new Set());
      listeners.get(event)!.add(id);
    },
    unregisterListener: (event: string, id: number) => {
      listeners.get(event)?.delete(id);
    },
  };

  w.__TAURI_INTERNALS__ = {
    ...(w.__TAURI_INTERNALS__ ?? {}),
    // Shape expected by @tauri-apps/api: getCurrentWindow() reads
    // metadata.currentWindow.label, and Channel/event listeners need
    // transformCallback + a convertFileSrc stub.
    metadata: {
      currentWindow: { label: "main" },
      currentWebview: { windowLabel: "main", label: "main" },
    },
    currentWindow: { label: "main" },
    currentWebview: { windowLabel: "main", label: "main" },
    transformCallback: (cb?: (v: unknown) => void) => {
      const id = ++callbackId;
      (window as unknown as Record<string, unknown>)[`_${id}`] = cb ?? (() => {});
      return id;
    },
    convertFileSrc: (p: string) => p,
    plugins: {},
    invoke: (cmd: string, args?: Record<string, unknown>) => {
      // Honour the query arguments the real backend honours, so search /
      // filter / no-results states can actually be exercised in QA.
      if (cmd === "admin_list_products") {
        const search = String(args?.search ?? "").trim().toLowerCase();
        const categoryId = args?.categoryId ? String(args.categoryId) : "";
        let items = PRODUCTS;
        if (search) {
          items = items.filter(p =>
            p.name.toLowerCase().includes(search) || (p.barcode ?? "").includes(search));
        }
        if (categoryId) items = items.filter(p => p.category_id === categoryId);
        /* Saved views filter here for the same reason they filter next to the
           LIMIT in SQL: a view applied after paging would answer "which of
           these hundred are out of stock", which is not the question asked.
           Mirrors product_view_predicate in admin_commands.rs — if the two
           drift, QA passes on a rule the database does not apply. */
        const view = String(args?.view ?? "").trim();
        if (view && view !== "all") {
          items = items.filter(p => {
            const qty = Number(p.stock_qty);
            switch (view) {
              case "active":       return p.is_active;
              case "inactive":     return !p.is_active;
              case "out_of_stock": return p.track_inventory && Number.isFinite(qty) && qty <= 0;
              case "low_stock":    return p.track_inventory && Number.isFinite(qty) && qty > 0 && qty <= p.reorder_point;
              case "no_barcode":   return !(p.barcode ?? "").trim() && !(p.barcodes ?? []).length;
              case "no_image":     return !(p.image_path ?? "").trim();
              default:             return true;
            }
          });
        }
        /* Paging is part of the contract, not a detail. Returning every row
           while reporting a page-sized total let a paginated picker render
           sixteen rows under a "1-8 of 16" footer and push its own Next button
           out of reach — a state the real command cannot produce. */
        const total = items.length;
        const offset = Number(args?.offset ?? 0) || 0;
        const limit = Number(args?.limit ?? 0) || total;
        return Promise.resolve(structuredClone({
          items: items.slice(offset, offset + limit), total, offset, limit,
        }));
      }
      // The till asks for active riders only; the admin roster asks for all.
      // Honouring the flag here is what makes "a rider who left is not offered
      // at checkout" verifiable rather than assumed.
      if (cmd === "rider_list") {
        const rows = HANDLERS.rider_list as { is_active: boolean }[];
        return Promise.resolve(structuredClone(
          args?.activeOnly ? rows.filter(r => r.is_active) : rows,
        ));
      }
      if (cmd === "ai_list_actions") {
        const statuses = (args?.statuses as string[] | undefined) ?? [];
        const all = HANDLERS.ai_list_actions as { status: string }[];
        const rows = statuses.length ? all.filter(a => statuses.includes(a.status)) : all;
        return Promise.resolve(structuredClone(rows));
      }
      // `customer_list` searches name and phone server-side, exactly as the
      // Rust command does — so the no-results state is reachable in QA rather
      // than simulated.
      // ── POS register entry ───────────────────────────────────────────────
      // Shapes traced from source: auth_list_users -> Vec<UserSummary>
      // (user_id, display_name, username, role_name) and auth_login_pin ->
      // SessionUser. Without these the POS stopped at the register-handoff
      // screen and could never be captured for visual QA.
      if (cmd === "auth_list_users") {
        return Promise.resolve(structuredClone([
          { user_id: "usr_renihal", display_name: "Renihal", username: "renihal", role_name: "owner" },
          { user_id: "usr_ahmed",   display_name: "Ahmed",   username: "ahmed",   role_name: "cashier" },
        ]));
      }
      if (cmd === "auth_login_pin") {
        // The real command rejects a wrong PIN, so the mock does too — an
        // always-succeeding stub would hide the failure path from QA.
        const input = (args?.input ?? {}) as { username?: string; pin?: string };
        if (input.pin !== "1234") {
          return Promise.reject("Incorrect PIN");
        }
        return Promise.resolve(structuredClone({
          ...MOCK_SESSION,
          username: input.username || MOCK_SESSION.username,
          session_expires_at: new Date(Date.now() + 86_400_000).toISOString(),
        }));
      }

      /* ── Cart lifecycle ──────────────────────────────────────────────────
         The real commands take the current cart and hand back the next one, so
         these are pure functions of their arguments — no session state to keep
         in sync. Without them the till could be captured only with an empty
         cart, which put the payment modal (the densest screen in the app, and
         the one with the tender keypad) out of reach of visual QA entirely. */
      if (cmd === "pos_start_cart") {
        const input = (args?.input ?? {}) as Record<string, string>;
        return Promise.resolve(structuredClone({
          cart_id: "crt_mock_01",
          branch_id: input.branch_id ?? BRANCH_ID,
          device_id: input.device_id ?? DEVICE_ID,
          shift_id: input.shift_id ?? "shf_mock_01",
          cashier_user_id: input.cashier_user_id ?? "usr_renihal",
          lines: [],
          bill_discount_minor: 0,
          bill_discount_reason: null,
        }));
      }
      if (cmd === "pos_add_item" || cmd === "pos_add_item_by_barcode") {
        const input = (args?.input ?? {}) as { cart?: MockCart; barcode?: string; product_id?: string; quantity?: string };
        const cart = input.cart;
        if (!cart) return Promise.reject("No cart");
        const product = input.barcode
          ? PRODUCTS.find(p => p.barcode === input.barcode)
          : PRODUCTS.find(p => p.product_id === input.product_id);
        if (!product) return Promise.reject("Product not found");
        const quantity = Number(input.quantity ?? 1) || 1;
        return Promise.resolve(structuredClone({
          ...cart,
          lines: [...cart.lines, cartLine(product, quantity, cart.lines.length)],
        }));
      }
      if (cmd === "pos_finalize_sale") {
        const input = (args?.input ?? {}) as { cart?: MockCart; payments?: { method: string; amount_minor: number; tendered_minor?: number }[] };
        const lines = (input.cart?.lines ?? []).filter(l => !l.voided);
        const net = lines.reduce((sum, l) => sum + l.line_total_minor, 0);
        return Promise.resolve(structuredClone({
          sale_id: "sal_mock_01",
          receipt_number: "R-000241",
          net_total_minor: net,
          tax_total_minor: 0,
          discount_total_minor: 0,
          currency: "BHD",
          payments: (input.payments ?? []).map(p => ({
            method: p.method,
            amount_minor: p.amount_minor,
            change_minor: p.tendered_minor ? Math.max(p.tendered_minor - p.amount_minor, 0) : null,
          })),
          items: lines.map(l => ({ ...l })),
          cashier_name: "Renihal",
          branch_name: "Amwaj AlDair",
          sold_at: new Date().toISOString(),
          business_date: new Date().toISOString().slice(0, 10),
          created_offline: false,
          low_stock_alerts: [],
        }));
      }
      if (cmd === "pos_update_quantity" || cmd === "pos_set_line_price" || cmd === "pos_void_line") {
        const input = (args?.input ?? {}) as { cart?: MockCart; cart_line_id?: string; quantity?: string; price_minor?: number };
        const cart = input.cart;
        if (!cart) return Promise.reject("No cart");
        return Promise.resolve(structuredClone({
          ...cart,
          lines: cart.lines.map(line => {
            if (line.cart_line_id !== input.cart_line_id) return line;
            if (cmd === "pos_void_line") return { ...line, voided: true, line_total_minor: 0 };
            const unit = cmd === "pos_set_line_price" ? (input.price_minor ?? line.unit_price_minor) : line.unit_price_minor;
            const qty = cmd === "pos_update_quantity" ? (Number(input.quantity) || 1) : Number(line.quantity) || 1;
            return { ...line, quantity: String(qty), unit_price_minor: unit, line_total_minor: unit * qty };
          }),
        }));
      }

      if (cmd === "customer_list") {
        const q = String(args?.search ?? "").trim().toLowerCase();
        const matched = q
          ? CUSTOMERS.filter(c =>
              c.name.toLowerCase().includes(q)
              || (c.phone ?? "").toLowerCase().includes(q)
              || (c.email ?? "").toLowerCase().includes(q))
          : CUSTOMERS;
        // Mirrors the real command's page shape, including a `total` that
        // describes the whole match rather than the slice returned.
        const offset = Number(args?.offset ?? 0);
        const limit = Number(args?.limit ?? 50) || 50;
        return Promise.resolve(structuredClone({
          items: matched.slice(offset, offset + limit),
          total: matched.length,
          offset,
          limit,
        }));
      }
      if (cmd === "product_cost_history_list") {
        // Shape mirrors ProductCostChange exactly: no po_id and no branch_id,
        // because the real table has neither.
        return Promise.resolve(structuredClone([
          { cost_history_id: "ch_1", product_id: "prd_1", product_name: "Almarai Fresh Milk 1L",
            old_cost_minor: 400, new_cost_minor: 420, supplier_id: "sup_1",
            supplier_name: "Almarai Bahrain", source: "purchase_order_receive",
            created_at: new Date(Date.now() - 2 * 864e5).toISOString() },
          { cost_history_id: "ch_2", product_id: "prd_5", product_name: "Basmati Rice 5kg",
            old_cost_minor: null, new_cost_minor: 2400, supplier_id: null,
            supplier_name: null, source: "catalog_import",
            created_at: new Date(Date.now() - 9 * 864e5).toISOString() },
        ]));
      }
      if (cmd === "customer_loyalty_summary") {
        const holders = CUSTOMERS.filter(c => c.loyalty_points > 0);
        return Promise.resolve({
          outstanding_points: CUSTOMERS.reduce((n, c) => n + c.loyalty_points, 0),
          holders: holders.length,
          total_customers: CUSTOMERS.length,
          contactable_holders: holders.filter(c => c.phone && c.phone.trim() !== "").length,
        });
      }
      if (cmd === "customer_top_balances") {
        const limit = Number(args?.limit ?? 25) || 25;
        return Promise.resolve(structuredClone(
          CUSTOMERS.filter(c => c.loyalty_points > 0)
            .sort((a, b) => b.loyalty_points - a.loyalty_points || a.name.localeCompare(b.name))
            .slice(0, limit)));
      }
      if (cmd === "__force_error__") return Promise.reject(new Error("forced"));

      const known = Object.prototype.hasOwnProperty.call(HANDLERS, cmd);
      const hit = known ? HANDLERS[cmd] : emptyFor(cmd);
      // Recorded so a reviewer can see exactly which commands are unstubbed.
      const log = ((window as unknown as { __uimockCalls?: string[] }).__uimockCalls ??= []);
      log.push(`${known ? "ok  " : "MISS"} ${cmd}`);
      return Promise.resolve(structuredClone(hit));
    },
  };

  console.warn("[uimock] Tauri IPC mocked — visual QA mode. NOT a real backend.");
}
