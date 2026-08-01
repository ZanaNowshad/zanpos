/**
 * POS workflow page object for ZANPOS Tauri E2E tests.
 */
class PosPage {
  get root() { return browser.$("#root"); }
  get searchInput() { return browser.$('input[placeholder*="Scan"], input[placeholder*="search"], input[placeholder*="ابحث"]'); }
  get totalLabel() { return browser.$(".cart-total, [data-testid='cart-total']"); }
  get payBtn() { return browser.$('button:has-text("Pay"), button:has-text("دفع")'); }

  async open() {
    await this.root.waitForExist({ timeout: 15000 });
  }
}

export default new PosPage();

describe("ZANPOS POS workflow", () => {
  it("should render the POS screen after login", async () => {
    const root = await browser.$("#root");
    await root.waitForExist({ timeout: 20000 });
    expect(await root.isDisplayed()).toBe(true);
  });

  it("should have a search or scan input", async () => {
    const inputs = await browser.$$("input");
    expect(inputs.length).toBeGreaterThan(0);
  });
});
