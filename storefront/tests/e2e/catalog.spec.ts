import { test, expect, type Page } from "@playwright/test";
import { englishCatalog, arabicCatalog, emptyCatalog, unavailableCatalog } from "./fixtures";
import type { Catalog } from "../../src/types";

// Helper: intercept /api/catalog with deterministic fixture
async function mockCatalog(page: Page, catalog: Catalog) {
  await page.route("**/api/catalog", (route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(catalog) }),
  );
}

// The add button in each product card — text content contains "Add" (English) / "إضافة" (Arabic)
const addButton = (page: Page) => page.locator(".product-card").first().getByRole("button");

test.describe("Catalog — English", () => {
  test("loads English catalog with product grid", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await expect(page.locator("h1")).toContainText("Fresh every day");
    await expect(page.locator(".product-card")).toHaveCount(6);
  });

  test("displays prices with BHD 3-decimal minor-units", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    // Fresh Milk at 600 fils = 0.600 BHD
    await expect(page.locator(".product-card").first().locator("strong")).toContainText("0.600");
  });

  test("search filters products by name", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await page.locator("input[type='search']").fill("Milk");
    await expect(page.locator(".product-card")).toHaveCount(1);
    await expect(page.locator(".product-card h2")).toContainText("Fresh Milk");
  });

  test("category filter shows only matching products", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await page.locator("nav.categories button", { hasText: "Bakery" }).click();
    await expect(page.locator(".product-card")).toHaveCount(2);
  });

  test("adds product to cart and updates total using integer minor units", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    // Add Milk (600 fils) + Bread (200 fils) = 800 fils = 0.800 BHD
    await page.locator(".product-card").nth(0).getByRole("button").click();
    await page.locator(".product-card").nth(1).getByRole("button").click();
    // Open cart
    await page.locator(".ribbon-handle").click();
    const total = page.locator(".cart-total strong");
    await expect(total).toContainText("0.800");
  });

  test("removes product from cart", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await page.locator(".product-card").first().getByRole("button").click();
    await page.locator(".ribbon-handle").click();
    await page.locator(".cart-line .remove").click();
    await expect(page.locator(".cart-empty")).toBeVisible();
  });

  test("cart persists across page reloads via localStorage", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await page.locator(".product-card").first().getByRole("button").click();
    await page.reload();
    await page.locator(".ribbon-handle").click();
    await expect(page.locator(".cart-line")).toHaveCount(1);
  });

  test("WhatsApp checkout link is present when cart has items", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    await page.locator(".product-card").first().getByRole("button").click();
    await page.locator(".ribbon-handle").click();
    const waLink = page.locator("a.whatsapp");
    await expect(waLink).toBeVisible();
    const href = await waLink.getAttribute("href");
    expect(href).toContain("wa.me/97317000000");
  });
});

test.describe("Catalog — Arabic RTL", () => {
  test("loads Arabic catalog with RTL direction", async ({ page }) => {
    await mockCatalog(page, arabicCatalog());
    await page.goto("/");
    // Switch to Arabic
    await page.locator("button.language").click();
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await expect(page.locator("h1")).toContainText("طازج");
  });

  test("Arabic product names displayed after locale switch", async ({ page }) => {
    await mockCatalog(page, arabicCatalog());
    await page.goto("/");
    await page.locator("button.language").click();
    await expect(page.locator(".product-card h2").first()).toContainText("حليب");
  });

  test("Arabic search filters Arabic names", async ({ page }) => {
    await mockCatalog(page, arabicCatalog());
    await page.goto("/");
    await page.locator("button.language").click();
    await page.locator("input[type='search']").fill("حليب");
    await expect(page.locator(".product-card")).toHaveCount(1);
  });

  test("Arabic category labels rendered in Arabic", async ({ page }) => {
    await mockCatalog(page, arabicCatalog());
    await page.goto("/");
    await page.locator("button.language").click();
    await expect(page.locator("nav.categories")).toContainText("ألبان");
  });
});

test.describe("Error and edge cases", () => {
  test("API error fallback renders error state", async ({ page }) => {
    await page.route("**/api/catalog", (route) => route.abort());
    await page.goto("/");
    // The error state shows "Try again" button in English
    await expect(page.getByRole("button", { name: /try again/i })).toBeVisible();
  });

  test("clicking retry after error reloads catalog", async ({ page }) => {
    // First: simulate error
    let firstCall = true;
    await page.route("**/api/catalog", (route) => {
      if (firstCall) { firstCall = false; route.abort(); return; }
      route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(englishCatalog()) });
    });
    await page.goto("/");
    await expect(page.getByRole("button", { name: /try again/i })).toBeVisible();
    await page.getByRole("button", { name: /try again/i }).click();
    await expect(page.locator(".product-card").first()).toBeVisible({ timeout: 15000 });
  });

  test("unavailable products are marked but add button is disabled", async ({ page }) => {
    await mockCatalog(page, unavailableCatalog());
    await page.goto("/");
    await expect(page.locator(".sold-tag")).toHaveCount(6);
    // Each product card's button should be disabled
    await expect(page.locator(".product-card").first().getByRole("button")).toBeDisabled();
  });

  test("stale banner appears when catalog uses cached data", async ({ page }) => {
    // First: load and cache catalog
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    // Wait for catalog to be cached
    await expect(page.locator(".product-card").first()).toBeVisible();
    await page.evaluate(() => localStorage.getItem("zanpos:catalog:v1") !== null);

    // Reload with API failure — should show stale banner
    await page.route("**/api/catalog", (route) => route.abort());
    await page.reload();
    await expect(page.locator(".stale-banner")).toBeVisible({ timeout: 15000 });
  });
});

test.describe("Accessibility", () => {
  test("search input has accessible label", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    const searchInput = page.locator("input[type='search']");
    await expect(searchInput).toHaveAttribute("aria-label");
  });

  test("cart toggle has expanded state", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    const handle = page.locator(".ribbon-handle");
    await expect(handle).toHaveAttribute("aria-expanded", "false");
    await handle.click();
    await expect(handle).toHaveAttribute("aria-expanded", "true");
  });

  test("product add buttons have accessible labels", async ({ page }) => {
    await mockCatalog(page, englishCatalog());
    await page.goto("/");
    const addBtn = page.locator(".product-card").first().getByRole("button");
    await expect(addBtn).toHaveAttribute("aria-label");
  });
});
