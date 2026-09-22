import type {
  ArchiveFormat,
  AudioOutputFormat,
  AudioCompressionPreset,
  ImageExportPreset,
  ImageOptimizationGoal,
  OcrOutputFormat,
  Operation,
  PdfCompressionGoal,
  PdfCompressionPreset,
  PdfPageImageFormat,
  PdfPageImageResolution,
  PdfSplitMode,
  RenameSettings,
  SubtitleOutputFormat,
  ThumbnailMode,
  ThumbnailOutputFormat,
  TranscriptionModel,
  TranscriptionOutputFormat,
  VideoCompressionGoal,
  VideoEncodingPreset,
  VideoQuality,
  VideoResolution,
} from "../types";
import { ACTIVE_OPERATION_IDS, isActiveOperation, isRetiredOperation } from "./operations.ts";
import { TRANSCRIPTION_LANGUAGE_SET } from "./transcription.ts";
import { ARCHIVE_FORMATS } from "./archive.ts";
import {
  DEFAULT_VIDEO_TARGET_SIZE_BYTES,
  effectiveVideoCompressionGoal,
  isVideoTargetSizeBytes,
} from "./videoCompression.ts";
import { DEFAULT_PDF_TARGET_SIZE_BYTES, isPdfTargetSizeBytes } from "./pdfCompression.ts";
import { DEFAULT_IMAGE_TARGET_SIZE_BYTES, isImageTargetSizeBytes } from "./imageOptimization.ts";

export { ACTIVE_OPERATION_IDS, isActiveOperation, OPERATION_IDS } from "./operations.ts";

const WORKSPACE_DRAFT_KEY = "convertkit.workspaceDraft.v1";
export const WORKSPACE_DRAFT_VERSION = 1;
export const MAX_WORKSPACE_DRAFT_BYTES = 1024 * 1024;

export interface WorkspaceSessionDraft {
  paths: string[];
  outputFormats: Record<string, string>;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  keepMetadata: boolean;
  imageOptimizationGoal: ImageOptimizationGoal;
  imageTargetSizeBytes: number | null;
  imageExportPresets: ImageExportPreset[];
  audioOutputFormat: AudioOutputFormat;
  audioTrackIndexes: Record<string, number>;
  transcriptionModel: TranscriptionModel;
  transcriptionLanguage: string;
  transcriptionOutputFormat: TranscriptionOutputFormat;
  ocrOutputFormat: OcrOutputFormat;
  videoEncodingPreset: VideoEncodingPreset;
  videoResolution: VideoResolution;
  videoQuality: VideoQuality;
  videoCompressionGoal: VideoCompressionGoal;
  videoTargetSizeBytes: number | null;
  audioCompressionPreset: AudioCompressionPreset;
  subtitleOutputFormat: SubtitleOutputFormat;
  subtitleTrackIndexes: Record<string, number>;
  thumbnailMode: ThumbnailMode;
  thumbnailOutputFormat: ThumbnailOutputFormat;
  pdfSplitMode: PdfSplitMode;
  pdfPageSelection: string;
  pdfPageImageFormat: PdfPageImageFormat;
  pdfPageImageResolution: PdfPageImageResolution;
  pdfCompressionPreset: PdfCompressionPreset;
  pdfCompressionGoal: PdfCompressionGoal;
  pdfTargetSizeBytes: number | null;
  archiveFormat: ArchiveFormat;
  renameSettings: RenameSettings;
}

export interface WorkspaceDraft {
  version: typeof WORKSPACE_DRAFT_VERSION;
  activeOperation: Operation;
  sessions: Partial<Record<Operation, WorkspaceSessionDraft>>;
}

