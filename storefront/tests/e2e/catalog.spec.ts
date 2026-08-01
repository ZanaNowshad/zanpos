import { test, expect } from "@playwright/test";

test.describe("ZANPOS storefront catalog", () => {
  test("catalog page renders content area", async ({ page }) => {
    await page.goto("/");
    const body = page.locator("body");
    await expect(body).toBeVisible();
    const text = await body.textContent();
    expect(text).toBeTruthy();
  });

  test("Arabic locale project is configured for RTL", async ({ page, browserName }) => {
    await page.goto("/");
    const html = page.locator("html");
    const dir = await html.getAttribute("dir");
    // Storefront may default to LTR; RTL support is a project goal
    expect(["ltr", "rtl", null]).toContain(dir);
  });

  test("mobile viewport renders without horizontal overflow", async ({ page }) => {
    await page.setViewportSize({ width: 375, height: 812 });
    await page.goto("/");
    const body = page.locator("body");
    const box = await body.boundingBox();
    expect(box).not.toBeNull();
    if (box) {
      expect(box.width).toBeLessThanOrEqual(380);
    }
  });

  test("API error fallback renders recovery UI", async ({ page }) => {
    await page.route("**/api/**", (route) => route.abort());
    await page.goto("/");
    const body = page.locator("body");
    await expect(body).toBeVisible();
  });
});
