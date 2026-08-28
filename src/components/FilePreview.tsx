import { useEffect, useState } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO, getCompatibleFormats } from "../lib/formats";
import { formatFileSize } from "../lib/fileUtils";
import { readFileThumbnail } from "../lib/tauri";
import type { FileInfo } from "../types";

const PREVIEWABLE = new Set(["image", "vector", "document", "video"]);

interface FilePreviewProps {
  file: FileInfo;
  canDismiss?: boolean;
}

export function FilePreview({ file, canDismiss = false }: FilePreviewProps) {
  const operation = useAppStore((s) => s.operation);
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

  return (
    <motion.div className="relative flex items-center gap-2.5 border border-[#44464f] bg-[#0e0e10] p-2">
      {/* Thumbnail or icon */}
      <div className="flex size-9 shrink-0 items-center justify-center overflow-hidden border border-[#44464f] bg-[#2a2a2c] text-base">
        {thumbnail ? <img src={thumbnail} alt="" className="w-full h-full object-cover" /> : icon}
      </div>

      {/* Info */}
      <div className="flex-1 min-w-0">
        <p className="truncate text-[13px] font-medium text-white/90">{file.name}</p>
        <p className="mt-1 text-[10px] leading-4 text-white/40">
          {formatFileSize(file.size)}
          {file.width && file.height
            ? ` · ${file.width.toLocaleString()} × ${file.height.toLocaleString()} px`
            : ""}
        </p>
      </div>

      {/* Source format */}
      <span className="shrink-0 border border-[#44464f] bg-[#201f22] px-1.5 py-1 text-[10px] font-medium text-white/55">
        {label}
      </span>

      {operation === "convert" && (
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
              className="h-7 min-w-16 appearance-none border border-[#44464f] bg-[#0e0e10] pl-2 pr-6 text-[10px] font-medium text-[#e5e1e4] outline-none focus:border-[#b0c6ff]"
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
    </motion.div>
  );
}