const AUDIO_FORMATS = new Set<AudioOutputFormat>(["mp3", "m4a", "wav", "flac"]);
const TRANSCRIPTION_MODELS = new Set<TranscriptionModel>(["tiny", "base", "small"]);
const TRANSCRIPTION_FORMATS = new Set<TranscriptionOutputFormat>(["txt", "srt", "vtt"]);
const OCR_OUTPUT_FORMATS = new Set<OcrOutputFormat>(["text", "searchablePdf"]);
const IMAGE_EXPORT_PRESETS = new Set<ImageExportPreset>(["web", "email", "social", "preview"]);
const IMAGE_OPTIMIZATION_GOALS = new Set<ImageOptimizationGoal>(["quality", "fileSize"]);
const VIDEO_PRESETS = new Set<VideoEncodingPreset>(["compatible", "smaller", "web", "archive"]);
const VIDEO_RESOLUTIONS = new Set<VideoResolution>(["automatic", "original", "fullHd", "hd"]);
const VIDEO_QUALITIES = new Set<VideoQuality>(["high", "balanced", "smallest"]);
const VIDEO_COMPRESSION_GOALS = new Set<VideoCompressionGoal>(["quality", "fileSize"]);
const AUDIO_COMPRESSION_PRESETS = new Set<AudioCompressionPreset>(["high", "balanced", "smallest"]);
const SUBTITLE_FORMATS = new Set<SubtitleOutputFormat>(["srt", "vtt"]);
const THUMBNAIL_MODES = new Set<ThumbnailMode>(["frame"]);
const THUMBNAIL_FORMATS = new Set<ThumbnailOutputFormat>(["jpeg", "png"]);
const PDF_SPLIT_MODES = new Set<PdfSplitMode>(["everyPage", "extract"]);
const PDF_PAGE_IMAGE_FORMATS = new Set<PdfPageImageFormat>(["png", "jpeg"]);
const PDF_PAGE_IMAGE_RESOLUTIONS = new Set<PdfPageImageResolution>(["screen", "print"]);
const PDF_COMPRESSION_PRESETS = new Set<PdfCompressionPreset>(["high", "balanced", "smallest"]);
const PDF_COMPRESSION_GOALS = new Set<PdfCompressionGoal>(["quality", "fileSize"]);
const ARCHIVE_FORMAT_SET = new Set<ArchiveFormat>(ARCHIVE_FORMATS);

function objectValue(value: unknown): Record<string, unknown> | null {
  return typeof value === "object" && value !== null && !Array.isArray(value)
    ? (value as Record<string, unknown>)
    : null;
}

function enumValue<T extends string>(value: unknown, allowed: Set<T>, fallback: T): T {
  return typeof value === "string" && allowed.has(value as T) ? (value as T) : fallback;
}

function boundedString(value: unknown, fallback: string, maximum: number): string {
  return typeof value === "string" ? value.slice(0, maximum) : fallback;
}

function nullableInteger(value: unknown, maximum: number): number | null {
  return typeof value === "number" && Number.isInteger(value) && value > 0 && value <= maximum
    ? value
    : null;
}

function pathsFrom(value: unknown): string[] {
  if (!Array.isArray(value)) return [];
  const paths: string[] = [];
  const seen = new Set<string>();
  for (const candidate of value) {
    if (
      typeof candidate !== "string" ||
      candidate.length === 0 ||
      candidate.length > 4096 ||
      seen.has(candidate)
    )
      continue;
    seen.add(candidate);
    paths.push(candidate);
    if (paths.length === 100) break;
  }
  return paths;
}

function stringRecord(value: unknown, paths: Set<string>): Record<string, string> {
  const object = objectValue(value);
  if (!object) return {};
  return Object.fromEntries(
    Object.entries(object).filter(
      (entry): entry is [string, string] =>
        paths.has(entry[0]) &&
        typeof entry[1] === "string" &&
        entry[1].length > 0 &&
        entry[1].length <= 16 &&
        /^[a-z0-9]+$/i.test(entry[1]),
    ),
  );
}

function streamIndexRecord(value: unknown, paths: Set<string>): Record<string, number> {
  const object = objectValue(value);
  if (!object) return {};
  return Object.fromEntries(
    Object.entries(object).filter(
      (entry): entry is [string, number] =>
        paths.has(entry[0]) &&
        typeof entry[1] === "number" &&
        Number.isInteger(entry[1]) &&
        entry[1] >= 0 &&
        entry[1] <= 4096,
    ),
  );
}

