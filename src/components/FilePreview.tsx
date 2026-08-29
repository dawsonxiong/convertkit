import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO, getCompatibleFormats } from "../lib/formats";
import { formatFileSize } from "../lib/fileUtils";
import { readFileThumbnail, revealInFinder } from "../lib/tauri";
import type { FileInfo } from "../types";

const PREVIEWABLE = new Set(["image", "vector", "document", "video"]);

interface FilePreviewProps {
  file: FileInfo;
  canDismiss?: boolean;
  onRetry?: () => void;
  onCancel?: () => void;
  onSkip?: () => void;
}

export function FilePreview({
  file,
  canDismiss = false,
  onRetry,
  onCancel,
  onSkip,
}: FilePreviewProps) {
  const operation = useAppStore((s) => s.operation);
  const appState = useAppStore((s) => s.state);
  const queueItem = useAppStore((s) => s.queueItems[file.path]);
  const outputFormat = useAppStore((s) => s.outputFormats[file.path]);
  const setOutputFormat = useAppStore((s) => s.setOutputFormat);
  const removeFile = useAppStore((s) => s.removeFile);
  const [thumbnail, setThumbnail] = useState<string | null>(null);

  useEffect(() => {
    const meta = FORMAT_INFO[file.format];
    if (!meta || !PREVIEWABLE.has(meta.category) || file.format === "heic") {
      setThumbnail(null);
      return;
    }
    readFileThumbnail(file.path)
      .then(setThumbnail)
      .catch(() => setThumbnail(null));
  }, [file]);

  const meta = FORMAT_INFO[file.format];
  const icon = meta?.icon ?? "📁";
  const label = meta?.label ?? file.extension.toUpperCase();
  const compatible = getCompatibleFormats(file.format);
  const status = queueItem?.status ?? "pending";
  const showRetry = appState === "done" && ["failed", "skipped", "cancelled"].includes(status);
  const progress = queueItem?.progress ?? 0;
  const outputSize = queueItem?.result?.output_size;
  const savedPercent =
    outputSize !== undefined && file.size > 0
      ? Math.round(Math.max(0, (1 - outputSize / file.size) * 100))
      : null;

  return (
    <motion.div className="relative flex items-center gap-2.5 border border-[#44464f] bg-[#0e0e10] p-2">
      {/* Thumbnail or icon */}
      <div className="flex size-9 shrink-0 items-center justify-center overflow-hidden border border-[#44464f] bg-[#2a2a2c] text-base">
        {thumbnail ? <img src={thumbnail} alt="" className="w-full h-full object-cover" /> : icon}
      </div>

      {/* Info */}
      <div className="flex-1 min-w-0">
        <p className="truncate text-[13px] font-medium text-white/90">{file.name}</p>
        <div className="mt-1 flex min-w-0 items-center gap-2 text-[10px] leading-4 text-white/40">
          {status === "completed" && outputSize !== undefined ? (
            <>
              <span className="whitespace-nowrap">
                {formatFileSize(file.size)} → {formatFileSize(outputSize)}
              </span>
              {savedPercent !== null && savedPercent > 0 && (
                <span className="whitespace-nowrap text-emerald-300/65">
                  {savedPercent}% smaller
                </span>
              )}
            </>
          ) : status === "running" ? (
            <span className="truncate">{queueItem?.stage || "Working"}</span>
          ) : (
            <>
              <span className="whitespace-nowrap">{formatFileSize(file.size)}</span>
              {file.width && file.height && (
                <span className="truncate">
                  {file.width.toLocaleString()} × {file.height.toLocaleString()} px
                </span>
              )}
            </>
          )}
        </div>
      </div>

      {/* Completed conversions already show their result, so the format chain is no longer needed. */}
      {(operation !== "convert" || status !== "completed") && (
        <span className="shrink-0 border border-[#44464f] bg-[#201f22] px-1.5 py-1 text-[10px] font-medium text-white/55">
          {label}
        </span>
      )}

      {operation === "convert" && status !== "completed" && (
        <>
          <svg
            className="size-3 shrink-0 text-white/35"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="m9 6 6 6-6 6" />
          </svg>
          <div className="relative shrink-0">
            <select
              value={outputFormat ?? ""}
              onChange={(event) => setOutputFormat(event.target.value, file.path)}
              aria-label={`Output format for ${file.name}`}
              disabled={appState !== "loaded"}
              className="h-7 min-w-16 appearance-none border border-[#44464f] bg-[#0e0e10] pl-2 pr-6 text-[10px] font-medium text-[#e5e1e4] outline-none focus:border-[#b0c6ff] disabled:opacity-55"
            >
              {compatible.map((format) => (
                <option key={format} value={format}>
                  {FORMAT_INFO[format]?.label ?? format.toUpperCase()}
                </option>
              ))}
            </select>
            <svg
              className="pointer-events-none absolute right-2 top-1/2 size-2.5 -translate-y-1/2 text-white/40"
              viewBox="0 0 24 24"
              fill="none"
              stroke="currentColor"
              strokeWidth="2"
              aria-hidden="true"
            >
              <path d="m7 9 5 5 5-5" />
            </svg>
          </div>
        </>
      )}

      {status === "running" && (
        <button
          type="button"
          onClick={onCancel}
          className="h-7 shrink-0 border border-[#44464f] px-2 text-[9px] text-white/55 hover:text-white"
        >
          Cancel
        </button>
      )}

      {appState === "converting" && status === "pending" && (
        <button
          type="button"
          onClick={onSkip}
          className="h-7 shrink-0 border border-[#44464f] px-2 text-[9px] text-white/45 hover:text-white"
        >
          Skip
        </button>
      )}

      {status === "completed" && queueItem?.result && (
        <button
          type="button"
          onClick={() => revealInFinder(queueItem.result?.output_path ?? "")}
          className="flex size-7 shrink-0 items-center justify-center text-emerald-300 hover:bg-white/[0.06]"
          aria-label={`Reveal output for ${file.name}`}
          title="Reveal output"
        >
          <svg
            className="size-3.5"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="2"
            aria-hidden="true"
          >
            <path d="m5 13 4 4L19 7" />
          </svg>
        </button>
      )}

      {status === "failed" && !showRetry && (
        <span
          className="shrink-0 text-[9px] font-medium text-red-300"
          title={queueItem?.error?.detail?.message ?? queueItem?.error?.kind}
        >
          Failed
        </span>
      )}

      {appState === "converting" && ["skipped", "cancelled"].includes(status) && (
        <span className="shrink-0 text-[9px] font-medium text-white/35">
          {status === "skipped" ? "Skipped" : "Cancelled"}
        </span>
      )}

      {showRetry && (
        <button
          type="button"
          onClick={onRetry}
          className="h-7 shrink-0 border border-[#44464f] px-2 text-[9px] text-white/60 hover:border-[#696b75] hover:text-white"
        >
          Retry
        </button>
      )}

      {/* Clear button */}
      {canDismiss && (
        <button
          type="button"
          onClick={() => removeFile(file.path)}
          className="flex size-7 shrink-0 items-center justify-center text-white/40 hover:bg-white/[0.06] hover:text-white"
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

      {status === "running" && (
        <div className="absolute inset-x-0 bottom-0 h-px overflow-hidden bg-white/[0.06]">
          {progress < 0 ? (
            <div className="h-full w-1/4 animate-indeterminate bg-[#b0c6ff]" />
          ) : (
            <div className="h-full bg-[#b0c6ff]" style={{ width: `${progress}%` }} />
          )}
        </div>
      )}
    </motion.div>
  );
}
