import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO } from "../lib/formats";
import { formatFileSize } from "../lib/fileUtils";
import { readFileThumbnail } from "../lib/tauri";

const PREVIEWABLE = new Set(["image", "vector"]);

export function FilePreview() {
  const file = useAppStore((s) => s.file);
  const reset = useAppStore((s) => s.reset);
  const state = useAppStore((s) => s.state);
  const [thumbnail, setThumbnail] = useState<string | null>(null);

  useEffect(() => {
    if (!file) {
      setThumbnail(null);
      return;
    }
    const meta = FORMAT_INFO[file.format];
    if (!meta || !PREVIEWABLE.has(meta.category) || file.format === "heic") {
      setThumbnail(null);
      return;
    }
    readFileThumbnail(file.path)
      .then(setThumbnail)
      .catch(() => setThumbnail(null));
  }, [file]);

  if (!file) return null;

  const meta = FORMAT_INFO[file.format];
  const icon = meta?.icon ?? "📁";
  const label = meta?.label ?? file.extension.toUpperCase();
  const canDismiss = state === "loaded";

  return (
    <motion.div
      layout
      className="
        relative flex items-center gap-3 p-3
        bg-surface-elevated light:bg-surface-elevated-light
        rounded-[var(--radius-card)]
        border border-white/[0.06] light:border-black/[0.06]
      "
    >
      {/* Thumbnail or icon */}
      <div className="w-10 h-10 rounded-[var(--radius-button)] bg-white/[0.05] light:bg-black/[0.04] flex items-center justify-center text-lg shrink-0 overflow-hidden">
        {thumbnail ? (
          <img
            src={thumbnail}
            alt=""
            className="w-full h-full object-cover"
          />
        ) : (
          icon
        )}
      </div>

      {/* Info */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium text-white/90 light:text-black/90 truncate">
          {file.name}
        </p>
        <p className="text-xs text-white/40 light:text-black/40 mt-0.5">
          {formatFileSize(file.size)}
        </p>
      </div>

      {/* Format badge */}
      <span className="shrink-0 text-[10px] font-semibold uppercase tracking-wider px-2 py-0.5 rounded-[var(--radius-pill)] bg-white/[0.06] light:bg-black/[0.05] text-white/50 light:text-black/50">
        {label}
      </span>

      {/* Clear button */}
      {canDismiss && (
        <button
          type="button"
          onClick={reset}
          className="
            shrink-0 w-6 h-6 flex items-center justify-center
            rounded-[var(--radius-pill)] text-white/30 light:text-black/30
            hover:text-white/60 light:hover:text-black/60
            hover:bg-white/[0.06] light:hover:bg-black/[0.05]
          "
          aria-label="Clear file"
        >
          <svg
            className="w-3.5 h-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </button>
      )}
    </motion.div>
  );
}
