/**
 * The till's root class list.
 *
 * These four states drive real behaviour in App.css, not just decoration:
 * `pos-payment-started` dims and disables the product grid and barcode field so
 * a scan cannot land mid-tender, and `pos-offline` is what makes the degraded
 * state visible rather than silent. Named here so the conditions can be read
 * without unpicking a template literal in the middle of the JSX.
 */
export function posLayoutClass(state: {
  hasCart: boolean;
  paymentStarted: boolean;
  online: boolean;
}): string {
  return [
    "pos-layout",
    state.hasCart ? "pos-has-cart" : "pos-idle",
    state.paymentStarted ? "pos-payment-started" : "",
    state.online ? "pos-online" : "pos-offline",
  ].filter(Boolean).join(" ");
}
