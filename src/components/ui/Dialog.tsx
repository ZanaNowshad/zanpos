import * as DialogPrimitive from "@radix-ui/react-dialog";
import { type ComponentPropsWithoutRef } from "react";

type DialogProps = ComponentPropsWithoutRef<typeof DialogPrimitive.Root>;

export function Dialog({ children, ...props }: DialogProps) {
  return <DialogPrimitive.Root {...props}>{children}</DialogPrimitive.Root>;
}

export function DialogTrigger({ children, ...props }: ComponentPropsWithoutRef<typeof DialogPrimitive.Trigger>) {
  return <DialogPrimitive.Trigger {...props}>{children}</DialogPrimitive.Trigger>;
}

export function DialogContent({
  children,
  title,
  ...props
}: ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & { title?: string }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="modal-overlay" />
      <DialogPrimitive.Content
        className="modal confirm-action-modal"
        {...props}
      >
        {title && (
          <DialogPrimitive.Title asChild>
            <h2 id="app-confirm-title">{title}</h2>
          </DialogPrimitive.Title>
        )}
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export function DialogClose({ children, ...props }: ComponentPropsWithoutRef<typeof DialogPrimitive.Close>) {
  return <DialogPrimitive.Close {...props}>{children}</DialogPrimitive.Close>;
}
