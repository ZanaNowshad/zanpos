import { useMemo, useState } from "react";
import type { Cart, PaymentInput, SessionUser } from "../types";
import { DEVICE } from "../types";
import { buildTrainingSale } from "../utils/trainingSale";

/**
 * Practice mode: a rehearsal that never reaches the database.
 *
 * The request arrives through sessionStorage because it is set on a different
 * page before this one mounts, and it is consumed on read so a refresh does
 * not silently put the cashier back into a rehearsal they thought they left.
 *
 * Returns `null` for the sale builder when practice is off — `useCart` treats
 * that as "commit for real", so the off switch is the absence of a builder
 * rather than a flag someone could forget to check.
 */
export function usePracticeMode(sessionUser: SessionUser) {
  const [trainingMode, setTrainingMode] = useState(() => {
    try {
      const requested = sessionStorage.getItem("zanpos:start-practice") === "1";
      sessionStorage.removeItem("zanpos:start-practice");
      return requested;
    } catch {
      return false;
    }
  });

  const buildTrainingResult = useMemo(
    () => trainingMode
      ? (cart: Cart, payments: PaymentInput[]) =>
          buildTrainingSale(cart, payments, sessionUser, DEVICE.branch_name, DEVICE.currency)
      : null,
    [trainingMode, sessionUser],
  );

  return { trainingMode, setTrainingMode, buildTrainingResult };
}