function imageExportPresetsFrom(value: unknown): ImageExportPreset[] {
  if (!Array.isArray(value)) return ["web"];
  const presets = Array.from(
    new Set(
      value.filter(
        (candidate): candidate is ImageExportPreset =>
          typeof candidate === "string" && IMAGE_EXPORT_PRESETS.has(candidate as ImageExportPreset),
      ),
    ),
  ).slice(0, IMAGE_EXPORT_PRESETS.size);
  return presets.length > 0 ? presets : ["web"];
}

function renameSettingsFrom(value: unknown): RenameSettings {
  const object = objectValue(value) ?? {};
  const numbering =
    object.numbering === "prefix" || object.numbering === "suffix" ? object.numbering : "none";
  return {
    find: boundedString(object.find, "", 120),
    replace: boundedString(object.replace, "", 120),
    prefix: boundedString(object.prefix, "", 120),
    suffix: boundedString(object.suffix, "", 120),
    numbering,
    start:
      typeof object.start === "number" &&
      Number.isInteger(object.start) &&
      object.start >= 0 &&
      object.start <= 999_999
        ? object.start
        : 1,
    padding:
      typeof object.padding === "number" &&
      Number.isInteger(object.padding) &&
      object.padding >= 1 &&
      object.padding <= 8
        ? object.padding
        : 2,
  };
}

function sessionFrom(value: unknown): WorkspaceSessionDraft | null {
  const object = objectValue(value);
  if (!object) return null;
  const paths = pathsFrom(object.paths);
  const pathSet = new Set(paths);
  const videoEncodingPreset = enumValue(object.videoEncodingPreset, VIDEO_PRESETS, "compatible");
  const videoCompressionGoal = effectiveVideoCompressionGoal(
    videoEncodingPreset,
    enumValue(object.videoCompressionGoal, VIDEO_COMPRESSION_GOALS, "quality"),
  );
  return {
    paths,
    outputFormats: stringRecord(object.outputFormats, pathSet),
    resizeWidth: nullableInteger(object.resizeWidth, 32_768),
    resizeHeight: nullableInteger(object.resizeHeight, 32_768),
    preserveAspect: object.preserveAspect !== false,
    keepMetadata: object.keepMetadata === true,
    imageOptimizationGoal: enumValue(
      object.imageOptimizationGoal,
      IMAGE_OPTIMIZATION_GOALS,
      "quality",
    ),
    imageTargetSizeBytes: isImageTargetSizeBytes(object.imageTargetSizeBytes)
      ? object.imageTargetSizeBytes
      : DEFAULT_IMAGE_TARGET_SIZE_BYTES,
    imageExportPresets: imageExportPresetsFrom(object.imageExportPresets),
    audioOutputFormat: enumValue(object.audioOutputFormat, AUDIO_FORMATS, "mp3"),
    audioTrackIndexes: streamIndexRecord(object.audioTrackIndexes, pathSet),
    transcriptionModel: enumValue(object.transcriptionModel, TRANSCRIPTION_MODELS, "base"),
    transcriptionLanguage:
      typeof object.transcriptionLanguage === "string" &&
      TRANSCRIPTION_LANGUAGE_SET.has(object.transcriptionLanguage)
        ? object.transcriptionLanguage
        : "auto",
    transcriptionOutputFormat: enumValue(
      object.transcriptionOutputFormat,
      TRANSCRIPTION_FORMATS,
      "txt",
    ),
    ocrOutputFormat: enumValue(object.ocrOutputFormat, OCR_OUTPUT_FORMATS, "text"),
    videoEncodingPreset,
    videoResolution: enumValue(object.videoResolution, VIDEO_RESOLUTIONS, "automatic"),
    videoQuality: enumValue(object.videoQuality, VIDEO_QUALITIES, "balanced"),
    videoCompressionGoal,
    videoTargetSizeBytes: isVideoTargetSizeBytes(object.videoTargetSizeBytes)
      ? object.videoTargetSizeBytes
      : DEFAULT_VIDEO_TARGET_SIZE_BYTES,
    audioCompressionPreset: enumValue(
      object.audioCompressionPreset,
      AUDIO_COMPRESSION_PRESETS,
      "balanced",
    ),
    subtitleOutputFormat: enumValue(object.subtitleOutputFormat, SUBTITLE_FORMATS, "srt"),
    subtitleTrackIndexes: streamIndexRecord(object.subtitleTrackIndexes, pathSet),
    thumbnailMode: enumValue(object.thumbnailMode, THUMBNAIL_MODES, "frame"),
    thumbnailOutputFormat: enumValue(object.thumbnailOutputFormat, THUMBNAIL_FORMATS, "jpeg"),
    pdfSplitMode: enumValue(object.pdfSplitMode, PDF_SPLIT_MODES, "everyPage"),
    pdfPageSelection: boundedString(object.pdfPageSelection, "", 1000),
    pdfPageImageFormat: enumValue(object.pdfPageImageFormat, PDF_PAGE_IMAGE_FORMATS, "png"),
    pdfPageImageResolution: enumValue(
      object.pdfPageImageResolution,
      PDF_PAGE_IMAGE_RESOLUTIONS,
      "screen",
    ),
    pdfCompressionPreset: enumValue(
      object.pdfCompressionPreset,
      PDF_COMPRESSION_PRESETS,
      "balanced",
    ),
    pdfCompressionGoal: enumValue(object.pdfCompressionGoal, PDF_COMPRESSION_GOALS, "quality"),
    pdfTargetSizeBytes: isPdfTargetSizeBytes(object.pdfTargetSizeBytes)
      ? object.pdfTargetSizeBytes
      : DEFAULT_PDF_TARGET_SIZE_BYTES,
    archiveFormat: enumValue(object.archiveFormat, ARCHIVE_FORMAT_SET, "zip"),
    renameSettings: renameSettingsFrom(object.renameSettings),
  };
}

