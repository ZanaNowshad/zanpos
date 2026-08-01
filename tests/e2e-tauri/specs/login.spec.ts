import loginPage from "../pageobjects/login.page";

describe("ZANPOS authentication", () => {
  it("should render the login screen", async () => {
    await loginPage.open();
    const root = await loginPage.root;
    expect(await root.isDisplayed()).toBe(true);
  });

  it("should show the PIN input field", async () => {
    await loginPage.open();
    const pin = await loginPage.pinInput;
    expect(await pin.isDisplayed()).toBe(true);
  });

  it("should show the submit button", async () => {
    await loginPage.open();
    const btn = await loginPage.submitBtn;
    expect(await btn.isDisplayed()).toBe(true);
  });
});
