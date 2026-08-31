import { useEffect, useState } from "react";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO, getCompatibleFormats } from "../lib/formats";
import { formatFileSize } from "../lib/fileUtils";
import { getPageSelectionCount } from "../lib/pdfPages";
import { readFileThumbnail } from "../lib/tauri";
import { archiveFormatLabel, archivePathLabel } from "../lib/archive";
import { CONVERSION_ERROR_MESSAGES } from "../lib/conversionErrorMessages";
import { outputPathsForResult } from "../lib/outputHandoff";
import type { ActiveOperation } from "../lib/operations";
import type { AudioTrack, FileInfo } from "../types";
import { OutputActionsMenu } from "./OutputActionsMenu";

const PREVIEWABLE = new Set(["image", "vector", "document", "video"]);

function subtitleTrackLabel(ordinal: number, language: string | null, title: string | null) {
  const identity = title || (language ? language.toUpperCase() : `Track ${ordinal + 1}`);
  return identity.length > 24 ? `${identity.slice(0, 23)}…` : identity;
}

function subtitleCodecLabel(codec: string) {
  const labels: Record<string, string> = {
    ass: "ASS",
    mov_text: "Timed text",
    ssa: "SSA",
    srt: "SRT",
    subrip: "SRT",
    webvtt: "WebVTT",
  };
  return labels[codec.toLowerCase()] ?? codec.toUpperCase().replaceAll("_", " ");
}

function audioTrackLabel(ordinal: number, track: AudioTrack) {
  const identity = track.title || track.language?.toUpperCase() || `Track ${ordinal + 1}`;
  return identity.length > 24 ? `${identity.slice(0, 23)}…` : identity;
}

function audioTrackSummary(track: AudioTrack) {
  const channels = track.channelLayout
    ? track.channelLayout.replaceAll("_", " ")
    : track.channels === 1
      ? "Mono"
      : track.channels === 2
        ? "Stereo"
        : track.channels
          ? `${track.channels} channels`
          : null;
  const sampleRate = track.sampleRate
    ? `${(track.sampleRate / 1000).toFixed(1).replace(/\.0$/, "")} kHz`
    : null;
  return [
    track.language?.toUpperCase(),
    track.title,
    subtitleCodecLabel(track.codec),
    channels,
    sampleRate,
  ]
    .filter(Boolean)
    .join(", ");
}

interface FilePreviewProps {
  file: FileInfo;
  canDismiss?: boolean;
  onRetry?: () => void;
  onCancel?: () => void;
  onSkip?: () => void;
  onMoveUp?: () => void;
  onMoveDown?: () => void;
  showResultAction?: boolean;
  previewName?: string;
  previewInvalid?: boolean;
  onOpenOperation: (operation: ActiveOperation) => void;
}

