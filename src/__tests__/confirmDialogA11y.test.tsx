import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import ConfirmDialog from "../components/templates/ConfirmDialog";
import StatusPill from "../components/templates/StatusPill";

/**
 * These lock in the accessible shape of the two components that used to attach
 * click handlers to non-interactive elements. Both were reachable by mouse only.
 */
describe("ConfirmDialog backdrop", () => {
  const props = {
    open: true,
    title: "Discard draft",
    message: "The draft will not be kept.",
    confirmLabel: "Discard",
    cancelLabel: "Keep editing",
    onConfirm: () => {},
    onCancel: () => {},
  };

  it("dismisses on backdrop click through a real button, not a div handler", () => {
    const html = renderToStaticMarkup(<ConfirmDialog {...props} />);

    expect(html).toContain('class="modal-overlay-dismiss"');
    expect(html).toContain('<button type="button" class="modal-overlay-dismiss"');
  });

  it("keeps the backdrop out of the tab order and the accessibility tree", () => {
    // The backdrop only duplicates Escape and the close button. Exposing it
    // would put a large unlabelled focus stop in front of the dialog.
    const html = renderToStaticMarkup(<ConfirmDialog {...props} />);
    const backdrop = html.slice(html.indexOf("modal-overlay-dismiss"));

    expect(backdrop).toContain('tabindex="-1"');
    expect(backdrop).toContain('aria-hidden="true"');
  });

  it("still announces itself as a modal alert dialog", () => {
    const html = renderToStaticMarkup(<ConfirmDialog {...props} />);

    expect(html).toContain('role="alertdialog"');
    expect(html).toContain('aria-modal="true"');
    expect(html).toContain('aria-labelledby="confirm-dialog-title"');
  });

  it("renders nothing when closed", () => {
    expect(renderToStaticMarkup(<ConfirmDialog {...props} open={false} />)).toBe("");
  });
});

describe("StatusPill", () => {
  it("is a real button when it does something, so Space activates it", () => {
    const html = renderToStaticMarkup(
      <StatusPill level="warning" label="11 Syncing" onClick={() => {}} />,
    );

    expect(html).toContain("<button");
    expect(html).toContain('type="button"');
    expect(html).not.toContain('role="status"');
  });

  it("stays a plain status when it does nothing, and takes no focus", () => {
    const html = renderToStaticMarkup(<StatusPill level="ok" label="Up to date" />);

    expect(html).toContain("<span");
    expect(html).toContain('role="status"');
    expect(html).not.toContain("<button");
    expect(html).not.toContain("tabindex");
  });

  it("folds the detail into the accessible name", () => {
    const html = renderToStaticMarkup(
      <StatusPill level="critical" label="WhatsApp" detail="Disconnected" />,
    );

    expect(html).toContain('aria-label="WhatsApp: Disconnected"');
  });
});
