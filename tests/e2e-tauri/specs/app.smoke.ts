/**
 * ZANPOS Tauri E2E smoke tests.
 *
 * The embedded WebDriver provider launches the app and connects through
 * Microsoft Edge WebDriver. ZANPOS uses a frameless window (decorations: false)
 * so browser.getTitle() may return empty.
 */

describe("ZANPOS Tauri app", () => {
  it("should launch and connect via WebDriver", async () => {
    // The session creation itself proves the app launched and WebDriver connected
    const capabilities = await browser.getWindowHandle();
    expect(capabilities).toBeTruthy();
  });

  it("should render the application shell", async () => {
    // ZANPOS renders into a WebView; the main container might not be #root
    // depending on how Vite bootstraps. Wait for any visible element.
    const body = await browser.$("body");
    await body.waitForExist({ timeout: 20000 });
    expect(await body.isDisplayed()).toBe(true);
  });
});
