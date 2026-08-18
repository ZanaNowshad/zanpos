import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AiConfirmationPolicyToggle } from "../components/settings/AiConfirmationPolicyToggle";
import { shouldAutoExecuteRun } from "../zanai/confirmationPolicy";

describe("ZanAI confirmation policy toggle", () => {
  it("explains that destructive confirmation cannot be disabled", () => {
    const html = renderToStaticMarkup(
      <AiConfirmationPolicyToggle checked={false} onChange={() => {}} />,
    );

    expect(html).toContain("Confirm non-destructive ZanAI actions");
    expect(html).toContain("always require confirmation");
    expect(html).not.toContain("checked=\"\"");
  });

  it("renders the saved enabled state", () => {
    const html = renderToStaticMarkup(
      <AiConfirmationPolicyToggle checked onChange={() => {}} />,
    );

    expect(html).toContain("checked=\"\"");
  });

  it("auto-executes safe bulk runs but pauses protected runs", () => {
    expect(shouldAutoExecuteRun(false)).toBe(true);
    expect(shouldAutoExecuteRun(true)).toBe(false);
  });
});
