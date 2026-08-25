import { describe, expect, it } from "vitest";
import { PAYMENT_FIELD_ORDER, nextInOrder } from "../components/paymentFieldNav";

/**
 * Enter used to complete the sale from wherever the caret happened to be. On a
 * delivery the first field is the phone number, so pressing it there saved an
 * order with no address on it — a drop the rider could not deliver.
 *
 * The DOM half of this (which fields a journey renders, skipping a disabled
 * rider select, the caret actually moving) is covered end-to-end by
 * qa/payment-input-check.mjs in a real browser. What is worth pinning here is
 * the order itself and the "nowhere left to go" answer, because both are
 * decisions rather than plumbing.
 */
describe("the order the payment modal asks for things", () => {
  it("asks for the contact before the address, and the rider last", () => {
    expect([...PAYMENT_FIELD_ORDER]).toEqual([
      "#payment-customer-phone",
      "#payment-house",
      "#payment-flat",
      "#payment-road",
      "#payment-rider",
    ]);
  });

  it("advances one step at a time", () => {
    const fields = ["a", "b", "c"];
    expect(nextInOrder(fields, "a")).toBe("b");
    expect(nextInOrder(fields, "b")).toBe("c");
  });

  it("reports nothing after the last field", () => {
    // This is what turns Enter from "next" into "complete the sale". Without
    // it the cashier can never finish from the keyboard.
    expect(nextInOrder(["a", "b"], "b")).toBeNull();
  });

  it("starts at the first field when nothing holds the caret", () => {
    expect(nextInOrder(["a", "b"], null)).toBe("a");
  });

  it("starts at the first field when the caret is somewhere else entirely", () => {
    // The caret can be on the confirm button or a method radio. Treating that
    // as "not in the list" sends the cashier to the top rather than nowhere.
    expect(nextInOrder(["a", "b"], "not-a-field")).toBe("a");
  });

  it("has nowhere to go in a journey with no fields", () => {
    expect(nextInOrder([], null)).toBeNull();
  });
});

describe("where the modal opens", () => {
  /* The dialog puts the caret in the first of these on open, and Enter walks
     the same list. Both read PAYMENT_FIELD_ORDER, so they cannot disagree —
     this pins the entry point so a reorder is a deliberate act rather than a
     silent change to what a cashier types into first. */
  it("starts at the customer field, which is what a delivery asks for first", () => {
    expect(PAYMENT_FIELD_ORDER[0]).toBe("#payment-customer-phone");
  });

  it("asks for the address only after it knows who the order is for", () => {
    const customer = PAYMENT_FIELD_ORDER.indexOf("#payment-customer-phone");
    for (const later of ["#payment-house", "#payment-flat", "#payment-road", "#payment-rider"]) {
      expect(PAYMENT_FIELD_ORDER.indexOf(later as never)).toBeGreaterThan(customer);
    }
  });
});
