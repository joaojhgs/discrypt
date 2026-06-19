import * as React from "react";
import { createPortal } from "react-dom";
import { cn } from "@/lib/utils";

const focusableSelector = [
  "a[href]",
  "button:not([disabled])",
  "textarea:not([disabled])",
  "input:not([disabled])",
  "select:not([disabled])",
  "[tabindex]:not([tabindex='-1'])",
].join(",");

type DialogContentProps = React.HTMLAttributes<HTMLDivElement> & {
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
};

function getFocusableElements(container: HTMLElement) {
  return Array.from(container.querySelectorAll<HTMLElement>(focusableSelector))
    .filter(
      (element) =>
        !element.hasAttribute("disabled") &&
        element.getAttribute("aria-hidden") !== "true",
    );
}

const Dialog = ({ children }: { children: React.ReactNode }) => <>{children}</>;
const DialogPortal = ({ children }: { children: React.ReactNode }) => (
  typeof document === "undefined" ? (
    <>{children}</>
  ) : (
    createPortal(children, document.body)
  )
);
const DialogOverlay = React.forwardRef<
  HTMLDivElement,
  React.HTMLAttributes<HTMLDivElement>
>((props, ref) => <div ref={ref} {...props} />);
DialogOverlay.displayName = "DialogOverlay";
const DialogTrigger = ({
  children,
}: {
  children: React.ReactNode;
  asChild?: boolean;
}) => <>{children}</>;
const DialogClose = ({
  children,
}: {
  children: React.ReactNode;
  asChild?: boolean;
}) => <>{children}</>;
const DialogContent = React.forwardRef<
  HTMLDivElement,
  DialogContentProps
>(
  (
    { className, onKeyDown, onEscapeKeyDown, role = "dialog", ...props },
    ref,
  ) => {
    const contentRef = React.useRef<HTMLDivElement | null>(null);

    React.useImperativeHandle(ref, () => contentRef.current as HTMLDivElement);

    React.useEffect(() => {
      const content = contentRef.current;
      const previousFocus =
        document.activeElement instanceof HTMLElement
          ? document.activeElement
          : null;
      const previousOverflow = document.body.style.overflow;
      document.body.style.overflow = "hidden";

      window.requestAnimationFrame(() => {
        const preferredFocus =
          content?.querySelector<HTMLElement>("[data-autofocus='true']") ??
          (content ? getFocusableElements(content)[0] : null) ??
          content;
        preferredFocus?.focus({ preventScroll: true });
      });

      return () => {
        document.body.style.overflow = previousOverflow;
        if (previousFocus && document.contains(previousFocus)) {
          previousFocus.focus({ preventScroll: true });
        }
      };
    }, []);

    React.useEffect(() => {
      const onDocumentKeyDown = (event: KeyboardEvent) => {
        const content = contentRef.current;
        if (!content) return;

        if (event.key === "Escape") {
          event.preventDefault();
          onEscapeKeyDown?.(event);
          return;
        }

        if (event.key !== "Tab") return;
        const focusableElements = getFocusableElements(content);
        if (!focusableElements.length) {
          event.preventDefault();
          content.focus({ preventScroll: true });
          return;
        }

        const firstElement = focusableElements[0];
        const lastElement = focusableElements[focusableElements.length - 1];
        const activeElement = document.activeElement;

        if (
          event.shiftKey &&
          (activeElement === firstElement || activeElement === content)
        ) {
          event.preventDefault();
          lastElement.focus({ preventScroll: true });
        } else if (!event.shiftKey && activeElement === lastElement) {
          event.preventDefault();
          firstElement.focus({ preventScroll: true });
        }
      };

      document.addEventListener("keydown", onDocumentKeyDown);
      return () => document.removeEventListener("keydown", onDocumentKeyDown);
    }, [onEscapeKeyDown]);

    return (
      <div
        ref={contentRef}
        {...props}
        role={role}
        aria-modal={props["aria-modal"] ?? true}
        tabIndex={-1}
        className={cn(
          "rounded-2xl border border-[hsl(var(--border))] bg-[hsl(var(--popover))] p-6 outline-none",
          className,
        )}
        onKeyDown={onKeyDown}
      />
    );
  },
);
DialogContent.displayName = "DialogContent";
const DialogHeader = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn("flex flex-col space-y-1.5 text-left", className)}
    {...props}
  />
);
const DialogFooter = ({
  className,
  ...props
}: React.HTMLAttributes<HTMLDivElement>) => (
  <div
    className={cn(
      "flex flex-col-reverse gap-2 sm:flex-row sm:justify-end",
      className,
    )}
    {...props}
  />
);
const DialogTitle = React.forwardRef<
  HTMLHeadingElement,
  React.HTMLAttributes<HTMLHeadingElement>
>(({ className, ...props }, ref) => (
  <h2
    ref={ref}
    className={cn(
      "text-lg font-semibold leading-none tracking-tight",
      className,
    )}
    {...props}
  />
));
DialogTitle.displayName = "DialogTitle";
const DialogDescription = React.forwardRef<
  HTMLParagraphElement,
  React.HTMLAttributes<HTMLParagraphElement>
>(({ className, ...props }, ref) => (
  <p
    ref={ref}
    className={cn(
      "text-sm leading-6 text-[hsl(var(--muted-foreground))]",
      className,
    )}
    {...props}
  />
));
DialogDescription.displayName = "DialogDescription";
export {
  Dialog,
  DialogPortal,
  DialogOverlay,
  DialogTrigger,
  DialogClose,
  DialogContent,
  DialogHeader,
  DialogFooter,
  DialogTitle,
  DialogDescription,
};
