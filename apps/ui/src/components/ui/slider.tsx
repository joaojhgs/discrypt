import * as React from "react";
import { cn } from "@/lib/utils";

type SliderProps = Omit<
  React.InputHTMLAttributes<HTMLInputElement>,
  "value" | "defaultValue" | "onChange" | "type"
> & {
  value?: number[];
  defaultValue?: number[];
  onValueChange?: (value: number[]) => void;
};

const Slider = React.forwardRef<HTMLInputElement, SliderProps>(
  (
    {
      className,
      value,
      defaultValue,
      min = 0,
      max = 100,
      step = 1,
      onValueChange,
      ...props
    },
    ref,
  ) => (
    <input
      ref={ref}
      type="range"
      min={min}
      max={max}
      step={step}
      value={value?.[0]}
      defaultValue={defaultValue?.[0]}
      onChange={(event) => onValueChange?.([Number(event.currentTarget.value)])}
      className={cn(
        "h-2 w-full cursor-pointer accent-[hsl(var(--primary))]",
        className,
      )}
      {...props}
    />
  ),
);
Slider.displayName = "Slider";
export { Slider };
