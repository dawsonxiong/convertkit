import { useEffect, useMemo, useRef, useState } from "react";
import type { Operation } from "../types";
import { OPERATIONS } from "../lib/operations";
import { filterToolSections, type SidebarOperation } from "../lib/toolNavigation";
import appIcon from "../../src-tauri/icons/128x128.png";
import { RecentJobs } from "./RecentJobs";

interface ToolNavProps {
  operation: Operation;
  homeActive: boolean;
  activityActive: boolean;
  disabled: boolean;
  onChange: (operation: Operation) => void;
  onOpenHome: () => void;
  onOpenActivity: () => void;
}

function HomeIcon() {
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
      <path d="m4 10 8-6 8 6v10H4V10Z" />
      <path d="M9 20v-6h6v6" />
    </svg>
  );
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

function OptimizeIcon() {
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
      <path d="M8 3H3v5M16 21h5v-5M3 8l5-5M21 16l-5 5" />
      <path d="M14 5h5v5M10 19H5v-5M19 5l-5 5M5 19l5-5" />
    </svg>
  );
}

function ImageExportIcon() {
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
      <path d="M4 5h10v10H4zM10 9h10v10H10" />
      <path d="M17 3v5m0-5-2 2m2-2 2 2" />
    </svg>
  );
}

function VideoEncodeIcon() {
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
      <rect x="3" y="5" width="18" height="14" />
      <path d="m9 9 5 3-5 3V9ZM17 8v8M19 10v4" />
    </svg>
  );
}

function RemoveMetadataIcon() {
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
      <path d="M6 3h8l4 4v5M14 3v5h4M6 8v11h6" />
      <path d="M9 12h5M9 15h3M15 16l5 5M20 16l-5 5" />
    </svg>
  );
}

function ExtractAudioIcon() {
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
      <path d="M5 4h14v10H5zM9 19h6M12 14v5" />
      <path d="M9 9v1a3 3 0 0 0 6 0V9M12 7v6" />
    </svg>
  );
}

function CompressAudioIcon() {
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
      <path d="M4 12h2l1.5-4 3 8 3-10 3 6H20" />
      <path d="m6 19 3-3m-3 3v-3m12 3-3-3m3 3v-3" />
    </svg>
  );
}

function TranscribeIcon() {
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
      <path d="M6 3h8l4 4v14H6zM14 3v5h4" />
      <path d="M9 13v2M12 11v6M15 12v4" />
    </svg>
  );
}

function ExtractSubtitlesIcon() {
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
      <rect x="3" y="5" width="18" height="14" />
      <path d="M6.5 10.5h4M6.5 13.5h5M13.5 10.5h4M13.5 13.5h4" />
    </svg>
  );
}

function VideoThumbnailsIcon() {
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
      <rect x="3" y="4" width="18" height="16" />
      <path d="M9 4v16M15 4v16M3 12h18" />
    </svg>
  );
}

function RemoveAudioIcon() {
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
      <rect x="3" y="5" width="18" height="14" />
      <path d="M7 11h2l3-2v6l-3-2H7zM15 10l4 4M19 10l-4 4" />
    </svg>
  );
}

function ExtractTextIcon() {
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
      <path d="M6 3h8l4 4v6M14 3v5h4M6 8v11h6" />
      <path d="M9 12h6M9 15h3M15 16v5m0 0-2-2m2 2 2-2M19 16v5" />
    </svg>
  );
}

function RecognizeTextIcon() {
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
      <path d="M8 4H4v4M16 4h4v4M4 16v4h4M20 16v4h-4" />
      <path d="M8 9h8M12 9v7M9.5 16h5" />
    </svg>
  );
}

function PdfMergeIcon() {
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
      <path d="M6 3h8l4 4v11H6zM14 3v5h4" />
      <path d="M3 7v14h11M9 12h6M9 15h6" />
    </svg>
  );
}

function PdfSplitIcon() {
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
      <path d="M5 3h9l4 4v4M14 3v5h4M10 12v9H4v-9h6ZM14 14h6v7h-6v-7Z" />
      <path d="M12 9v3M12 16v3" />
    </svg>
  );
}

function PdfCompressIcon() {
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
      <path d="M6 3h8l4 4v14H6zM14 3v5h4" />
      <path d="m9 13 3 3 3-3M12 10v6" />
    </svg>
  );
}

function PdfImageIcon() {
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
      <path d="M6 3h8l4 4v6M14 3v5h4M6 8v11h6" />
      <rect x="12" y="13" width="9" height="8" />
      <path d="m13.5 19 2.2-2.2 1.7 1.7 1.4-1.4 1.2 1.2" />
    </svg>
  );
}

function CreateArchiveIcon() {
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
      <path d="M4 7h16v13H4zM7 4h10l2 3H5l2-3Z" />
      <path d="M12 10v7M9 13.5h6" />
    </svg>
  );
}

function ExtractArchiveIcon() {
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
      <path d="M4 7h16v13H4zM7 4h10l2 3H5l2-3Z" />
      <path d="M12 10v7m0 0-3-3m3 3 3-3" />
    </svg>
  );
}

function InspectIcon() {
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
      <circle cx="11" cy="11" r="6" />
      <path d="m16 16 4 4M11 8v6M8 11h6" />
    </svg>
  );
}

function RenameIcon() {
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
      <path d="M4 7h10M4 17h10M17 4l3 3-3 3M20 14l-3 3 3 3" />
      <path d="M7 4v6M11 14v6" />
    </svg>
  );
}

