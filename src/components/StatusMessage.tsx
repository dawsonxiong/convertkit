import { useCallback } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";
import { revealInFinder } from "../lib/tauri";
import { useConvert } from "../hooks/useConvert";
import { formatFileSize } from "../lib/fileUtils";

interface StatusMessageProps {
  variant: "success" | "error";
}

const ERROR_MESSAGES: Record<string, string> = {
  MissingDependency: "A required tool is not installed.",
  UnsupportedConversion: "This conversion is not supported.",
  InputNotFound: "The input file could not be found.",
  ProcessFailed: "The conversion process failed.",
  Cancelled: "Conversion was cancelled.",
  Timeout: "Conversion timed out.",
  OutputMissing: "The output file was not created.",
  DiskFull: "Not enough disk space.",
};

const INSTALL_HINTS: Record<string, string> = {
  ffmpeg: "brew install ffmpeg",
  magick: "brew install imagemagick",
  pandoc: "brew install pandoc",
  soffice: "brew install --cask libreoffice",
  resvg: "cargo install resvg",
  vtracer: "cargo install vtracer",
  tectonic: "brew install tectonic",
};

export function StatusMessage({ variant }: StatusMessageProps) {
  const result = useAppStore((s) => s.result);
  const error = useAppStore((s) => s.error);
  const file = useAppStore((s) => s.file);
  const reset = useAppStore((s) => s.reset);
  const { convert } = useConvert();

  const handleReveal = useCallback(() => {
    if (result?.output_path) {
      revealInFinder(result.output_path);
    }
  }, [result]);

  const handleCopyHint = useCallback((text: string) => {
    navigator.clipboard.writeText(text);
  }, []);

  if (variant === "success" && result) {
    const outputName = result.output_path.split("/").pop() ?? "File";
    const savedSize = formatFileSize(result.output_size);
    const duration = (result.duration_ms / 1000).toFixed(1);

    return (
      <motion.div
        initial={{ opacity: 0, scale: 0.96 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.25, ease: "easeOut" }}
        className="flex flex-col items-center gap-4 py-6"
      >
        {/* Check icon */}
        <div className="w-10 h-10 rounded-full bg-success/[0.08] flex items-center justify-center">
          <svg
            className="w-6 h-6 text-success"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M5 13l4 4L19 7" />
          </svg>
        </div>

        <div className="flex flex-col items-center gap-1 text-center">
          <p className="text-sm font-medium text-white/90 light:text-black/90">
            Conversion complete
          </p>
          <p className="text-xs text-white/40 light:text-black/40 max-w-xs truncate">
            {outputName} &middot; {savedSize} &middot; {duration}s
          </p>
        </div>

        {/* Actions */}
        <div className="flex items-center gap-2 w-full">
          <button
            type="button"
            onClick={handleReveal}
            className="
              flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
              bg-white/[0.06] light:bg-black/[0.05]
              text-white/60 light:text-black/60
              hover:bg-white/[0.10] light:hover:bg-black/[0.08]
              transition-all duration-200
            "
          >
            Reveal in Finder
          </button>
          <button
            type="button"
            onClick={reset}
            className="
              flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
              bg-accent hover:bg-accent-hover text-white
              transition-all duration-200
            "
          >
            Convert another
          </button>
        </div>
      </motion.div>
    );
  }

  if (variant === "error") {
    const kind = error?.kind ?? "ProcessFailed";
    const message = ERROR_MESSAGES[kind] ?? "Something went wrong.";
    const toolName = error?.detail?.tool;
    const installHint = toolName ? (INSTALL_HINTS[toolName] ?? error?.detail?.install_hint ?? null) : null;

    let detail = "";
    if (kind === "MissingDependency") {
      detail = error?.detail?.install_hint ?? "";
    } else if (kind === "ProcessFailed") {
      detail = error?.detail?.message ?? "";
      const stderr = error?.detail?.stderr;
      if (stderr) {
        const truncated = stderr.length > 200 ? stderr.slice(0, 200) + "..." : stderr;
        detail = detail ? `${detail}\n${truncated}` : truncated;
      }
    } else {
      detail = error?.detail?.message ?? "";
    }

    return (
      <motion.div
        initial={{ opacity: 0, scale: 0.96 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.25, ease: "easeOut" }}
        className="flex flex-col items-center gap-4 py-6"
      >
        {/* Error icon */}
        <div className="w-10 h-10 rounded-full bg-error/[0.08] flex items-center justify-center">
          <svg
            className="w-6 h-6 text-error"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        </div>

        <div className="flex flex-col items-center gap-1 text-center">
          <p className="text-sm font-medium text-white/90 light:text-black/90">{message}</p>
          {detail && <p className="text-xs text-white/40 light:text-black/40 max-w-xs whitespace-pre-wrap break-words">{detail}</p>}
        </div>

        {/* Install hint for missing dependencies */}
        {installHint && (
          <button
            type="button"
            onClick={() => handleCopyHint(installHint)}
            className="
              px-3 py-1.5 text-xs font-mono rounded-[var(--radius-pill)]
              bg-white/[0.04] light:bg-black/[0.03]
              text-white/50 light:text-black/50
              border border-white/[0.06] light:border-black/[0.06]
              hover:border-white/[0.14] light:hover:border-black/[0.14]
              transition-all duration-200
            "
            title="Click to copy"
          >
            {installHint}
            <span className="ml-2 text-white/20 light:text-black/20">copy</span>
          </button>
        )}

        {/* Actions */}
        <div className="flex items-center gap-2 w-full">
          {kind !== "Cancelled" && file && (
            <button
              type="button"
              onClick={convert}
              className="
                flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
                bg-white/[0.06] light:bg-black/[0.05]
                text-white/60 light:text-black/60
                hover:bg-white/[0.10] light:hover:bg-black/[0.08]
                transition-all duration-200
              "
            >
              Retry
            </button>
          )}
          <button
            type="button"
            onClick={reset}
            className="
              flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
              bg-accent hover:bg-accent-hover text-white
              transition-all duration-200
            "
          >
            Start over
          </button>
        </div>
      </motion.div>
    );
  }

  return null;
}
