import type { Operation } from "../types";
import { OPERATIONS } from "../lib/operations";
import appIcon from "../../src-tauri/icons/128x128.png";

interface ToolNavProps {
  operation: Operation;
  disabled: boolean;
  onChange: (operation: Operation) => void;
}

function ConvertIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M7 7h11m0 0-3-3m3 3-3 3M17 17H6m0 0 3 3m-3-3 3-3" />
    </svg>
  );
}

function ResizeIcon() {
  return (
    <svg
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.7"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <path d="M8 3H3v5M16 21h5v-5M3 8l6-6M21 16l-6 6M14 5h5v5M10 19H5v-5" />
    </svg>
  );
}

const ICONS: Record<Operation, () => React.JSX.Element> = {
  convert: ConvertIcon,
  resize: ResizeIcon,
};

export function ToolNav({ operation, disabled, onChange }: ToolNavProps) {
  return (
    <aside className="flex w-48 shrink-0 flex-col border-r border-[#3b3d46] bg-[#131315] px-4 pb-4">
      <div className="h-10 shrink-0" data-tauri-drag-region />

      <div className="mb-6 flex items-center gap-2 bg-[#e5e1e4] px-3 py-2">
        <img src={appIcon} alt="" className="size-9 shrink-0" />
        <p className="text-lg font-semibold leading-none text-[#0e0e10]">ConvertKit</p>
      </div>

      <nav className="flex flex-col gap-2.5" aria-label="File tools">
        {(Object.keys(OPERATIONS) as Operation[]).map((value) => {
          const meta = OPERATIONS[value];
          const Icon = ICONS[value];
          const active = value === operation;

          return (
            <button
              key={value}
              type="button"
              disabled={disabled}
              onClick={() => onChange(value)}
              aria-current={active ? "page" : undefined}
              className={`group flex h-7 items-center gap-1.5 px-2 text-left text-[12px] font-medium transition-colors disabled:cursor-not-allowed disabled:opacity-50 ${
                active
                  ? "bg-[#353437] text-[#b0c6ff]"
                  : "text-[#a9a9b2] hover:bg-[#201f22] hover:text-[#e5e1e4]"
              }`}
            >
              <span
                className={`size-5 ${active ? "text-[#b0c6ff]" : "text-[#8e909a] group-hover:text-[#c5c6d0]"}`}
              >
                <Icon />
              </span>
              {meta.label}
            </button>
          );
        })}
      </nav>
    </aside>
  );
}
