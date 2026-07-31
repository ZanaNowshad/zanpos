import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { App } from "./App";

const catalog = {
  version: "2026-07-23T1",
  updatedAt: new Date().toISOString(),
  store: { name: { en: "Palm Pantry", ar: "مؤونة النخيل" }, phone: "97330000000" },
  currency: { code: "BHD", decimals: 3 },
  categories: [{ id: "pantry", name: { en: "Pantry", ar: "المؤونة" } }],
  products: [
    {
      id: "dates",
      name: { en: "Khalas dates", ar: "تمر خلاص" },
      categoryId: "pantry",
      priceMinor: 2250,
      available: true,
      quantityDecimals: 3,
    },
    {
      id: "sold",
      name: { en: "Saffron", ar: "زعفران" },
      categoryId: "pantry",
      priceMinor: 8000,
      available: false,
      quantityDecimals: 0,
    },
  ],
};

describe("public storefront", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.stubGlobal("fetch", vi.fn().mockResolvedValue({
      ok: true,
      json: async () => catalog,
    }));
  });

  it("loads, searches, filters unavailable products, and persists decimal cart quantities", async () => {
    const user = userEvent.setup();
    render(<App />);
    expect(screen.getByRole("status")).toHaveTextContent("Loading");
    expect(await screen.findByText("Khalas dates")).toBeVisible();
    expect(screen.getByText("Powered by ZanShop")).toBeVisible();
    expect(screen.queryByRole("link", { name: "Powered by ZanShop" })).not.toBeInTheDocument();
    expect(screen.getByText("Unavailable")).toBeVisible();
    await user.click(screen.getByRole("button", { name: /add khalas dates/i }));
    await user.clear(screen.getByLabelText(/quantity for khalas dates/i));
    await user.type(screen.getByLabelText(/quantity for khalas dates/i), "1.5");
    expect(localStorage.getItem("zanpos:cart:v1")).toContain("1.5");
    await user.type(screen.getByRole("searchbox"), "nothing");
    expect(screen.getByText("No products found")).toBeVisible();
  });

  it("links the ZanShop credit only when an HTTPS URL is configured", async () => {
    render(<App zanShopUrl="https://brand.example/zan-shop" />);
    await screen.findByText("Khalas dates");
    expect(screen.getByRole("link", { name: "Powered by ZanShop" }))
      .toHaveAttribute("href", "https://brand.example/zan-shop");
  });

  it("switches Arabic direction and exposes a WhatsApp checkout link", async () => {
    const user = userEvent.setup();
    render(<App />);
    await screen.findByText("Khalas dates");
    await user.click(screen.getByRole("button", { name: /العربية/i }));
    expect(document.documentElement).toHaveAttribute("dir", "rtl");
    await user.click(screen.getByRole("button", { name: /أضف تمر خلاص/i }));
    await waitFor(() => expect(screen.getByRole("link", { name: /واتساب/i })).toHaveAttribute("href", expect.stringContaining("wa.me")));
  });
});
