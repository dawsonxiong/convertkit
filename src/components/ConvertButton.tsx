import { useAppStore } from "../store/useAppStore";
import { OPERATIONS } from "../lib/operations";
import { isValidPageSelection } from "../lib/pdfPages";
import { buildRenamePreviews, renamePlanIsReady } from "../lib/rename";
import { archiveFormatLabel } from "../lib/archive";
import { videoCompressionSettingsAreReady } from "../lib/videoCompression";
import { pdfCompressionSettingsAreReady } from "../lib/pdfCompression";
import { imageOptimizationSettingsAreReady } from "../lib/imageOptimization";

interface ConvertButtonProps {
  onClick: () => void;
  mode: "convert" | "cancel";
  disabled?: boolean;
  capabilityBlocked?: boolean;
}

export function ConvertButton({
  onClick,
  mode,
  disabled = false,
  capabilityBlocked = false,
}: ConvertButtonProps) {
  const files = useAppStore((s) => s.files);
  const outputFormats = useAppStore((s) => s.outputFormats);
  const operation = useAppStore((s) => s.operation);
  const resizeWidth = useAppStore((s) => s.resizeWidth);
  const resizeHeight = useAppStore((s) => s.resizeHeight);
  const imageExportPresets = useAppStore((s) => s.imageExportPresets);
  const imageOptimizationGoal = useAppStore((s) => s.imageOptimizationGoal);
  const imageTargetSizeBytes = useAppStore((s) => s.imageTargetSizeBytes);
  const pdfSplitMode = useAppStore((s) => s.pdfSplitMode);
  const pdfPageSelection = useAppStore((s) => s.pdfPageSelection);
  const renameSettings = useAppStore((s) => s.renameSettings);
  const archiveFormat = useAppStore((s) => s.archiveFormat);
  const archivePassword = useAppStore((s) => s.archivePassword);
  const archivePasswordConfirmation = useAppStore((s) => s.archivePasswordConfirmation);
  const audioTracks = useAppStore((s) => s.audioTracks);
  const audioTrackIndexes = useAppStore((s) => s.audioTrackIndexes);
  const subtitleTracks = useAppStore((s) => s.subtitleTracks);
  const subtitleTrackIndexes = useAppStore((s) => s.subtitleTrackIndexes);
  const videoEncodingPreset = useAppStore((s) => s.videoEncodingPreset);
  const videoCompressionGoal = useAppStore((s) => s.videoCompressionGoal);
  const videoTargetSizeBytes = useAppStore((s) => s.videoTargetSizeBytes);
  const ocrOutputFormat = useAppStore((s) => s.ocrOutputFormat);
  const pdfCompressionGoal = useAppStore((s) => s.pdfCompressionGoal);
  const pdfTargetSizeBytes = useAppStore((s) => s.pdfTargetSizeBytes);
  const actionLabel = OPERATIONS[operation].actionLabel;
  const isDisabled =
    mode === "convert" &&
    (disabled ||
      capabilityBlocked ||
      files.length === 0 ||
      (operation === "extractAudio"
        ? files.some((file) => {
            const selected = audioTrackIndexes[file.path];
            return !audioTracks[file.path]?.some((track) => track.streamIndex === selected);
          })
        : operation === "extractSubtitles"
          ? files.some((file) => {
              const selected = subtitleTrackIndexes[file.path];
              return !subtitleTracks[file.path]?.some(
                (track) => track.supported && track.streamIndex === selected,
              );
            })
          : operation === "resize"
            ? !resizeWidth || !resizeHeight
            : operation === "optimize"
              ? !imageOptimizationSettingsAreReady(
                  imageOptimizationGoal,
                  imageTargetSizeBytes,
                  files,
                )
              : operation === "encodeVideo"
                ? !videoCompressionSettingsAreReady(
                    videoEncodingPreset,
                    videoCompressionGoal,
                    videoTargetSizeBytes,
                  )
                : operation === "exportImages"
                  ? imageExportPresets.length === 0
                  : operation === "compressPdf"
                    ? !pdfCompressionSettingsAreReady(pdfCompressionGoal, pdfTargetSizeBytes, files)
                    : operation === "createArchive" && archiveFormat === "sevenZ"
                      ? archivePassword !== archivePasswordConfirmation
                      : operation === "convert"
                        ? files.some((file) => !outputFormats[file.path])
                        : operation === "mergePdf"
                          ? files.length < 2
                          : (operation === "splitPdf" || operation === "exportPdfPages") &&
                              pdfSplitMode === "extract"
                            ? !isValidPageSelection(pdfPageSelection)
                            : operation === "rename"
                              ? !renamePlanIsReady(buildRenamePreviews(files, renameSettings))
                              : false));

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={isDisabled}
      className={`${mode === "cancel" ? "secondary-button" : "primary-button"} w-full`}
    >
      <span className="flex items-center gap-2">
        {mode === "cancel" && (
          <svg
            className="size-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        )}
        {mode === "cancel"
          ? "Cancel"
          : operation === "createArchive"
            ? archiveFormat === "gzip"
              ? "Compress as GZIP"
              : `Create ${archiveFormatLabel(archiveFormat)}`
            : operation === "exportImages"
              ? `${actionLabel} ${files.length * imageExportPresets.length} ${files.length * imageExportPresets.length === 1 ? "image" : "images"}`
              : operation === "extractArchive" && files.length > 1
                ? `Extract ${files.length} archives`
                : operation === "removeMetadata" && files.length > 1
                  ? `Remove metadata from ${files.length} files`
                  : operation === "removeAudio" && files.length > 1
                    ? `Remove audio from ${files.length} videos`
                    : operation === "compressAudio" && files.length > 1
                      ? `Compress ${files.length} audio files`
                      : operation === "extractAudio" && files.length > 1
                        ? `Extract audio from ${files.length} videos`
                        : operation === "extractSubtitles" && files.length > 1
                          ? `Extract subtitles from ${files.length} videos`
                          : operation === "generateThumbnails" && files.length > 1
                            ? `Generate ${files.length} images`
                            : operation === "extractText" && files.length > 1
                              ? `Extract text from ${files.length} documents`
                              : operation === "recognizeText" && ocrOutputFormat === "searchablePdf"
                                ? files.length > 1
                                  ? `Create ${files.length} searchable PDFs`
                                  : "Create searchable PDF"
                                : operation === "recognizeText" && files.length > 1
                                  ? `Recognize text in ${files.length} files`
                                  : operation === "splitPdf" && pdfSplitMode === "extract"
                                    ? files.length > 1
                                      ? `Extract pages from ${files.length} files`
                                      : "Extract pages"
                                    : operation === "exportPdfPages" && files.length > 1
                                      ? `Export pages from ${files.length} PDFs`
                                      : files.length > 1
                                        ? `${actionLabel} ${files.length} files`
                                        : actionLabel}
      </span>
    </button>
  );
}
