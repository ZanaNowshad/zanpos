/**
 * ZANPOS a11y lint config — jsx-a11y rules as errors.
 * Run with: npm run lint:a11y
 *
 * This config is separate from the main lint because the codebase has many
 * pre-existing a11y issues. Run this separately to audit without blocking CI.
 * As violations are incrementally fixed, individual rules can be moved into
 * eslint.config.js as errors under --max-warnings 0 enforcement.
 */
import jsxA11y from "eslint-plugin-jsx-a11y";
import reactHooks from "eslint-plugin-react-hooks";
import tsParser from "@typescript-eslint/parser";

/** @type {import("eslint").Linter.FlatConfig[]} */
export default [
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: {
      parser: tsParser,
      parserOptions: {
        ecmaVersion: "latest",
        sourceType: "module",
        ecmaFeatures: { jsx: true },
      },
    },
    plugins: {
      "jsx-a11y": jsxA11y,
      "react-hooks": reactHooks,
    },
    rules: {
      ...jsxA11y.configs.recommended.rules,
      // react-hooks plugin loaded only so that inline
      // // eslint-disable-next-line react-hooks/exhaustive-deps
      // comments do not cause a "rule not found" error
      "react-hooks/exhaustive-deps": "off",
    },
  },
];
