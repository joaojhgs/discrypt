import * as React from "react";
const TooltipProvider = ({
  children,
}: {
  children: React.ReactNode;
  delayDuration?: number;
}) => <>{children}</>;
const Tooltip = ({ children }: { children: React.ReactNode }) => (
  <span className="group/tooltip relative inline-flex">{children}</span>
);
const TooltipTrigger = ({
  children,
}: {
  children: React.ReactNode;
  asChild?: boolean;
}) => <>{children}</>;
const TooltipContent = ({
  children,
  side = "top",
}: {
  children: React.ReactNode;
  side?: string;
}) => {
  const sideClass =
    side === "left"
      ? "right-full top-1/2 mr-2 -translate-y-1/2"
      : "bottom-full right-0 mb-2";
  return (
    <span
      role="tooltip"
      className={`pointer-events-none absolute z-50 hidden w-72 max-w-[calc(100vw-2rem)] rounded-md border border-[hsl(var(--border))] bg-[hsl(var(--popover))] px-2.5 py-1.5 text-left text-xs leading-5 text-[hsl(var(--popover-foreground))] shadow-lg group-focus-within/tooltip:block group-hover/tooltip:block ${sideClass}`}
    >
      {children}
    </span>
  );
};
export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
