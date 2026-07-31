import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";

Object.defineProperty(globalThis, "crypto", {
  value: globalThis.crypto,
  configurable: true,
});

afterEach(() => cleanup());
