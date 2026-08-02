import * as TabsPrimitive from "@radix-ui/react-tabs";
import { type ComponentPropsWithoutRef, type ReactNode } from "react";

type TabsProps = ComponentPropsWithoutRef<typeof TabsPrimitive.Root> & {
  tabs: readonly { value: string; label: string }[];
  children: (activeTab: string) => ReactNode;
};

export function Tabs({ tabs, children, ...props }: TabsProps) {
  return (
    <TabsPrimitive.Root className="zan-tabs" {...props}>
      <TabsPrimitive.List className="zan-tabs-list">
        {tabs.map((tab) => (
          <TabsPrimitive.Trigger key={tab.value} value={tab.value} className="zan-tabs-trigger">
            {tab.label}
          </TabsPrimitive.Trigger>
        ))}
      </TabsPrimitive.List>
      {tabs.map((tab) => (
        <TabsPrimitive.Content key={tab.value} value={tab.value} className="zan-tabs-content">
          {children(tab.value)}
        </TabsPrimitive.Content>
      ))}
    </TabsPrimitive.Root>
  );
}
