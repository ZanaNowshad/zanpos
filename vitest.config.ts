import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // "node" is the default and works for all current tests (money utilities).
    // Upgrade to "jsdom" (+ install @vitest/browser or jsdom) when component
    // tests with @testing-library/react are added.
    environment: "node",

    // Glob for test files — explicit pattern avoids picking up Vitest fixtures
    include: ["src/**/*.{test,spec}.{ts,tsx}"],

    // Fail on any console.error (catches React prop-type violations etc.)
    // Use onConsoleApiCalled to report but still fail cleanly.
    // Note: set to false once suppressFalsePositives is needed.
    reporters: ["verbose"],

    // Coverage is opt-in (run `npm run coverage` separately)
    coverage: {
      provider: "v8",
      reporter: ["text", "html", "lcov"],
      include: ["src/**/*.{ts,tsx}"],
      exclude: [
        "src/main.tsx",
        "src/vite-env.d.ts",
        "src/**/*.d.ts",
        "src/__tests__/**",
      ],
      // Target: 80 / 80 / 80 (pre-flight COVERAGE_THRESHOLD = 80%).
      // Currently informational — CI enforces once test suite reaches target.
      // Expand component tests with @testing-library/react to close the gap.
      thresholds: {
        lines: 80,
        functions: 80,
        branches: 80,
      },
    },
  },
});
