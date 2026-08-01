import * as TooltipPrimitive from "@radix-ui/react-tooltip";
import { type ComponentPropsWithoutRef } from "react";

export function TooltipProvider({ children, ...props }: ComponentPropsWithoutRef<typeof TooltipPrimitive.Provider>) {
  return <TooltipPrimitive.Provider {...props}>{children}</TooltipPrimitive.Provider>;
}

export function Tooltip({ children, content, ...props }: ComponentPropsWithoutRef<typeof TooltipPrimitive.Root> & { content: string }) {
  return (
    <TooltipPrimitive.Root {...props}>
      <TooltipPrimitive.Trigger asChild>{children}</TooltipPrimitive.Trigger>
      <TooltipPrimitive.Portal>
        <TooltipPrimitive.Content className="zan-tooltip-content" sideOffset={4}>
          {content}
          <TooltipPrimitive.Arrow className="zan-tooltip-arrow" />
        </TooltipPrimitive.Content>
      </TooltipPrimitive.Portal>
    </TooltipPrimitive.Root>
  );
}
