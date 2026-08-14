import { describe, expect, it } from "vitest";
import { classifyScannerBurst, removeBurstSuffix } from "../zanai/scannerBurst";

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

  it("preserves ambiguous input and removes only a confirmed suffix", () => {
    expect(classifyScannerBurst([
      { key: "6", at: 0 },
      { key: " ", at: 5 },
      { key: "2", at: 10 },
      { key: "Enter", at: 15 },
    ])).toEqual({ kind: "text" });
    expect(removeBurstSuffix("please check 628001", "628001")).toBe("please check ");
    expect(removeBurstSuffix("please check 628001", "999999")).toBe("please check 628001");
  });
});
