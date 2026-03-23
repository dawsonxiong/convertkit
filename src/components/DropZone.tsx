import { useCallback } from "react";
import { motion } from "framer-motion";
import { open } from "@tauri-apps/plugin-dialog";
import { getFileInfo } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import { isSupportedFile } from "../lib/formats";

interface DropZoneProps {
  isDragging: boolean;
}

export function DropZone({ isDragging }: DropZoneProps) {
  const setFile = useAppStore((s) => s.setFile);
  const setRejection = useAppStore((s) => s.setRejection);

  const handleClick = useCallback(async () => {
    const selected = await open({
      multiple: false,
      title: "Choose a file to convert",
    });

    if (selected) {
      if (!isSupportedFile(selected)) {
        const ext = selected.split(".").pop()?.toLowerCase() ?? "unknown";
        setRejection(`".${ext}" files are not supported`);
        return;
      }
      try {
        const info = await getFileInfo(selected);
        setFile(info);
      } catch (err) {
        console.error("Failed to get file info:", err);
      }
    }
  }, [setFile, setRejection]);

  return (
    <motion.button
      type="button"
      onClick={handleClick}
      whileHover={{ scale: 1.01 }}
      whileTap={{ scale: 0.99 }}
      className={`
        w-full aspect-[4/3] rounded-[var(--radius-card)]
        flex flex-col items-center justify-center gap-3
        border border-dashed cursor-pointer
        transition-all duration-200
        ${
          isDragging
            ? "border-accent bg-accent/[0.06] scale-[1.02] shadow-[0_0_24px_rgba(37,99,235,0.12)]"
            : "border-white/[0.06] light:border-black/[0.06] hover:border-white/[0.10] light:hover:border-black/[0.10]"
        }
      `}
    >
      {/* Upload icon */}
      <svg
        className={`w-8 h-8 transition-colors duration-200 ${
          isDragging ? "text-accent" : "text-white/15 light:text-black/15"
        }`}
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={1}
      >
        <path
          strokeLinecap="round"
          strokeLinejoin="round"
          d="M3 16.5v2.25A2.25 2.25 0 005.25 21h13.5A2.25 2.25 0 0021 18.75V16.5m-13.5-9L12 3m0 0l4.5 4.5M12 3v13.5"
        />
      </svg>

      <div className="flex flex-col items-center gap-1">
        <span
          className={`text-sm font-medium transition-colors duration-200 ${
            isDragging ? "text-accent" : "text-white/50 light:text-black/50"
          }`}
        >
          {isDragging ? "Drop to convert" : "Drop file here"}
        </span>
        <span className="text-xs text-white/25 light:text-black/25">or click to browse</span>
      </div>
    </motion.button>
  );
}
