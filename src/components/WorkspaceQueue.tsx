import { useMemo, useState } from "react";
import { ConvertButton } from "./ConvertButton";
import { FilePreview } from "./FilePreview";
import { OptimizePanel } from "./OptimizePanel";
import { ImageExportPanel } from "./ImageExportPanel";
import { OutputSettings } from "./OutputSettings";
import { PdfCompressionPanel } from "./PdfCompressionPanel";
import { PdfPageExportPanel } from "./PdfPageExportPanel";
import { PdfSplitPanel } from "./PdfSplitPanel";
import { ResizePanel } from "./ResizePanel";
import { RenamePanel } from "./RenamePanel";
import { ArchiveFormatPanel } from "./ArchiveFormatPanel";
import { ArchivePasswordPanel } from "./ArchivePasswordPanel";
import { AudioFormatPanel } from "./AudioFormatPanel";
import { VideoPresetPanel } from "./VideoPresetPanel";
import { AudioCompressionPanel } from "./AudioCompressionPanel";
import { SubtitlePanel } from "./SubtitlePanel";
import { ThumbnailPanel } from "./ThumbnailPanel";
import { TranscriptionPanel } from "./TranscriptionPanel";
import { OcrOutputPanel } from "./OcrOutputPanel";
import { useAppStore } from "../store/useAppStore";
import { useJobCapabilities } from "../hooks/useJobCapabilities";
import { buildRenamePreviews } from "../lib/rename";
import { getFileInfo, undoRename } from "../lib/tauri";
import { useRecentJobs } from "../store/useRecentJobs";
import { FORMAT_INFO, getCompatibleFormats } from "../lib/formats";
import { outputPathsForResults } from "../lib/outputHandoff";
import type { ActiveOperation } from "../lib/operations";
import { OutputActionsMenu } from "./OutputActionsMenu";

interface WorkspaceQueueProps {
  onStart: (paths?: string[]) => void;
  onCancel: () => void;
  onCancelItem: () => void;
  onOpenOperation: (operation: ActiveOperation) => void;
  interactionBlocked?: boolean;
}

