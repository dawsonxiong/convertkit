import type { FinderQuickActionStatus } from "../lib/tauri";

interface FinderQuickActionButtonProps {
  name: string;
  status?: FinderQuickActionStatus | null;
  disabled?: boolean;
  onClick: () => void;
}

export function FinderQuickActionButton({
  name,
  status,
  disabled = false,
  onClick,
}: FinderQuickActionButtonProps) {
  const installed = status?.installed ?? false;

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={disabled || status?.supported === false}
      className={`icon-button ${installed ? "text-[#b0c6ff]" : ""}`.trim()}
      aria-label={`${installed ? "Remove" : "Add"} ${name} ${installed ? "from" : "to"} Finder Quick Actions`}
      aria-pressed={installed}
      title={installed ? "Remove from Finder Quick Actions" : "Add to Finder Quick Actions"}
    >
      <svg
        className="size-3.5"
        viewBox="0 0 24 24"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.7"
        aria-hidden="true"
      >
        <path d="M13 2 5 13h6l-1 9 9-12h-6V2Z" />
      </svg>
    </button>
  );
}
