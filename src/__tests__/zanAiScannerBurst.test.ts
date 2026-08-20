import { describe, expect, it } from "vitest";
import { classifyScannerBurst } from "../zanai/scannerBurst";

describe("ZanAI scanner burst protection", () => {
  it("recognizes a rapid barcode terminated by Enter", () => {
    const scan = [
      { key: "6", at: 0 },
      { key: "2", at: 9 },
      { key: "8", at: 18 },
      { key: "0", at: 27 },
      { key: "0", at: 36 },
      { key: "1", at: 45 },
      { key: "Enter", at: 54 },
    ];

    expect(classifyScannerBurst(scan)).toEqual({ kind: "barcode", value: "628001" });
  });

  it("accepts scanner quantity prefixes but preserves human-paced text", () => {
    expect(classifyScannerBurst([
      { key: "3", at: 0 }, { key: "*", at: 8 }, { key: "6", at: 16 },
      { key: "2", at: 24 }, { key: "8", at: 32 }, { key: "0", at: 40 },
      { key: "0", at: 48 }, { key: "1", at: 56 }, { key: "Enter", at: 64 },
    ])).toEqual({ kind: "barcode", value: "3*628001" });
    expect(classifyScannerBurst([
      { key: "h", at: 0 },
      { key: "i", at: 180 },
      { key: "Enter", at: 500 },
    ])).toEqual({ kind: "text" });
  });

  it("treats input containing a space as text, however fast it arrives", () => {
    expect(classifyScannerBurst([
      { key: "6", at: 0 },
      { key: " ", at: 5 },
      { key: "2", at: 10 },
      { key: "Enter", at: 15 },
    ])).toEqual({ kind: "text" });
  });

  /* A scan into the composer now types like any other input — the digits stay
     in the message being written instead of being pulled into the cart, which
     is what happens when the same barcode is typed by hand. Classification
     survives for one reason only: the scanner's trailing Enter must not submit
     a half-written message. A human's Enter must still send. */
  it("separates a scanner's terminating Enter from a person pressing Enter", () => {
    const scanned = classifyScannerBurst([
      { key: "6", at: 0 }, { key: "2", at: 9 }, { key: "8", at: 18 },
      { key: "0", at: 27 }, { key: "0", at: 36 }, { key: "1", at: 45 },
      { key: "Enter", at: 54 },
    ]);
    expect(scanned.kind).toBe("barcode");

    // Same characters, typed. Nothing is swallowed and Enter sends.
    const typed = classifyScannerBurst([
      { key: "6", at: 0 }, { key: "2", at: 130 }, { key: "8", at: 265 },
      { key: "0", at: 395 }, { key: "0", at: 540 }, { key: "1", at: 700 },
      { key: "Enter", at: 900 },
    ]);
    expect(typed.kind).toBe("text");
  });
});
