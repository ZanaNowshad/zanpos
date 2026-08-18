import type { AdminProduct } from "../types";
import * as cmd from "../tauri/commands";
import { downloadCsv, productsToCsv } from "./productsCsv";

/**
 * Export every product the current filters select — not the visible page.
 *
 * The catalogue list is paginated at 100, so writing out the loaded page would
 * quietly hand back one page of a catalogue that may run to thousands and look
 * complete. The pages are walked here instead, using the same search and
 * category the screen is showing, and the file is written in the import
 * template's exact shape so it can be edited and imported straight back.
 *
 * Lives outside the component because it is a data operation, not a rendering
 * one: nothing here touches React state, and the caller owns the busy flag.
 */
export async function exportProductCatalogue(options: {
  sessionUserId: string;
  search: string;
  categoryFilter: string;
  /** Applied client-side, exactly as the status chip does, so the file matches
   *  what the screen claims to be showing. */
  statusFilter: "" | "active" | "inactive";
  currencyExponent: number;
  pageSize: number;
}): Promise<void> {
  const all: AdminProduct[] = [];
  for (let off = 0; ; off += options.pageSize) {
    const page = await cmd.adminListProducts(options.sessionUserId, {
      search: options.search,
      categoryId: options.categoryFilter || undefined,
      offset: off,
      limit: options.pageSize,
    });
    all.push(...page.items);
    if (all.length >= page.total || page.items.length === 0) break;
  }

  const rows = options.statusFilter
    ? all.filter(p => (options.statusFilter === "active" ? p.is_active : !p.is_active))
    : all;
  const stamp = new Date().toLocaleDateString("en-CA", { timeZone: "Asia/Bahrain" });
  downloadCsv(`products_export_${stamp}.csv`, productsToCsv(rows, options.currencyExponent));
}