const ICONS: Record<SidebarOperation, () => React.JSX.Element> = {
  convert: ConvertIcon,
  resize: ResizeIcon,
  optimize: OptimizeIcon,
  exportImages: ImageExportIcon,
  encodeVideo: VideoEncodeIcon,
  compressAudio: CompressAudioIcon,
  removeAudio: RemoveAudioIcon,
  removeMetadata: RemoveMetadataIcon,
  extractAudio: ExtractAudioIcon,
  transcribe: TranscribeIcon,
  extractSubtitles: ExtractSubtitlesIcon,
  generateThumbnails: VideoThumbnailsIcon,
  extractText: ExtractTextIcon,
  recognizeText: RecognizeTextIcon,
  mergePdf: PdfMergeIcon,
  splitPdf: PdfSplitIcon,
  exportPdfPages: PdfImageIcon,
  compressPdf: PdfCompressIcon,
  createArchive: CreateArchiveIcon,
  extractArchive: ExtractArchiveIcon,
  rename: RenameIcon,
  inspect: InspectIcon,
};

export function OperationIcon({ operation }: { operation: SidebarOperation }) {
  const Icon = ICONS[operation];
  return <Icon />;
}

export function ToolNav({
  operation,
  homeActive,
  activityActive,
  disabled,
  onChange,
  onOpenHome,
  onOpenActivity,
}: ToolNavProps) {
  const [query, setQuery] = useState("");
  const searchInput = useRef<HTMLInputElement>(null);
  const filteredSections = useMemo(() => filterToolSections(query), [query]);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const hasResults = filteredSections.length > 0;

  useEffect(() => {
    const focusSearch = (event: KeyboardEvent) => {
      if (!(event.metaKey || event.ctrlKey) || event.key.toLocaleLowerCase() !== "k") return;
      event.preventDefault();
      searchInput.current?.focus();
      searchInput.current?.select();
    };

    window.addEventListener("keydown", focusSearch);
    return () => window.removeEventListener("keydown", focusSearch);
  }, []);

  const selectOperation = (value: Operation) => {
    setQuery("");
    onChange(value);
  };

  return (
    <aside className="sidebar-grid flex w-48 shrink-0 flex-col border-r border-[#3b3d46] px-4 pb-4">
      <div className="h-10 shrink-0" data-tauri-drag-region />

      <div className="mb-5 flex items-center gap-2 py-2">
        <img src={appIcon} alt="" className="size-8 shrink-0" />
        <p className="brand-wordmark text-xl font-semibold leading-none text-[#f2f2f4]">
          ConvertKit
        </p>
      </div>

      <div className="relative mb-4 shrink-0">
        <svg
          className="pointer-events-none absolute left-2 top-1/2 size-3.5 -translate-y-1/2 text-white/35"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.8"
          strokeLinecap="round"
          aria-hidden="true"
        >
          <circle cx="11" cy="11" r="6" />
          <path d="m16 16 4 4" />
        </svg>
        <input
          ref={searchInput}
          type="text"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={(event) => {
            if (event.key !== "Escape") return;
            setQuery("");
            event.currentTarget.blur();
          }}
          placeholder="Find a tool"
          aria-label="Find a tool"
          autoComplete="off"
          spellCheck={false}
          className="sidebar-search"
        />
      </div>

      {!normalizedQuery && (
        <button
          type="button"
          disabled={disabled}
          onClick={onOpenHome}
          aria-current={homeActive ? "page" : undefined}
          className={`sidebar-tool-button group mb-4 ${
            homeActive
              ? "bg-[#353437] text-[#b0c6ff]"
              : "text-[#a9a9b2] hover:bg-[#201f22] hover:text-[#e5e1e4]"
          }`}
        >
          <span
            className={`sidebar-tool-icon ${homeActive ? "text-[#b0c6ff]" : "text-[#8e909a] group-hover:text-[#c5c6d0]"}`}
          >
            <HomeIcon />
          </span>
          Dashboard
        </button>
      )}

      <div className="sidebar-scroll queue-scroll -mr-1 flex min-h-0 flex-1 flex-col overflow-y-auto pr-1">
        <nav className="flex shrink-0 flex-col gap-4" aria-label="File tools">
          {filteredSections.map((section) => (
            <section key={section.id} aria-labelledby={`tool-section-${section.id}`}>
              <h2 id={`tool-section-${section.id}`} className="sidebar-section-title">
                {section.label}
              </h2>

              <div className="flex flex-col gap-1.5">
                {section.operations.map((value) => {
                  const meta = OPERATIONS[value];
                  const active = !homeActive && !activityActive && value === operation;

                  return (
                    <button
                      key={value}
                      type="button"
                      disabled={disabled}
                      onClick={() => selectOperation(value)}
                      aria-current={active ? "page" : undefined}
                      className={`sidebar-tool-button group ${
                        active
                          ? "bg-[#353437] text-[#b0c6ff]"
                          : "text-[#a9a9b2] hover:bg-[#201f22] hover:text-[#e5e1e4]"
                      }`}
                    >
                      <span
                        className={`sidebar-tool-icon ${active ? "text-[#b0c6ff]" : "text-[#8e909a] group-hover:text-[#c5c6d0]"}`}
                      >
                        <OperationIcon operation={value} />
                      </span>
                      {meta.label}
                    </button>
                  );
                })}
              </div>
            </section>
          ))}

          {!hasResults && <p className="px-2 py-2 text-[11px] text-white/35">No matching tools</p>}
        </nav>

        {!normalizedQuery && (
          <RecentJobs
            disabled={disabled}
            onOpen={onChange}
            onViewAll={onOpenActivity}
            active={activityActive}
          />
        )}
      </div>
    </aside>
  );
}
