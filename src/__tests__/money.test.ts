import { describe, it, expect } from "vitest";
import { formatMoney, parseMoney } from "../money";

// ── formatMoney ────────────────────────────────────────────────────────────────

describe("formatMoney", () => {
  // BHD — 3 decimal places (default)
  it("formats zero correctly (BHD)", () => {
    expect(formatMoney(0)).toBe("0.000");
  });

  it("formats exact major unit (BHD)", () => {
    expect(formatMoney(1000)).toBe("1.000");
    expect(formatMoney(2000)).toBe("2.000");
  });

  it("formats sub-unit amounts (BHD)", () => {
    expect(formatMoney(1)).toBe("0.001");
    expect(formatMoney(10)).toBe("0.010");
    expect(formatMoney(100)).toBe("0.100");
    expect(formatMoney(500)).toBe("0.500");
    expect(formatMoney(1500)).toBe("1.500");
  });

  it("formats large amounts (BHD)", () => {
    expect(formatMoney(1234567)).toBe("1234.567");
    expect(formatMoney(100000)).toBe("100.000");
  });

  it("formats negative amounts (BHD)", () => {
    expect(formatMoney(-1500)).toBe("-1.500");
    expect(formatMoney(-100)).toBe("-0.100");
    expect(formatMoney(-1)).toBe("-0.001");
  });

  // USD — 2 decimal places
  it("formats zero correctly (exponent=2)", () => {
    expect(formatMoney(0, 2)).toBe("0.00");
  });

  it("formats cents correctly (exponent=2)", () => {
    expect(formatMoney(199, 2)).toBe("1.99");
    expect(formatMoney(100, 2)).toBe("1.00");
    expect(formatMoney(5, 2)).toBe("0.05");
  });

  it("formats negative cents (exponent=2)", () => {
    expect(formatMoney(-199, 2)).toBe("-1.99");
  });

  // Whole numbers — exponent=0 (frac is 0 mod 1 = 0, padStart(0) keeps "0")
  it("formats exponent=0 correctly", () => {
    expect(formatMoney(42, 0)).toBe("42.0");
    expect(formatMoney(0, 0)).toBe("0.0");
  });
});

// ── parseMoney ─────────────────────────────────────────────────────────────────

describe("parseMoney", () => {
  // BHD — 3 decimal places (default)
  it("parses integer string (BHD)", () => {
    expect(parseMoney("1")).toBe(1000);
    expect(parseMoney("2")).toBe(2000);
  });

  it("parses decimal string (BHD)", () => {
    expect(parseMoney("1.500")).toBe(1500);
    expect(parseMoney("0.001")).toBe(1);
    expect(parseMoney("0.010")).toBe(10);
    expect(parseMoney("0.100")).toBe(100);
    expect(parseMoney("1.000")).toBe(1000);
  });

  it("parses zero (BHD)", () => {
    expect(parseMoney("0")).toBe(0);
    expect(parseMoney("0.000")).toBe(0);
  });

  it("parses large decimal string (BHD)", () => {
    expect(parseMoney("1234.567")).toBe(1234567);
  });

  it("parses negative (BHD)", () => {
    expect(parseMoney("-1.500")).toBe(-1500);
  });

  it("returns 0 for invalid input", () => {
    expect(parseMoney("")).toBe(0);
    expect(parseMoney("abc")).toBe(0);
    expect(parseMoney("NaN")).toBe(0);
  });

  // USD — 2 decimal places
  it("parses decimal string (exponent=2)", () => {
    expect(parseMoney("1.99", 2)).toBe(199);
    expect(parseMoney("0.05", 2)).toBe(5);
    expect(parseMoney("10.00", 2)).toBe(1000);
  });

  // Round-trip invariant
  it("round-trips formatMoney → parseMoney (BHD)", () => {
    const samples = [0, 1, 10, 100, 500, 999, 1000, 1500, 99999, 1234567];
    for (const minor of samples) {
      expect(parseMoney(formatMoney(minor))).toBe(minor);
    }
  });

  it("round-trips parseMoney → formatMoney (BHD)", () => {
    const strings = ["0.000", "0.001", "0.500", "1.000", "1.500", "1234.567"];
    for (const s of strings) {
      expect(formatMoney(parseMoney(s))).toBe(s);
    }
  });
});