export function parseWorkspaceDraft(serialized: string | null): WorkspaceDraft | null {
  if (!serialized || serialized.length > MAX_WORKSPACE_DRAFT_BYTES) return null;
  try {
    const object = objectValue(JSON.parse(serialized));
    if (!object || object.version !== WORKSPACE_DRAFT_VERSION) return null;
    if (typeof object.activeOperation !== "string") return null;
    const activeOperation = isActiveOperation(object.activeOperation)
      ? object.activeOperation
      : isRetiredOperation(object.activeOperation)
        ? "convert"
        : null;
    if (!activeOperation) return null;
    const rawSessions = objectValue(object.sessions);
    if (!rawSessions) return null;
    const sessions: Partial<Record<Operation, WorkspaceSessionDraft>> = {};
    for (const operation of ACTIVE_OPERATION_IDS) {
      const session = sessionFrom(rawSessions[operation]);
      if (session) sessions[operation] = session;
    }
    return { version: WORKSPACE_DRAFT_VERSION, activeOperation, sessions };
  } catch {
    return null;
  }
}

export function loadWorkspaceDraft(): WorkspaceDraft | null {
  try {
    return parseWorkspaceDraft(window.localStorage.getItem(WORKSPACE_DRAFT_KEY));
  } catch {
    return null;
  }
}

export function saveWorkspaceDraft(draft: WorkspaceDraft): boolean {
  try {
    let serialized = JSON.stringify(draft);
    if (serialized.length > MAX_WORKSPACE_DRAFT_BYTES) {
      serialized = JSON.stringify({
        ...draft,
        sessions: { [draft.activeOperation]: draft.sessions[draft.activeOperation] },
      });
    }
    if (serialized.length > MAX_WORKSPACE_DRAFT_BYTES) return false;
    window.localStorage.setItem(WORKSPACE_DRAFT_KEY, serialized);
    return true;
  } catch {
    return false;
  }
}
