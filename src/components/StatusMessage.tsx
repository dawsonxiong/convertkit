import { motion } from "framer-motion";
import { useConvert } from "../hooks/useConvert";
import { useResize } from "../hooks/useResize";
import { formatFileSize } from "../lib/fileUtils";
import { revealInFinder } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";

interface StatusMessageProps {
  variant: "success" | "error";
}

const ERROR_MESSAGES: Record<string, string> = {
  MissingDependency: "A required tool is not installed.",
  UnsupportedConversion: "This operation is not supported.",
  InputNotFound: "The input file could not be found.",
  ProcessFailed: "The file could not be processed.",
  Cancelled: "The operation was cancelled.",
  Timeout: "The operation timed out.",
  OutputMissing: "The output file was not created.",
  DiskFull: "There is not enough disk space.",
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
  const result = useAppStore((store) => store.result);
  const results = useAppStore((store) => store.results);
  const error = useAppStore((store) => store.error);
  const file = useAppStore((store) => store.file);
  const files = useAppStore((store) => store.files);
  const operation = useAppStore((store) => store.operation);
  const reset = useAppStore((store) => store.reset);
  const { convert } = useConvert();
  const { resize } = useResize();
  const retry = operation === "resize" ? resize : convert;
  const actionName = operation === "resize" ? "Resize" : "Conversion";

  const handleReveal = () => {
    if (result?.output_path) revealInFinder(result.output_path);
  };

  if (variant === "success" && result) {
    const isBatch = results.length > 1;
    const outputName = result.output_path.split("/").pop() ?? "File";
    const totalOutputSize = results.reduce((total, item) => total + item.output_size, 0);
    const totalDuration = results.reduce((total, item) => total + item.duration_ms, 0);
    const outputSize = formatFileSize(totalOutputSize);
    const duration =
      totalDuration < 1000 ? "Under 1 sec" : `${(totalDuration / 1000).toFixed(1)} sec`;
    const inputSize = files.reduce((total, item) => total + item.size, 0);
    const difference = inputSize > 0 ? (1 - totalOutputSize / inputSize) * 100 : 0;
    const sizeComparison =
      Math.abs(difference) < 1
        ? null
        : `${Math.round(Math.abs(difference))}% ${difference > 0 ? "smaller" : "larger"}`;

    return (
      <motion.div
        initial={{ opacity: 0, scale: 0.97 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.24, ease: "easeOut" }}
        className="flex flex-col items-center py-3 text-center"
      >
        <div className="grid size-11 place-items-center rounded-full bg-emerald-500/15">
          <svg
            className="size-6 text-emerald-300"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
            aria-hidden="true"
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="m5 13 4 4L19 7" />
          </svg>
        </div>

        <p className="mt-4 text-base font-semibold text-white/90">
          {isBatch
            ? `${results.length} files ${operation === "resize" ? "resized" : "converted"}`
            : `${actionName} complete`}
        </p>
        {!isBatch && <p className="mt-1 max-w-sm truncate text-sm text-white/60">{outputName}</p>}
        <div className="mt-2 flex items-center justify-center gap-4 text-[10px] text-white/40">
          <span>{outputSize}</span>
          {sizeComparison && <span>{sizeComparison}</span>}
          <span>{duration}</span>
        </div>

        <div className="mt-5 flex w-full gap-2">
          <button type="button" onClick={handleReveal} className="secondary-button flex-1">
            Reveal in Finder
          </button>
          <button type="button" onClick={reset} className="primary-button flex-1">
            {operation === "resize" ? "Resize another" : "Convert another"}
          </button>
        </div>
      </motion.div>
    );
  }

  if (variant === "error") {
    const kind = error?.kind ?? "ProcessFailed";
    const message = ERROR_MESSAGES[kind] ?? "Something went wrong.";
    const toolName = error?.detail?.tool;
    const installHint = toolName
      ? (INSTALL_HINTS[toolName] ?? error?.detail?.install_hint ?? null)
      : null;
    const rawDetail = error?.detail?.message ?? error?.detail?.stderr ?? "";
    const detail = rawDetail.length > 220 ? `${rawDetail.slice(0, 220)}…` : rawDetail;

    return (
      <motion.div
        initial={{ opacity: 0, scale: 0.97 }}
        animate={{ opacity: 1, scale: 1 }}
        transition={{ duration: 0.24, ease: "easeOut" }}
        className="flex flex-col items-center py-3 text-center"
      >
        <div className="grid size-11 place-items-center rounded-full bg-red-500/15">
          <svg
            className="size-5 text-red-300"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
            aria-hidden="true"
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18 18 6M6 6l12 12" />
          </svg>
        </div>

        <p className="mt-4 text-base font-semibold text-white/90">{message}</p>
        {detail && (
          <p className="mt-2 max-w-sm whitespace-pre-wrap break-words text-sm leading-relaxed text-white/55">
            {detail}
          </p>
        )}

        {installHint && (
          <button
            type="button"
            onClick={() => navigator.clipboard.writeText(installHint)}
            className="mt-4 rounded-lg border border-white/[0.1] bg-[#111318] px-3 py-2 font-mono text-xs text-white/60 hover:border-white/[0.2] hover:text-white"
            title="Click to copy"
          >
            {installHint} <span className="ml-2 text-white/20">copy</span>
          </button>
        )}

        <div className="mt-5 flex w-full gap-2">
          {kind !== "Cancelled" && file && (
            <button type="button" onClick={retry} className="secondary-button flex-1">
              Retry
            </button>
          )}
          <button type="button" onClick={reset} className="primary-button flex-1">
            Start over
          </button>
        </div>
      </motion.div>
    );
  }

  return null;
}
