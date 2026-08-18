import * as DialogPrimitive from "@radix-ui/react-dialog";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import "./drawer.css";

/**
 * Side drawer for secondary workflows.
 *
 * Purchasing kept supplier administration and draft-PO creation permanently on
 * the main canvas, so a manager reviewing orders had to read two admin forms
 * first. Those flows belong here: reachable in one click, gone when finished,
 * and they never compete with the work surface.
 *
 * Built on the Radix dialog already in use, so focus trapping, Escape, scroll
 * locking and `aria-modal` come from a maintained implementation rather than a
 * hand-rolled one.
 */
interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  title: string;
  description?: string;
  /** Pinned to the drawer foot, outside the scrolling body. */
  footer?: ReactNode;
  width?: "md" | "lg";
  children: ReactNode;
}

export default function Drawer({
  open,
  onOpenChange,
  title,
  description,
  footer,
  width = "md",
  children,
}: Props) {
  return (
    <DialogPrimitive.Root open={open} onOpenChange={onOpenChange}>
      <DialogPrimitive.Portal>
        <DialogPrimitive.Overlay className="zp-drawer-overlay" />
        <DialogPrimitive.Content className={`zp-drawer zp-drawer-${width}`}>
          <header className="zp-drawer-head">
            <div>
              <DialogPrimitive.Title asChild>
                <h2 className="zp-drawer-title">{title}</h2>
              </DialogPrimitive.Title>
              {description
                ? (
                  <DialogPrimitive.Description asChild>
                    <p className="zp-drawer-desc">{description}</p>
                  </DialogPrimitive.Description>
                )
                /* Radix warns without a description; keep it for a11y even when
                   the caller has nothing extra to say. */
                : <DialogPrimitive.Description className="zp-visually-hidden">{title}</DialogPrimitive.Description>}
            </div>
            <DialogPrimitive.Close asChild>
              <button type="button" className="zp-drawer-close" aria-label="Close">
                <X size={16} />
              </button>
            </DialogPrimitive.Close>
          </header>

          <div className="zp-drawer-body">{children}</div>

          {footer && <footer className="zp-drawer-foot">{footer}</footer>}
        </DialogPrimitive.Content>
      </DialogPrimitive.Portal>
    </DialogPrimitive.Root>
  );
}
