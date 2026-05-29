import * as React from "react";
const TooltipProvider = ({
  children,
}: {
  children: React.ReactNode;
  delayDuration?: number;
}) => <>{children}</>;
const Tooltip = ({ children }: { children: React.ReactNode }) => (
  <>{children}</>
);
const TooltipTrigger = ({
  children,
}: {
  children: React.ReactNode;
  asChild?: boolean;
}) => <>{children}</>;
const TooltipContent = ({
  children,
}: {
  children: React.ReactNode;
  side?: string;
}) => <span className="sr-only">{children}</span>;
export { Tooltip, TooltipTrigger, TooltipContent, TooltipProvider };
