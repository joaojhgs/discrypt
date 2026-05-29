import * as React from "react";
import { cn } from "@/lib/utils";

type NativeSelectProps = Omit<
  React.SelectHTMLAttributes<HTMLSelectElement>,
  "onChange"
> & {
  onValueChange?: (value: string) => void;
};
const Select = React.forwardRef<HTMLSelectElement, NativeSelectProps>(
  ({ className, onValueChange, children, ...props }, ref) => (
    <select
      ref={ref}
      className={cn(
        "flex h-9 min-w-40 items-center rounded-xl border border-[hsl(var(--border))] bg-[hsl(var(--secondary)/0.62)] px-3 py-2 text-sm text-[hsl(var(--foreground))] outline-none transition focus:ring-2 focus:ring-[hsl(var(--ring))]",
        className,
      )}
      onChange={(event) => onValueChange?.(event.currentTarget.value)}
      {...props}
    >
      {children}
    </select>
  ),
);
Select.displayName = "Select";
const SelectItem = ({
  value,
  children,
}: {
  value: string;
  children: React.ReactNode;
}) => <option value={value}>{children}</option>;
const SelectTrigger = ({ children }: { children?: React.ReactNode }) => (
  <>{children}</>
);
const SelectContent = ({ children }: { children?: React.ReactNode }) => (
  <>{children}</>
);
const SelectValue = () => null;
const SelectGroup = ({ children }: { children?: React.ReactNode }) => (
  <>{children}</>
);
const SelectLabel = ({ children }: { children?: React.ReactNode }) => (
  <>{children}</>
);
const SelectSeparator = () => null;
const SelectScrollUpButton = () => null;
const SelectScrollDownButton = () => null;
export {
  Select,
  SelectGroup,
  SelectValue,
  SelectTrigger,
  SelectContent,
  SelectLabel,
  SelectItem,
  SelectSeparator,
  SelectScrollUpButton,
  SelectScrollDownButton,
};
