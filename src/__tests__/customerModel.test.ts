import { describe, expect, it } from "vitest";
import type { CustomerRow } from "../types";
import {
  directoryState,
  firstCustomerError,
  loyaltyDelta,
  loyaltyState,
  projectedBalance,
  validateCustomerField,
  validateCustomerName,
  validateLoyaltyInput,
} from "../command/pages/customers/customerModel";

/**
 * Assertions mirror `customer_add_loyalty` in
 * src-tauri/src/commands/customer_commands.rs. Where the client and backend
 * could disagree the backend wins, and the test states its rule.
 */
const customer = (points: number): CustomerRow => ({
  customer_id: "cus_1",
  branch_id: "br_1",
  name: "Fatima Al Sayed",
  phone: "+973 3600 1122",
  email: null,
  loyalty_points: points,
  created_at: new Date().toISOString(),
  notes: null,
});

describe("customer loyalty", () => {
  it("treats loyalty as one integer, with no tier", () => {
    expect(loyaltyState(customer(0))).toBe("none");
    expect(loyaltyState(customer(1))).toBe("active");
    expect(loyaltyState(customer(340))).toBe("active");
  });

  it("rejects a zero delta, matching the backend", () => {
    // Backend: `if points == 0 { return Err(Validation("must be non-zero")) }`
    expect(validateLoyaltyInput("0", "add", 100)).toBe("zero");
    expect(validateLoyaltyInput("", "add", 100)).toBe("zero");
    expect(validateLoyaltyInput("   ", "redeem", 100)).toBe("zero");
  });

  it("rejects non-integer and non-numeric input", () => {
    expect(validateLoyaltyInput("abc", "add", 100)).toBe("not-a-number");
    expect(validateLoyaltyInput("2.5", "add", 100)).toBe("not-a-number");
  });

  it("takes a positive magnitude and applies direction itself", () => {
    // A minus sign typed into an 'award' field must not silently deduct.
    expect(validateLoyaltyInput("-5", "add", 100)).toBe("negative-input");
    expect(loyaltyDelta("25", "add", 100)).toBe(25);
    expect(loyaltyDelta("25", "redeem", 100)).toBe(-25);
  });

  it("blocks over-redemption exactly where the backend does", () => {
    // Backend: `if points < 0 && before + points < 0 { Err(Insufficient) }`
    expect(validateLoyaltyInput("100", "redeem", 100)).toBeNull();  // to exactly zero is fine
    expect(validateLoyaltyInput("101", "redeem", 100)).toBe("insufficient");
    expect(validateLoyaltyInput("1", "redeem", 0)).toBe("insufficient");
    expect(loyaltyDelta("101", "redeem", 100)).toBeNull();
  });

  it("never sends a delta the backend would reject", () => {
    for (const bad of ["0", "", "abc", "-3", "999"]) {
      expect(loyaltyDelta(bad, "redeem", 10)).toBeNull();
    }
  });

  it("projects the resulting balance for the confirmation", () => {
    expect(projectedBalance("40", "add", 100)).toBe(140);
    expect(projectedBalance("40", "redeem", 100)).toBe(60);
    expect(projectedBalance("400", "redeem", 100)).toBeNull();
  });
});

describe("customer directory", () => {
  it("distinguishes a new store from a search that matched nothing", () => {
    expect(directoryState([], false, null)).toBe("first-use");
    expect(directoryState([], true, null)).toBe("no-results");
    expect(directoryState([customer(0)], true, null)).toBe("ready");
  });

  it("treats a load failure as degraded, not as absence of data", () => {
    expect(directoryState([], false, "db locked")).toBe("degraded");
    expect(directoryState([], true, "db locked")).toBe("degraded");
  });

  it("requires only a name, matching customer_create", () => {
    expect(validateCustomerName("Ahmed")).toBe(true);
    expect(validateCustomerName("  ")).toBe(false);
    expect(validateCustomerName("")).toBe(false);
  });
});

describe("customer field validation", () => {
  it("accepts names up to the backend's limit, not the retired schema's", () => {
    // The old zod schema capped names at 200; customer_create allows 255.
    expect(validateCustomerField("name", "a".repeat(255))).toBeNull();
    expect(validateCustomerField("name", "a".repeat(256))).toBe("name-too-long");
  });

  it("applies the backend's phone charset exactly", () => {
    // Backend: c.is_ascii_digit() || " +-()".contains(c)
    expect(validateCustomerField("phone", "+973 (17) 555-0100")).toBeNull();
    expect(validateCustomerField("phone", "17555100")).toBeNull();
    expect(validateCustomerField("phone", "ext. 12")).toBe("phone-charset");
    expect(validateCustomerField("phone", "٩٧٣")).toBe("phone-charset");
    expect(validateCustomerField("phone", "1".repeat(31))).toBe("phone-too-long");
  });

  it("treats blank optional fields as absent, not as errors", () => {
    expect(validateCustomerField("phone", "")).toBeNull();
    expect(validateCustomerField("phone", "   ")).toBeNull();
    expect(validateCustomerField("email", "")).toBeNull();
  });

  it("checks email format, which the backend never does", () => {
    expect(validateCustomerField("email", "fatima@amwaj.bh")).toBeNull();
    expect(validateCustomerField("email", "fatima@amwaj")).toBe("email-invalid");
    expect(validateCustomerField("email", "not an address")).toBe("email-invalid");
    expect(validateCustomerField("email", "a@b.c d")).toBe("email-invalid");
  });

  it("reports the first blocking error in field order", () => {
    expect(firstCustomerError({ name: "", phone: "abc", email: "bad" }))
      .toEqual({ field: "name", error: "name-required" });
    expect(firstCustomerError({ name: "Ahmed", phone: "abc", email: "bad" }))
      .toEqual({ field: "phone", error: "phone-charset" });
    expect(firstCustomerError({ name: "Ahmed", phone: "", email: "bad" }))
      .toEqual({ field: "email", error: "email-invalid" });
    expect(firstCustomerError({ name: "Ahmed", phone: "+973 3600 1122", email: "" }))
      .toBeNull();
  });
});
