import * as React from "react";
import { cn } from "@/lib/utils";

const Tabs = ({
  className,
  children,
  ...props
}: React.HTMLAttributes<HTMLDivElement> & {
  value?: string;
  onValueChange?: (value: string) => void;
  defaultValue?: string;
}) => (
  <div className={className} {...props}>
    {children}
  </div>
);
const TabsList = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>(({ className, ...props }, ref) => (
  <div
    ref={ref}
    className={cn(
      "inline-flex h-10 items-center justify-center rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--muted)/0.5)] p-1 text-[hsl(var(--muted-foreground))]",
      className,
    )}
    {...props}
  />
));
TabsList.displayName = "TabsList";
const TabsTrigger = React.forwardRef<
  HTMLButtonElement,
  React.ButtonHTMLAttributes<HTMLButtonElement> & { value?: string }
>(({ className, type = "button", ...props }, ref) => (
  <button
    ref={ref}
    type={type}
    className={cn(
      "inline-flex items-center justify-center whitespace-nowrap rounded-lg px-3 py-1.5 text-sm font-medium transition-all hover:bg-[hsl(var(--card))] hover:text-[hsl(var(--foreground))]",
      className,
    )}
    {...props}
  />
));
TabsTrigger.displayName = "TabsTrigger";
const TabsContent = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement> & { value?: string }
>(({ className, ...props }, ref) => (
  <div ref={ref} className={cn("mt-4", className)} {...props} />
));
TabsContent.displayName = "TabsContent";
export { Tabs, TabsList, TabsTrigger, TabsContent };
