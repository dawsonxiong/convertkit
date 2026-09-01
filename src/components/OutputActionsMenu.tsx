import { useCallback, useEffect, useId, useRef, useState } from "react";
import type { KeyboardEvent as ReactKeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { useAddPaths } from "../hooks/useAddPaths";
import { OPERATIONS, type ActiveOperation } from "../lib/operations";
import { outputHandoffOperations, uniqueOutputPaths } from "../lib/outputHandoff";
import { revealPathsInFinder } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import type { Operation } from "../types";
import { nextMenuItemIndex, type MenuNavigationKey } from "../lib/menuKeyboard";

interface OutputActionsMenuProps {
  paths: readonly string[];
  sourceOperation: Operation;
  onOpenOperation: (operation: ActiveOperation) => void;
  label: string;
  disabled?: boolean;
  onUndo?: () => void | Promise<void>;
  undoLabel?: string;
  onRemove?: () => void | Promise<void>;
  removeLabel?: string;
  triggerClassName?: string;
}

interface MenuPosition {
  left: number;
  top: number;
}

const MENU_WIDTH = 180;
const MENU_MAX_HEIGHT = 280;
const MENU_GAP = 4;
const VIEWPORT_MARGIN = 8;

function enabledMenuItems(menu: HTMLElement | null): HTMLButtonElement[] {
  if (!menu) return [];
  return Array.from(menu.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)'));
}

export function OutputActionsMenu({
  paths,
  sourceOperation,
  onOpenOperation,
  label,
  disabled = false,
  onUndo,
  undoLabel = "Undo",
  onRemove,
  removeLabel = "Remove",
  triggerClassName = "",
}: OutputActionsMenuProps) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const menuRef = useRef<HTMLDivElement>(null);
  const initialFocusRef = useRef<"first" | "last">("first");
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<MenuPosition>({ left: 0, top: 0 });
  const addPaths = useAddPaths();
  const setRejection = useAppStore((state) => state.setRejection);
  const outputPaths = uniqueOutputPaths(paths);
  const destinations = outputHandoffOperations(outputPaths, sourceOperation);

  const close = useCallback((restoreFocus = false) => {
    setOpen(false);
    if (restoreFocus) triggerRef.current?.focus();
  }, []);

  const openMenu = (initialFocus: "first" | "last" = "first") => {
    const rect = triggerRef.current?.getBoundingClientRect();
    if (!rect) return;

    const opensRight = rect.left < 260;
    const preferredLeft = opensRight ? rect.right + MENU_GAP : rect.right - MENU_WIDTH;
    const left = Math.min(
      Math.max(VIEWPORT_MARGIN, preferredLeft),
      window.innerWidth - MENU_WIDTH - VIEWPORT_MARGIN,
    );
    const top = Math.min(
      Math.max(VIEWPORT_MARGIN, rect.bottom + MENU_GAP),
      window.innerHeight - MENU_MAX_HEIGHT - VIEWPORT_MARGIN,
    );

    initialFocusRef.current = initialFocus;
    setPosition({ left, top });
    setOpen(true);
  };

  useEffect(() => {
    if (!open) return;

    const items = enabledMenuItems(menuRef.current);
    items[initialFocusRef.current === "last" ? items.length - 1 : 0]?.focus();
    const handlePointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (!menuRef.current?.contains(target) && !triggerRef.current?.contains(target)) close();
    };
    const handleViewportChange = () => close(true);

    document.addEventListener("pointerdown", handlePointerDown);
    window.addEventListener("resize", handleViewportChange);
    window.addEventListener("scroll", handleViewportChange, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      window.removeEventListener("resize", handleViewportChange);
      window.removeEventListener("scroll", handleViewportChange, true);
    };
  }, [close, open]);

  const handleMenuKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      close(true);
      return;
    }
    if (event.key === "Tab") {
      close(true);
      return;
    }
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;

    const items = enabledMenuItems(menuRef.current);
    if (items.length === 0) return;
    const currentIndex = items.findIndex((item) => item === document.activeElement);
    const nextIndex =
      currentIndex < 0
        ? event.key === "ArrowUp" || event.key === "End"
          ? items.length - 1
          : 0
        : nextMenuItemIndex(event.key as MenuNavigationKey, currentIndex, items.length);
    const nextItem = items[nextIndex];
    if (!nextItem) return;

    event.preventDefault();
    nextItem.focus();
  };

  const handleTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
    event.preventDefault();
    openMenu(event.key === "ArrowUp" ? "last" : "first");
  };

  const reveal = async () => {
    close(true);
    try {
      await revealPathsInFinder(outputPaths);
    } catch {
      setRejection("That output file is no longer available");
    }
  };

  const useIn = (operation: ActiveOperation) => {
    close();
    onOpenOperation(operation);
    void addPaths(outputPaths, operation);
  };

  return (
    <>
      <button
        ref={triggerRef}
        id={`${menuId}-trigger`}
        type="button"
        onClick={() => (open ? close() : openMenu())}
        onKeyDown={handleTriggerKeyDown}
        disabled={disabled}
        aria-label={label}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? `${menuId}-menu` : undefined}
        title="Output actions"
        className={`icon-button ${triggerClassName}`.trim()}
      >
        <svg
          className="size-3.5"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.7"
          aria-hidden="true"
        >
          <path d="M8 7h8M13 4l3 3-3 3M16 17H8M11 14l-3 3 3 3" />
        </svg>
      </button>

      {open &&
        createPortal(
          <div
            ref={menuRef}
            id={`${menuId}-menu`}
            role="menu"
            aria-labelledby={`${menuId}-trigger`}
            onKeyDown={handleMenuKeyDown}
            className="output-actions-menu fixed z-50 border border-[#44464f] bg-[#18181b] py-1"
            style={{ left: position.left, top: position.top, width: MENU_WIDTH }}
          >
            {outputPaths.length > 0 && (
              <button
                type="button"
                role="menuitem"
                tabIndex={-1}
                onClick={() => void reveal()}
                className="output-menu-row"
              >
                Reveal in Finder
              </button>
            )}
            {onUndo && (
              <button
                type="button"
                role="menuitem"
                tabIndex={-1}
                onClick={() => {
                  close(true);
                  void onUndo();
                }}
                className="output-menu-row"
              >
                {undoLabel}
              </button>
            )}
            {destinations.length > 0 && (
              <div
                role="group"
                aria-label="Use output in"
                className="mt-1 border-t border-white/10 pt-0.5"
              >
                <div aria-hidden="true" className="px-3 py-0.5 text-[10px] font-medium text-white/35">
                  Use in
                </div>
                <div className="queue-scroll max-h-44 overflow-y-auto">
                  {destinations.map((operation) => (
                    <button
                      key={operation}
                      type="button"
                      role="menuitem"
                      tabIndex={-1}
                      onClick={() => useIn(operation)}
                      className="output-menu-row"
                    >
                      {OPERATIONS[operation].label}
                    </button>
                  ))}
                </div>
              </div>
            )}
            {onRemove && (
              <div className="mt-1 border-t border-white/10 pt-1">
                <button
                  type="button"
                  role="menuitem"
                  tabIndex={-1}
                  onClick={() => {
                    close();
                    void onRemove();
                  }}
                  className="output-menu-row output-menu-row-danger"
                >
                  {removeLabel}
                </button>
              </div>
            )}
          </div>,
          document.body,
        )}
    </>
  );
}
