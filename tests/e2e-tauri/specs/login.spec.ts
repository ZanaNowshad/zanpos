import loginPage from "../pageobjects/login.page";

describe("ZANPOS authentication", () => {
  it("should render application after launch", async () => {
    const body = await browser.$("body");
    await body.waitForExist({ timeout: 20000 });
    expect(await body.isDisplayed()).toBe(true);
  });
});
