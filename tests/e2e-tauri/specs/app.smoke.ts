/**
 * ZANPOS Tauri E2E smoke tests.
 *
 * These tests require a compiled Tauri binary and run against the real app
 * through the Tauri WebDriver protocol. Not suitable for headless-only CI
 * without a Windows runner with display capabilities.
 */

describe("ZANPOS Tauri app", () => {
  it("should launch the application window", async () => {
    const title = await browser.getTitle();
    expect(title).toBeTruthy();
  });

  it("should render the root element", async () => {
    const root = await browser.$("#root");
    await root.waitForExist({ timeout: 15000 });
    expect(await root.isDisplayed()).toBe(true);
  });
});
