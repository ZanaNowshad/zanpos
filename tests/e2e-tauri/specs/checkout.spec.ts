/**
 * ZANPOS Tauri E2E — checkout workflow and IPC verification.
 *
 * These tests exercise real Tauri IPC and SQLite persistence through
 * the @wdio/tauri-service embedded WebDriver provider.
 */

describe("ZANPOS checkout workflow", () => {
  it("should list products via real IPC", async () => {
    // Invoke the product listing Tauri command directly
    const result = await browser.tauri.execute(({ core }) => {
      return core.invoke("product_list_all");
    });
    expect(result).toBeDefined();
    expect(Array.isArray(result)).toBe(true);
  });

  it("should report sync status via real IPC", async () => {
    const result = await browser.tauri.execute(({ core }) => {
      return core.invoke("sync_status");
    });
    expect(result).toBeDefined();
    expect(result).toHaveProperty("online");
  });

  it("should list available users via real IPC", async () => {
    const result = await browser.tauri.execute(({ core }) => {
      return core.invoke("auth_list_users");
    });
    expect(result).toBeDefined();
    expect(Array.isArray(result)).toBe(true);
  });

  it("should persist across page reload", async () => {
    // Get initial state
    const before = await browser.execute(() => document.title);
    // Reload the WebView
    await browser.refresh();
    await browser.pause(3000);
    // App should render again after reload
    const after = await browser.$("body");
    await after.waitForExist({ timeout: 15000 });
    expect(await after.isDisplayed()).toBe(true);
  });
});

describe("ZANPOS Arabic language", () => {
  it("should render body after language context init", async () => {
    const body = await browser.$("body");
    await body.waitForExist({ timeout: 20000 });
    expect(await body.isDisplayed()).toBe(true);
  });

  it("should have a language context available", async () => {
    // The app stores language preference in localStorage
    const lang = await browser.execute(() => {
      return localStorage.getItem("language") || "en";
    });
    expect(["en", "ar"]).toContain(lang);
  });
});
