import { useEffect } from "react";
import { createPortal } from "react-dom";
import { X } from "lucide-react";
import type { ReactNode } from "react";
import { cn } from "../utils";

const IS_MACOS = navigator.userAgent.includes("Mac");

interface DetailSheetProps {
  open: boolean;
  title: ReactNode;
  description?: ReactNode;
  meta?: ReactNode;
  onClose: () => void;
  children: ReactNode;
  /**
   * `overlay` covers the content region on top of a backdrop (modal flows).
   * `docked` renders in place as a fixed-width right column, so the sidebar and
   * the Board stay visible and operable.
   */
  variant?: "overlay" | "docked";
}

export function DetailSheet({
  open,
  title,
  description,
  meta,
  onClose,
  children,
  variant = "overlay",
}: DetailSheetProps) {
  useEffect(() => {
    // Escape belongs to the docked Inspector contract; the overlay keeps its
    // existing backdrop-click behavior.
    if (!open || variant !== "docked") return;
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [open, onClose, variant]);

  if (!open) return null;

  const body = (
    <>
      <button
        onClick={onClose}
        className="absolute top-4 right-5 z-10 shrink-0 rounded-md p-1.5 text-muted transition-colors outline-none hover:bg-surface-hover hover:text-secondary"
      >
        <X className="h-4 w-4" />
      </button>
      <div
        className={cn(
          "min-h-0 flex-1 overflow-y-auto scrollbar-hide",
          variant === "docked" ? "px-4 pt-4 pb-5" : "px-6 pt-5 pb-6",
        )}
      >
        <h2
          className={cn(
            "mb-3 min-w-0 pr-10 font-semibold leading-tight tracking-tight text-primary",
            variant === "docked" ? "text-[18px]" : "text-[28px]",
          )}
        >
          <span className="block">{title}</span>
        </h2>
        {description ? (
          <div
            className={cn(
              "text-secondary",
              variant === "docked" ? "text-[13px] leading-6" : "text-[15px] leading-7",
            )}
          >
            {description}
          </div>
        ) : null}
        {meta ? <div className="mt-4">{meta}</div> : null}
        <div className="mt-5">{children}</div>
      </div>
    </>
  );

  if (variant === "docked") {
    return (
      <aside className="flex h-full max-h-full w-[340px] shrink-0 flex-col overflow-hidden rounded-xl border border-border-subtle bg-bg-secondary">
        {body}
      </aside>
    );
  }

  return createPortal(
    <div className="fixed top-[28px] right-0 bottom-0 left-[220px] z-40 isolate">
      <div
        className={
          IS_MACOS
            ? "absolute inset-0 z-0 bg-black/65"
            : "absolute inset-0 z-0 bg-black/60 backdrop-blur-sm"
        }
        onClick={onClose}
      />
      <div className="absolute inset-0 z-10 flex min-h-0 flex-col overflow-hidden border-l border-border-subtle bg-bg-secondary">
        {body}
      </div>
    </div>,
    document.body
  );
}
