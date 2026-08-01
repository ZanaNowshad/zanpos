import * as SelectPrimitive from "@radix-ui/react-select";
import { type ComponentPropsWithoutRef } from "react";

type SelectProps<T extends string> = ComponentPropsWithoutRef<typeof SelectPrimitive.Root> & {
  options: readonly { value: T; label: string }[];
  placeholder?: string;
  value: T;
  onValueChange: (value: T) => void;
};

export function Select<T extends string>({ options, placeholder, ...props }: SelectProps<T>) {
  return (
    <SelectPrimitive.Root {...props}>
      <SelectPrimitive.Trigger className="zan-select-trigger">
        <SelectPrimitive.Value placeholder={placeholder} />
        <SelectPrimitive.Icon />
      </SelectPrimitive.Trigger>
      <SelectPrimitive.Portal>
        <SelectPrimitive.Content className="zan-select-content">
          <SelectPrimitive.ScrollUpButton />
          <SelectPrimitive.Viewport>
            {options.map((opt) => (
              <SelectPrimitive.Item key={opt.value} value={opt.value} className="zan-select-item">
                <SelectPrimitive.ItemText>{opt.label}</SelectPrimitive.ItemText>
              </SelectPrimitive.Item>
            ))}
          </SelectPrimitive.Viewport>
          <SelectPrimitive.ScrollDownButton />
        </SelectPrimitive.Content>
      </SelectPrimitive.Portal>
    </SelectPrimitive.Root>
  );
}
