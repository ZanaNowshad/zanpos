import * as DialogPrimitive from "@radix-ui/react-dialog";
import { type ComponentPropsWithoutRef, type ReactNode } from "react";

type DialogProps = ComponentPropsWithoutRef<typeof DialogPrimitive.Root> & {
  open: boolean;
  onOpenChange: (open: boolean) => void;
};

export function Dialog({ children, ...props }: DialogProps) {
  return <DialogPrimitive.Root {...props}>{children}</DialogPrimitive.Root>;
}

export function DialogTrigger({ children, ...props }: ComponentPropsWithoutRef<typeof DialogPrimitive.Trigger>) {
  return <DialogPrimitive.Trigger {...props}>{children}</DialogPrimitive.Trigger>;
}

export function DialogContent({ children, title, ...props }: ComponentPropsWithoutRef<typeof DialogPrimitive.Content> & { title?: string }) {
  return (
    <DialogPrimitive.Portal>
      <DialogPrimitive.Overlay className="zan-dialog-overlay" />
      <DialogPrimitive.Content className="zan-dialog-content" {...props}>
        {title && <DialogPrimitive.Title className="zan-dialog-title">{title}</DialogPrimitive.Title>}
        {children}
      </DialogPrimitive.Content>
    </DialogPrimitive.Portal>
  );
}

export function DialogClose({ children, ...props }: ComponentPropsWithoutRef<typeof DialogPrimitive.Close>) {
  return <DialogPrimitive.Close {...props}>{children}</DialogPrimitive.Close>;
}
