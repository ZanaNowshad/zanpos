/**
 * ZANPOS Tauri login page object.
 * Maps to the main login screen of the desktop POS application.
 */
class LoginPage {
  get root() { return browser.$("#root"); }
  get pinInput() { return browser.$('input[type="password"]'); }
  get submitBtn() { return browser.$('button[type="submit"]'); }
  get errorMsg() { return browser.$(".error-message, .auth-error"); }

  async open() {
    await this.root.waitForExist({ timeout: 15000 });
  }

  async login(pin: string) {
    await this.pinInput.setValue(pin);
    await this.submitBtn.click();
  }
}

export default new LoginPage();
