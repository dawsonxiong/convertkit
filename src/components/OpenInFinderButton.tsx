import type { MouseEvent } from "react";
import { uniqueOutputPaths } from "../lib/outputHandoff";
import { revealPathsInFinder } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";

interface OpenInFinderButtonProps {
  paths: readonly string[];
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
}

export function OpenInFinderButton({
  paths,
  disabled = false,
  className = "",
  ariaLabel = "Open in Finder",
}: OpenInFinderButtonProps) {
  const setRejection = useAppStore((state) => state.setRejection);
  const outputPaths = uniqueOutputPaths(paths);
  if (outputPaths.length === 0) return null;

  const reveal = async (event: MouseEvent<HTMLButtonElement>) => {
    event.currentTarget.blur();
    try {
      await revealPathsInFinder(outputPaths);
    } catch {
      setRejection("That output file is no longer available");
    }
  };

  return (
    <button
      type="button"
      onMouseDown={(event) => event.preventDefault()}
      onClick={(event) => void reveal(event)}
      disabled={disabled}
      aria-label={ariaLabel}
      title={ariaLabel}
      className={`icon-button ${className}`.trim()}
    >
      <svg
        className="size-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        strokeLinecap="round"
        aria-hidden="true"
      >
        <circle cx="11" cy="11" r="6" />
        <path d="m16 16 4 4" />
      </svg>
    </button>
  );
}
