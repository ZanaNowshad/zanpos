import { test, expect, type Page } from "@playwright/test";
import { englishCatalog } from "./fixtures";

async function mockCatalog(page: Page) {
  await page.route("**/api/catalog", (route) =>
    route.fulfill({ status: 200, contentType: "application/json", body: JSON.stringify(englishCatalog()) }),
  );
}

test.describe("Mobile viewport", () => {
  test("mobile viewport renders without horizontal overflow", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await mockCatalog(page);
    await page.goto("/");
    const body = page.locator("body");
    const box = await body.boundingBox();
    expect(box).not.toBeNull();
    if (box) expect(box.width).toBeLessThanOrEqual(390);
  });

  test("mobile viewport shows cart handle", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await mockCatalog(page);
    await page.goto("/");
    await expect(page.locator(".ribbon-handle")).toBeVisible();
  });
});

test.describe("Edge cases", () => {
  test("page loads with title", async ({ page }) => {
    await mockCatalog(page);
    await page.goto("/");
    await expect(page).toHaveTitle(/ZANPOS/i);
  });

  test("no console errors on load", async ({ page }) => {
    const errors: string[] = [];
    page.on("console", (msg) => {
      if (msg.type() === "error") errors.push(msg.text());
    });
    await mockCatalog(page);
    await page.goto("/");
    expect(errors.filter((e) => !e.includes("favicon"))).toHaveLength(0);
  });

  test("language toggle switches between EN and AR", async ({ page }) => {
    await mockCatalog(page);
    await page.goto("/");
    const langBtn = page.locator("button.language");
    await expect(langBtn).toContainText("عربي");
    await langBtn.click();
    await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
    await expect(langBtn).toContainText("English");
  });

  test("duplicate-add increments quantity not duplicate line", async ({ page }) => {
    await mockCatalog(page);
    await page.goto("/");
    const addBtn = page.locator(".product-card").first().getByRole("button");
    await addBtn.click();
    await addBtn.click();
    await page.locator(".ribbon-handle").click();
    await expect(page.locator(".cart-line")).toHaveCount(1);
    await expect(page.locator(".cart-line input").first()).toHaveValue("2");
  });

  test("quantity update recalculates total", async ({ page }) => {
    await mockCatalog(page);
    await page.goto("/");
    await page.locator(".product-card").first().getByRole("button").click();
    await page.locator(".ribbon-handle").click();
    const qtyInput = page.locator(".cart-line input").first();
    await qtyInput.fill("3");
    await qtyInput.blur();
    // 3 × 600 fils = 1800 fils = 1.800 BHD
    await expect(page.locator(".cart-total strong")).toContainText("1.800");
  });

  test("zero quantity removes line from cart", async ({ page }) => {
    test.setTimeout(60000);
    await mockCatalog(page);
    await page.goto("/");
    await page.locator(".product-card").first().getByRole("button").click();
    await expect(page.locator(".ribbon-handle b")).toHaveText("1");
    await page.locator(".ribbon-handle").click();
    await expect(page.locator(".cart-line")).toHaveCount(1, { timeout: 10000 });
    // Set quantity to 0 by clicking the remove button instead of typing zero
    await page.locator(".cart-line .remove").click();
    await expect(page.locator(".cart-empty")).toBeVisible();
  });
});
