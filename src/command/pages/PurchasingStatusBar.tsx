import { Check, Circle, Clock, Truck, AlertTriangle } from "lucide-react";
import type { ReactNode } from "react";

export type POStep =
  | "supplier"
  | "draft"
  | "approval"
  | "ordered"
  | "partial"
  | "received"
  | "variance";

interface StepDef {
  id: POStep;
  label: string;
  icon: ReactNode;
}

const STEPS: StepDef[] = [
  { id: "supplier", label: "Supplier", icon: <Circle size={12} /> },
  { id: "draft", label: "Draft PO", icon: <Clock size={12} /> },
  { id: "approval", label: "Approval", icon: <AlertTriangle size={12} /> },
  { id: "ordered", label: "Ordered", icon: <Truck size={12} /> },
  { id: "partial", label: "Partially Received", icon: <Truck size={12} /> },
  { id: "received", label: "Received", icon: <Check size={12} /> },
  { id: "variance", label: "Variance Resolved", icon: <Check size={12} /> },
];

interface Props {
  /** The current active step. Steps before this are shown as completed, after as pending. */
  currentStep: POStep;
  /** Optional overrides for step labels. */
  stepLabels?: Partial<Record<POStep, string>>;
}

export default function PurchasingStatusBar({ currentStep, stepLabels }: Props) {
  const currentIdx = STEPS.findIndex(s => s.id === currentStep);

  return (
    <nav className="purchasing-status-bar" aria-label="Purchase order workflow">
      {STEPS.map((step, i) => {
        const isComplete = i < currentIdx;
        const isCurrent = i === currentIdx;
        return (
          <div
            key={step.id}
            className={`purchasing-step ${isComplete ? "purchasing-step-done" : ""} ${isCurrent ? "purchasing-step-active" : ""}`}
            aria-current={isCurrent ? "step" : undefined}
          >
            <span className="purchasing-step-icon">
              {isComplete ? <Check size={12} /> : step.icon}
            </span>
            <span className="purchasing-step-label">
              {stepLabels?.[step.id] ?? step.label}
            </span>
          </div>
        );
      })}
    </nav>
  );
}