export function FilePreview({
  file,
  canDismiss = false,
  onRetry,
  onCancel,
  onSkip,
  onMoveUp,
  onMoveDown,
  showResultAction = true,
  previewName,
  previewInvalid = false,
  onOpenOperation,
}: FilePreviewProps) {
  const operation = useAppStore((s) => s.operation);
  const appState = useAppStore((s) => s.state);
  const queueItem = useAppStore((s) => s.queueItems[file.path]);
  const outputFormat = useAppStore((s) => s.outputFormats[file.path]);
  const pdfSplitMode = useAppStore((s) => s.pdfSplitMode);
  const pdfPageSelection = useAppStore((s) => s.pdfPageSelection);
  const pdfPageImageFormat = useAppStore((s) => s.pdfPageImageFormat);
  const archiveFormat = useAppStore((s) => s.archiveFormat);
  const audioOutputFormat = useAppStore((s) => s.audioOutputFormat);
  const videoEncodingPreset = useAppStore((s) => s.videoEncodingPreset);
  const subtitleOutputFormat = useAppStore((s) => s.subtitleOutputFormat);
  const transcriptionOutputFormat = useAppStore((s) => s.transcriptionOutputFormat);
  const ocrOutputFormat = useAppStore((s) => s.ocrOutputFormat);
  const thumbnailMode = useAppStore((s) => s.thumbnailMode);
  const thumbnailOutputFormat = useAppStore((s) => s.thumbnailOutputFormat);
  const imageExportPresets = useAppStore((s) => s.imageExportPresets);
  const audioTracks = useAppStore((s) => s.audioTracks[file.path]);
  const audioTrackIndex = useAppStore((s) => s.audioTrackIndexes[file.path]);
  const audioTrackError = useAppStore((s) => s.audioTrackErrors[file.path]);
  const subtitleTracks = useAppStore((s) => s.subtitleTracks[file.path]);
  const subtitleTrackIndex = useAppStore((s) => s.subtitleTrackIndexes[file.path]);
  const subtitleTrackError = useAppStore((s) => s.subtitleTrackErrors[file.path]);
  const setSubtitleTrackIndex = useAppStore((s) => s.setSubtitleTrackIndex);
  const setAudioTrackIndex = useAppStore((s) => s.setAudioTrackIndex);
  const setOutputFormat = useAppStore((s) => s.setOutputFormat);
  const removeFile = useAppStore((s) => s.removeFile);
  const [thumbnail, setThumbnail] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    const meta = FORMAT_INFO[file.format];

    setThumbnail(null);
    if (!meta || !PREVIEWABLE.has(meta.category) || file.format === "heic") {
      return () => {
        cancelled = true;
      };
    }

    readFileThumbnail(file.path)
      .then((result) => {
        if (!cancelled) setThumbnail(result);
      })
      .catch(() => {
        if (!cancelled) setThumbnail(null);
      });

    return () => {
      cancelled = true;
    };
  }, [file.format, file.path]);

  const meta = FORMAT_INFO[file.format];
  const label =
    (operation === "extractArchive" ? archivePathLabel(file.name) : null) ??
    meta?.label ??
    file.extension.toUpperCase();
  const compatible = getCompatibleFormats(file.format);
  const status = queueItem?.status ?? "pending";
  const showRetry =
    (appState === "done" || appState === "error") &&
    Boolean(onRetry) &&
    ["failed", "skipped", "cancelled"].includes(status);
  const progress = queueItem?.progress ?? 0;
  const outputSize = queueItem?.result?.output_size;
  const outputCount = queueItem?.result?.output_paths?.length ?? 0;
  const completedOcrIsPdf =
    status === "completed"
      ? (queueItem?.result?.output_path.toLowerCase().endsWith(".pdf") ?? false)
      : ocrOutputFormat === "searchablePdf";
  const extractedPageCount = getPageSelectionCount(pdfPageSelection);
  const savedPercent =
    outputSize !== undefined && file.size > 0
      ? (operation === "convert" && outputFormat === "pdf") ||
        operation === "mergePdf" ||
        operation === "splitPdf" ||
        operation === "exportPdfPages" ||
        operation === "createArchive" ||
        operation === "extractArchive" ||
        operation === "removeMetadata" ||
        operation === "removeAudio" ||
        operation === "extractSubtitles" ||
        operation === "generateThumbnails" ||
        operation === "extractAudio" ||
        operation === "transcribe" ||
        operation === "extractText" ||
        operation === "recognizeText" ||
        operation === "exportImages"
        ? null
        : Math.round(Math.max(0, (1 - outputSize / file.size) * 100))
      : null;
  const failureMessage = queueItem?.error
    ? (queueItem.error.detail?.message ??
      queueItem.error.detail?.install_hint ??
      CONVERSION_ERROR_MESSAGES[queueItem.error.kind])
    : null;
  const selectedAudioTrack = audioTracks?.find((track) => track.streamIndex === audioTrackIndex);
  const supportedSubtitleTracks = subtitleTracks?.filter((track) => track.supported) ?? [];
  const selectedSubtitleTrack = supportedSubtitleTracks.find(
    (track) => track.streamIndex === subtitleTrackIndex,
  );

  return (
    <div className="file-preview-row relative flex items-center gap-2.5 border border-[#44464f] bg-[#0e0e10] p-2">
      {/* Thumbnail or neutral placeholder */}
      <div className="size-9 shrink-0 overflow-hidden border border-[#44464f] bg-[#1a1a1d]">
        {thumbnail && <img src={thumbnail} alt="" className="size-full object-cover" />}
      </div>

      {/* Info */}
      <div className="file-preview-info min-w-0 flex-1">
        <p title={file.name} className="truncate text-[13px] font-medium text-white/90">
          {file.name}
        </p>
        <div className="mt-1 flex min-w-0 items-center gap-2 text-[10px] leading-4 text-white/40">
          {status === "failed" ? (
            <span className="truncate text-red-300/75" title={failureMessage ?? undefined}>
              {failureMessage}
            </span>
          ) : status === "cancelled" ? (
            <span className="whitespace-nowrap text-white/40">Cancelled</span>
          ) : status === "skipped" ? (
            <span className="whitespace-nowrap text-white/40">Skipped</span>
          ) : appState === "loaded" && operation === "extractAudio" ? (
            <span
              className={`truncate ${selectedAudioTrack ? "text-white/45" : "text-amber-200/70"}`}
            >
              {audioTracks === undefined
                ? "Reading audio tracks…"
                : audioTrackError
                  ? audioTrackError
                  : selectedAudioTrack
                    ? audioTrackSummary(selectedAudioTrack)
                    : audioTracks.length > 0
                      ? "Choose an audio track"
                      : "No audio tracks found"}
            </span>
          ) : appState === "loaded" && operation === "extractSubtitles" ? (
            <span
              className={`truncate ${
                selectedSubtitleTrack ? "text-white/45" : "text-amber-200/70"
              }`}
            >
              {subtitleTracks === undefined
                ? "Reading subtitle tracks…"
                : subtitleTrackError
                  ? subtitleTrackError
                  : selectedSubtitleTrack
                    ? [
                        selectedSubtitleTrack.language?.toUpperCase(),
                        selectedSubtitleTrack.title,
                        subtitleCodecLabel(selectedSubtitleTrack.codec),
                      ]
                        .filter(Boolean)
                        .join(", ")
                    : supportedSubtitleTracks.length > 0
                      ? "Choose a subtitle track"
                      : subtitleTracks.length > 0
                        ? "No exportable text subtitles"
                        : "No subtitle tracks found"}
            </span>
          ) : appState === "loaded" && operation === "rename" && previewName ? (
            <span
              className={`truncate ${previewInvalid ? "text-red-300/80" : "text-[#b0c6ff]/75"}`}
            >
              → {previewName}
            </span>
          ) : status === "completed" && operation === "rename" ? (
            <span className="whitespace-nowrap text-emerald-300/65">Renamed</span>
          ) : status === "completed" && operation === "mergePdf" ? (
            <span className="whitespace-nowrap">Included in combined PDF</span>
          ) : status === "completed" && operation === "exportImages" ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              {outputCount} {outputCount === 1 ? "variant" : "variants"} created
              {outputSize !== undefined ? ` · ${formatFileSize(outputSize)}` : ""}
            </span>
          ) : status === "completed" && operation === "extractAudio" && outputSize !== undefined ? (
            <span className="whitespace-nowrap">{formatFileSize(outputSize)} extracted</span>
          ) : status === "completed" && operation === "removeAudio" && outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              Audio removed · {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" && operation === "extractText" && outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              Text extracted · {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" &&
            operation === "recognizeText" &&
            outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              {completedOcrIsPdf ? "Searchable PDF created" : "Text recognized"}
              {" · "}
              {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" && operation === "transcribe" && outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              Transcript created · {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" &&
            operation === "extractSubtitles" &&
            outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              Subtitles extracted, {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" &&
            operation === "generateThumbnails" &&
            outputSize !== undefined ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              {thumbnailMode === "contactSheet" ? "Contact sheet" : "Thumbnail"} generated,{" "}
              {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" &&
            operation === "removeMetadata" &&
            outputSize !== undefined ? (
            <span className="truncate text-emerald-300/65">
              Metadata removed · {formatFileSize(file.size)} → {formatFileSize(outputSize)}
            </span>
          ) : status === "completed" && queueItem?.stage === "Already optimized" ? (
            <span className="whitespace-nowrap">
              Already optimized · {formatFileSize(file.size)}
            </span>
          ) : status === "completed" && operation === "createArchive" ? (
            <span className="whitespace-nowrap">
              {archiveFormat === "gzip"
                ? "Compressed as GZIP"
                : `Included in ${archiveFormatLabel(archiveFormat)} archive`}
            </span>
          ) : status === "completed" &&
            operation === "extractArchive" &&
            outputSize !== undefined ? (
            <span className="whitespace-nowrap">{formatFileSize(outputSize)} extracted</span>
          ) : status === "completed" && operation === "splitPdf" ? (
            <span className="whitespace-nowrap">
              {pdfSplitMode === "extract" && extractedPageCount
                ? `${extractedPageCount} ${extractedPageCount === 1 ? "page" : "pages"} extracted`
                : `${outputCount} PDFs created`}
            </span>
          ) : status === "completed" && operation === "exportPdfPages" ? (
            <span className="whitespace-nowrap text-emerald-300/65">
              {outputCount} {outputCount === 1 ? "image" : "images"} created
              {outputSize !== undefined ? ` · ${formatFileSize(outputSize)}` : ""}
            </span>
          ) : status === "completed" && outputSize !== undefined ? (
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

      <div className="file-preview-trailing flex min-w-0 shrink-0 items-center gap-2.5">
        <div className="file-preview-controls flex min-w-0 shrink-0 items-center gap-2.5">
          {appState === "loaded" && operation === "mergePdf" && (onMoveUp || onMoveDown) && (
            <div className="flex shrink-0 items-center">
              <button
                type="button"
                onClick={onMoveUp}
                disabled={!onMoveUp}
                className="icon-button disabled:pointer-events-none"
                aria-label={`Move ${file.name} up`}
                title="Move up"
              >
                <svg
                  className="size-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  aria-hidden="true"
                >
                  <path d="m6 15 6-6 6 6" />
                </svg>
              </button>
              <button
                type="button"
                onClick={onMoveDown}
                disabled={!onMoveDown}
                className="icon-button disabled:pointer-events-none"
                aria-label={`Move ${file.name} down`}
                title="Move down"
              >
                <svg
                  className="size-3"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  aria-hidden="true"
                >
                  <path d="m6 9 6 6 6-6" />
                </svg>
              </button>
            </div>
          )}

          <span className="shrink-0 border border-[#44464f] bg-[#201f22] px-1.5 py-1 text-[10px] font-medium text-white/55">
            {operation === "extractAudio"
              ? (FORMAT_INFO[audioOutputFormat]?.label ?? audioOutputFormat.toUpperCase())
              : operation === "encodeVideo"
                ? videoEncodingPreset === "web"
                  ? "WEBM"
                  : videoEncodingPreset === "archive"
                    ? "MKV"
                    : "MP4"
                : operation === "recognizeText"
                  ? completedOcrIsPdf
                    ? "PDF"
                    : "TXT"
                  : operation === "extractText"
                    ? "TXT"
                    : operation === "transcribe"
                      ? transcriptionOutputFormat.toUpperCase()
                      : operation === "extractSubtitles"
                        ? subtitleOutputFormat === "vtt"
                          ? "VTT"
                          : "SRT"
                        : operation === "generateThumbnails"
                          ? thumbnailOutputFormat === "jpeg"
                            ? "JPEG"
                            : "PNG"
                          : operation === "exportImages"
                            ? `${imageExportPresets.length}×`
                            : operation === "exportPdfPages"
                              ? pdfPageImageFormat.toUpperCase()
                              : operation === "convert" && status === "completed"
                                ? (FORMAT_INFO[outputFormat ?? ""]?.label ??
                                  outputFormat?.toUpperCase() ??
                                  label)
                                : label}
          </span>

          {appState === "loaded" &&
            operation === "extractAudio" &&
            audioTracks &&
            audioTracks.length > 0 && (
              <div className="relative max-w-24 shrink-0">
                <select
                  value={audioTrackIndex ?? ""}
                  onChange={(event) => setAudioTrackIndex(file.path, Number(event.target.value))}
                  aria-label={`Audio track for ${file.name}`}
                  className="h-7 w-full appearance-none border border-[#44464f] bg-[#0e0e10] pl-2 pr-5 text-[9px] text-white/65 outline-none focus:border-[#b0c6ff]"
                >
                  {audioTracks.map((track, index) => (
                    <option key={track.streamIndex} value={track.streamIndex}>
                      {audioTrackLabel(index, track)}
                    </option>
                  ))}
                </select>
                <svg
                  className="pointer-events-none absolute right-1.5 top-1/2 size-2 -translate-y-1/2 text-white/35"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  aria-hidden="true"
                >
                  <path d="m7 9 5 5 5-5" />
                </svg>
              </div>
            )}

          {appState === "loaded" &&
            operation === "extractSubtitles" &&
            supportedSubtitleTracks.length > 0 && (
              <div className="relative max-w-24 shrink-0">
                <select
                  value={subtitleTrackIndex ?? ""}
                  onChange={(event) => setSubtitleTrackIndex(file.path, Number(event.target.value))}
                  aria-label={`Subtitle track for ${file.name}`}
                  className="h-7 w-full appearance-none border border-[#44464f] bg-[#0e0e10] pl-2 pr-5 text-[9px] text-white/65 outline-none focus:border-[#b0c6ff]"
                >
                  {supportedSubtitleTracks.map((track, index) => (
                    <option key={track.streamIndex} value={track.streamIndex}>
                      {subtitleTrackLabel(index, track.language, track.title)}
                    </option>
                  ))}
                </select>
                <svg
                  className="pointer-events-none absolute right-1.5 top-1/2 size-2 -translate-y-1/2 text-white/35"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2"
                  aria-hidden="true"
                >
                  <path d="m7 9 5 5 5-5" />
                </svg>
              </div>
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
              <div className="file-preview-format-select relative min-w-0 shrink-0">
                <select
                  value={outputFormat ?? ""}
                  onChange={(event) => setOutputFormat(event.target.value, file.path)}
                  aria-label={`Output format for ${file.name}`}
                  disabled={appState !== "loaded"}
                  className="h-7 min-w-16 max-w-full appearance-none border border-[#44464f] bg-[#0e0e10] pl-2 pr-6 text-[10px] font-medium text-[#e5e1e4] outline-none focus:border-[#b0c6ff] disabled:opacity-55"
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
        </div>

        <div className="file-preview-actions flex min-w-0 shrink-0 items-center gap-2.5">
          {status === "running" && onCancel && (
            <button
              type="button"
              onClick={onCancel}
              className="secondary-button compact-button shrink-0"
            >
              Cancel
            </button>
          )}

          {appState === "converting" && status === "pending" && onSkip && (
            <button
              type="button"
              onClick={onSkip}
              className="secondary-button compact-button shrink-0"
            >
              Skip
            </button>
          )}

          {status === "completed" && queueItem?.result && showResultAction && (
            <OutputActionsMenu
              paths={outputPathsForResult(queueItem.result)}
              sourceOperation={operation}
              onOpenOperation={onOpenOperation}
              label={`Output actions for ${file.name}`}
            />
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
              className="secondary-button compact-button shrink-0"
            >
              Retry
            </button>
          )}

          {/* Clear button */}
          {canDismiss && (
            <button
              type="button"
              onClick={() => removeFile(file.path)}
              className="icon-button"
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
        </div>
      </div>

      {status === "running" && (
        <div
          role="progressbar"
          aria-label={`Progress for ${file.name}`}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={progress < 0 ? undefined : Math.min(100, Math.max(0, progress))}
          aria-valuetext={
            progress < 0
              ? queueItem?.stage || "Working"
              : `${Math.round(progress)}%${queueItem?.stage ? `, ${queueItem.stage}` : ""}`
          }
          className="absolute inset-x-0 bottom-0 h-px overflow-hidden bg-white/[0.06]"
        >
          {progress < 0 ? (
            <div className="h-full w-1/4 animate-indeterminate bg-[#b0c6ff]" />
          ) : (
            <div
              className="h-full bg-[#b0c6ff] transition-[width] duration-150 ease-out"
              style={{ width: `${progress}%` }}
            />
          )}
        </div>
      )}
    </div>
  );
}