export function WorkspaceQueue({
  onStart,
  onCancel,
  onCancelItem,
  onOpenOperation,
  interactionBlocked = false,
}: WorkspaceQueueProps) {
  const state = useAppStore((store) => store.state);
  const operation = useAppStore((store) => store.operation);
  const files = useAppStore((store) => store.files);
  const file = useAppStore((store) => store.file);
  const queueItems = useAppStore((store) => store.queueItems);
  const activePath = useAppStore((store) => store.activePath);
  const result = useAppStore((store) => store.result);
  const results = useAppStore((store) => store.results);
  const renameSettings = useAppStore((store) => store.renameSettings);
  const archiveFormat = useAppStore((store) => store.archiveFormat);
  const skipQueueItem = useAppStore((store) => store.skipQueueItem);
  const moveFile = useAppStore((store) => store.moveFile);
  const reset = useAppStore((store) => store.reset);
  const addFiles = useAppStore((store) => store.addFiles);
  const setRejection = useAppStore((store) => store.setRejection);
  const setCompatibleOutputFormats = useAppStore((store) => store.setCompatibleOutputFormats);
  const removeRecentByManifest = useRecentJobs((store) => store.removeByUndoManifest);
  const [undoing, setUndoing] = useState(false);
  const { unavailable, checking } = useJobCapabilities(operation);
  const isPdfMerge = operation === "mergePdf";
  const isPdfSplit = operation === "splitPdf";
  const isPdfPageExport = operation === "exportPdfPages";
  const isPdfCompress = operation === "compressPdf";
  const isCreateArchive = operation === "createArchive";
  const isExtractArchive = operation === "extractArchive";
  const isExtractAudio = operation === "extractAudio";
  const isTranscribe = operation === "transcribe";
  const isEncodeVideo = operation === "encodeVideo";
  const isCompressAudio = operation === "compressAudio";
  const isRecognizeText = operation === "recognizeText";
  const isExtractSubtitles = operation === "extractSubtitles";
  const isGenerateThumbnails = operation === "generateThumbnails";
  const isRename = operation === "rename";
  const isGroupJob = isPdfMerge || isCreateArchive || isRename;
  const renamePreviews = useMemo(
    () => (isRename ? buildRenamePreviews(files, renameSettings) : []),
    [files, isRename, renameSettings],
  );
  const renamePreviewByPath = useMemo(
    () => new Map(renamePreviews.map((preview) => [preview.inputPath, preview])),
    [renamePreviews],
  );

  const canClear = Boolean(file) && state !== "converting";
  const completedCount = files.filter(
    (item) => queueItems[item.path]?.status === "completed",
  ).length;
  const failedCount = files.filter((item) => queueItems[item.path]?.status === "failed").length;
  const skippedCount = files.filter((item) => queueItems[item.path]?.status === "skipped").length;
  const cancelledCount = files.filter(
    (item) => queueItems[item.path]?.status === "cancelled",
  ).length;
  const processedCount = completedCount + failedCount + skippedCount + cancelledCount;
  const activeStage = activePath ? queueItems[activePath]?.stage : null;
  const liveStatus = (() => {
    if (state === "empty") return "";
    if (state === "loaded") {
      return `${files.length} ${files.length === 1 ? "file" : "files"} ready`;
    }
    if (state === "converting") {
      const progressSummary = `${processedCount} of ${files.length} processed`;
      return activeStage ? `${activeStage}. ${progressSummary}` : progressSummary;
    }

    const outcomes = [
      completedCount > 0 ? `${completedCount} complete` : null,
      failedCount > 0 ? `${failedCount} failed` : null,
      skippedCount > 0 ? `${skippedCount} skipped` : null,
      cancelledCount > 0 ? `${cancelledCount} cancelled` : null,
    ].filter(Boolean);
    const outcomeSummary = outcomes.join(", ") || `${processedCount} processed`;
    return state === "error" ? `Job stopped. ${outcomeSummary}` : outcomeSummary;
  })();
  const retryablePaths = files
    .filter((item) =>
      ["failed", "skipped", "cancelled"].includes(queueItems[item.path]?.status ?? ""),
    )
    .map((item) => item.path);
  const errorRetryPaths = files
    .filter((item) => queueItems[item.path]?.status !== "completed")
    .map((item) => item.path);
  const showFooterRetry =
    (state === "done" && retryablePaths.length > 0 && (isGroupJob || retryablePaths.length > 1)) ||
    (state === "error" && errorRetryPaths.length > 0);
  const footerRetryPaths = state === "error" ? errorRetryPaths : retryablePaths;
  const bulkOutputFormats = useMemo(
    () =>
      operation === "convert" && files.length > 1
        ? Array.from(new Set(files.flatMap((item) => getCompatibleFormats(item.format)))).sort(
            (left, right) => FORMAT_INFO[left].label.localeCompare(FORMAT_INFO[right].label),
          )
        : [],
    [files, operation],
  );
  const batchOutputPaths = useMemo(() => outputPathsForResults(results), [results]);

  const handleUndoRename = async () => {
    if (!result?.undo_manifest || undoing) return;
    setUndoing(true);
    try {
      const paths = await undoRename(result.undo_manifest);
      const restored = await Promise.all(paths.map((path) => getFileInfo(path)));
      removeRecentByManifest(result.undo_manifest);
      reset();
      addFiles(restored);
      setRejection("Batch rename undone");
    } catch (error) {
      console.error("Could not undo batch rename:", error);
      setRejection("The batch rename could not be undone");
    } finally {
      setUndoing(false);
    }
  };

  return (
    <section className="workspace-queue flex min-h-0 flex-col border border-[#3b3d46] bg-[#131315]">
      <fieldset disabled={interactionBlocked} className="contents">
        <header className="flex h-11 shrink-0 items-center justify-between border-b border-[#3b3d46] bg-[#1b1b1d] px-3">
          <div className="flex min-w-0 items-center gap-2">
            <h2 className="shrink-0 text-[13px] font-semibold text-white/85">
              {isGroupJob ? "Files" : "Queue"} ({files.length})
            </h2>
            {state === "done" && processedCount > 0 && (
              <div className="flex min-w-0 items-center gap-1.5 text-[11px] font-medium">
                <span aria-hidden="true" className="text-white/20">
                  ·
                </span>
                {isGroupJob && completedCount === files.length ? (
                  <span className="whitespace-nowrap text-emerald-300">
                    {isCreateArchive
                      ? "1 archive created"
                      : isRename
                        ? `${completedCount} renamed`
                        : "1 PDF created"}
                  </span>
                ) : completedCount > 0 ? (
                  <span className="whitespace-nowrap text-emerald-300">
                    {completedCount} complete
                  </span>
                ) : null}
                {failedCount > 0 && (
                  <span className="whitespace-nowrap text-red-300">
                    {isPdfMerge
                      ? "Merge failed"
                      : isCreateArchive
                        ? "Archive failed"
                        : isRename
                          ? "Rename failed"
                          : `${failedCount} failed`}
                  </span>
                )}
                {skippedCount > 0 && (
                  <span className="whitespace-nowrap text-white/40">{skippedCount} skipped</span>
                )}
                {cancelledCount > 0 && (
                  <span className="whitespace-nowrap text-white/40">
                    {isGroupJob ? "Cancelled" : `${cancelledCount} cancelled`}
                  </span>
                )}
              </div>
            )}
          </div>
          <div className="flex min-w-0 items-center gap-2">
            {state === "loaded" && bulkOutputFormats.length > 0 && (
              <select
                value=""
                onChange={(event) => setCompatibleOutputFormats(event.target.value)}
                className="queue-bulk-select select-chevron"
                aria-label="Set output format for compatible queued files"
                title="Set every compatible queued file to this output format"
              >
                <option value="">Set output</option>
                {bulkOutputFormats.map((format) => (
                  <option key={format} value={format}>
                    {FORMAT_INFO[format].label}
                  </option>
                ))}
              </select>
            )}
            {state === "done" && results.length > 1 && batchOutputPaths.length > 0 && (
              <OutputActionsMenu
                paths={batchOutputPaths}
                sourceOperation={operation}
                onOpenOperation={onOpenOperation}
                label="Output actions for completed batch"
              />
            )}
            {canClear && (
              <button type="button" onClick={reset} className="text-button text-button-large">
                Clear
              </button>
            )}
          </div>
        </header>

        <p className="sr-only" role="status" aria-live="polite" aria-atomic="true">
          {liveStatus}
        </p>

        <div
          aria-busy={state === "converting"}
          className="queue-grid queue-scroll min-h-0 flex-1 overflow-y-auto p-3"
        >
          {state === "empty" ? (
            <div className="grid h-full min-h-40 place-items-center text-sm text-white/40">
              {isPdfMerge || isPdfSplit || isPdfPageExport || isPdfCompress
                ? "No PDFs added"
                : isExtractArchive
                  ? "No archives added"
                  : "No files added"}
            </div>
          ) : (
            <div className="flex flex-col gap-4">
              <div className="flex flex-col gap-2">
                {files.map((item, index) => (
                  <FilePreview
                    key={item.path}
                    file={item}
                    canDismiss={state === "loaded"}
                    onCancel={state === "converting" && !isGroupJob ? onCancelItem : undefined}
                    onSkip={
                      state === "converting" && !isGroupJob
                        ? () => skipQueueItem(item.path)
                        : undefined
                    }
                    onRetry={
                      (state === "done" || state === "error") && !isGroupJob
                        ? () => onStart([item.path])
                        : undefined
                    }
                    onMoveUp={isPdfMerge && index > 0 ? () => moveFile(item.path, -1) : undefined}
                    onMoveDown={
                      isPdfMerge && index < files.length - 1
                        ? () => moveFile(item.path, 1)
                        : undefined
                    }
                    showResultAction={!isGroupJob || isRename || index === 0}
                    previewName={renamePreviewByPath.get(item.path)?.outputName}
                    previewInvalid={
                      renamePreviewByPath.get(item.path)?.conflict ||
                      !renamePreviewByPath.get(item.path)?.valid
                    }
                    onOpenOperation={onOpenOperation}
                  />
                ))}
              </div>

              {state === "loaded" && operation === "resize" && <ResizePanel />}
              {state === "loaded" && operation === "optimize" && <OptimizePanel />}
              {state === "loaded" && operation === "exportImages" && <ImageExportPanel />}
              {state === "loaded" && isExtractAudio && <AudioFormatPanel />}
              {state === "loaded" && isTranscribe && <TranscriptionPanel />}
              {state === "loaded" && isEncodeVideo && <VideoPresetPanel />}
              {state === "loaded" && isCompressAudio && <AudioCompressionPanel />}
              {state === "loaded" && isRecognizeText && <OcrOutputPanel />}
              {state === "loaded" && isExtractSubtitles && <SubtitlePanel />}
              {state === "loaded" && isGenerateThumbnails && <ThumbnailPanel />}
              {state === "loaded" && isPdfSplit && <PdfSplitPanel />}
              {state === "loaded" && isPdfPageExport && <PdfPageExportPanel />}
              {state === "loaded" && isPdfCompress && <PdfCompressionPanel />}
              {state === "loaded" && isCreateArchive && <ArchiveFormatPanel />}
              {state === "loaded" && isExtractArchive && <ArchivePasswordPanel />}
              {state === "loaded" && isRename && <RenamePanel />}
              {state === "loaded" && unavailable?.message && (
                <p className="text-[11px] leading-relaxed text-amber-200/80">
                  {unavailable.message}
                </p>
              )}
              {state === "loaded" && !isRename && <OutputSettings />}

              {state === "converting" && (
                <p className="text-[10px] text-white/40">
                  {isGroupJob
                    ? isCreateArchive
                      ? archiveFormat === "gzip"
                        ? "Compressing file…"
                        : `Adding ${files.length} files…`
                      : isRename
                        ? `Renaming ${files.length} files…`
                        : `Merging ${files.length} PDFs…`
                    : `${processedCount} of ${files.length} processed`}
                </p>
              )}
            </div>
          )}
        </div>

        {(state === "loaded" ||
          state === "converting" ||
          state === "error" ||
          (state === "done" && (showFooterRetry || (isRename && result?.undo_manifest)))) && (
          <footer className="shrink-0 border-t border-[#3b3d46] bg-[#1b1b1d] p-3">
            <div className="flex gap-2">
              {showFooterRetry && (
                <button
                  type="button"
                  onClick={() => (isGroupJob ? onStart() : onStart(footerRetryPaths))}
                  className="primary-button flex-1"
                >
                  {isPdfMerge
                    ? "Retry merge"
                    : isCreateArchive
                      ? "Retry archive"
                      : isRename
                        ? "Retry rename"
                        : state === "error"
                          ? `Retry ${footerRetryPaths.length === 1 ? "file" : `${footerRetryPaths.length} files`}`
                          : `Retry all ${retryablePaths.length}`}
                </button>
              )}
              {state === "done" && isRename && result?.undo_manifest ? (
                <>
                  <button
                    type="button"
                    onClick={() => void handleUndoRename()}
                    disabled={undoing}
                    className="secondary-button flex-1"
                  >
                    {undoing ? "Undoing…" : "Undo rename"}
                  </button>
                  <button type="button" onClick={reset} className="primary-button flex-1">
                    Rename more
                  </button>
                </>
              ) : state === "error" ? (
                <button type="button" onClick={reset} className="secondary-button flex-1">
                  Reset
                </button>
              ) : state !== "done" ? (
                <div className="flex-1">
                  <ConvertButton
                    onClick={state === "converting" ? onCancel : () => onStart()}
                    mode={state === "converting" ? "cancel" : "convert"}
                    capabilityBlocked={state === "loaded" && (checking || Boolean(unavailable))}
                  />
                </div>
              ) : null}
            </div>
          </footer>
        )}
      </fieldset>
    </section>
  );
}
