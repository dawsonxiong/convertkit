import { create } from "zustand";
import type {
  AppState,
  FileInfo,
  ImageExportPreset,
  ImageOptimizationGoal,
  ConversionResult,
  ConversionError,
  Operation,
  ArchiveFormat,
  AudioTrack,
  AudioOutputFormat,
  AudioCompressionPreset,
  VideoEncodingPreset,
  VideoCompressionGoal,
  VideoQuality,
  VideoResolution,
  OcrOutputFormat,
  SubtitleOutputFormat,
  SubtitleTrack,
  ThumbnailMode,
  ThumbnailOutputFormat,
  PdfCompressionPreset,
  PdfCompressionGoal,
  PdfPageImageFormat,
  PdfPageImageResolution,
  PdfSplitMode,
  QueueItemState,
  QueueItemStatus,
  RenameSettings,
  RecentJobSetup,
  TranscriptionModel,
  TranscriptionOutputFormat,
} from "../types";
import { getCompatibleFormats } from "../lib/formats.ts";
import { INPUT_INTAKE_BLOCKED_MESSAGE, isInputIntakeBlocked } from "../lib/appShortcuts.ts";
import {
  ACTIVE_OPERATION_IDS,
  isActiveOperation,
  OPERATION_IDS,
  WORKSPACE_DRAFT_VERSION,
  type WorkspaceDraft,
  type WorkspaceSessionDraft,
} from "../lib/workspaceDraft.ts";
import type { SavedRecipeSettings } from "../lib/savedRecipes.ts";
import { archiveFormatLabel } from "../lib/archive.ts";
import {
  DEFAULT_VIDEO_TARGET_SIZE_BYTES,
  effectiveVideoCompressionGoal,
  isVideoTargetSizeBytes,
} from "../lib/videoCompression.ts";
import { DEFAULT_PDF_TARGET_SIZE_BYTES, isPdfTargetSizeBytes } from "../lib/pdfCompression.ts";
import {
  DEFAULT_IMAGE_TARGET_SIZE_BYTES,
  isImageTargetSizeBytes,
} from "../lib/imageOptimization.ts";

export const MAX_QUEUE_ITEMS = 100;

const OUTPUT_PREFERENCES_KEY = "convertkit.outputPreferences.v1";
const DEFAULT_OUTPUT_SUFFIXES: Record<Operation, string> = {
  convert: "",
  resize: "-resized",
  optimize: "-optimized",
  exportImages: "",
  encodeVideo: "-encoded",
  compressAudio: "-compressed",
  removeAudio: "-silent",
  extractSubtitles: "-subtitles",
  generateThumbnails: "-thumbnail",
  removeMetadata: "-clean",
  extractAudio: "-audio",
  transcribe: "-transcript",
  extractText: "-text",
  recognizeText: "-ocr",
  mergePdf: "-combined",
  splitPdf: "-pages",
  exportPdfPages: "-page",
  compressPdf: "-compressed",
  createArchive: "-archive",
  extractArchive: "-extracted",
  rename: "",
  inspect: "",
};

interface OutputPreferences {
  outputDirectory: string | null;
  outputSuffixes: Record<Operation, string>;
}

type WorkspaceAdmissionPhase = "idle" | "restoring" | "preflighting" | "running";

interface WorkspaceAdmission {
  phase: WorkspaceAdmissionPhase;
  token: string | null;
  operation: Operation | null;
}

const IDLE_WORKSPACE_ADMISSION: WorkspaceAdmission = {
  phase: "idle",
  token: null,
  operation: null,
};

const STARTUP_RESTORE_TOKEN = "startup-workspace-restore";

function initialWorkspaceAdmission(): WorkspaceAdmission {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window
    ? { phase: "restoring", token: STARTUP_RESTORE_TOKEN, operation: null }
    : IDLE_WORKSPACE_ADMISSION;
}

export function workspaceAdmissionIsPending(admission: WorkspaceAdmission): boolean {
  return admission.phase === "restoring" || admission.phase === "preflighting";
}

const defaultOutputPreferences: OutputPreferences = {
  outputDirectory: null,
  outputSuffixes: DEFAULT_OUTPUT_SUFFIXES,
};

function loadOutputPreferences(): OutputPreferences {
  try {
    const value = window.localStorage.getItem(OUTPUT_PREFERENCES_KEY);
    if (!value) return defaultOutputPreferences;
    const stored = JSON.parse(value) as Partial<OutputPreferences>;
    const storedSuffixes = (stored.outputSuffixes ?? {}) as Partial<
      Record<Operation | "createZip" | "extractZip", string>
    >;
    return {
      outputDirectory:
        typeof stored.outputDirectory === "string" && stored.outputDirectory.trim()
          ? stored.outputDirectory
          : null,
      outputSuffixes: {
        convert:
          typeof stored.outputSuffixes?.convert === "string"
            ? stored.outputSuffixes.convert
            : DEFAULT_OUTPUT_SUFFIXES.convert,
        resize:
          typeof stored.outputSuffixes?.resize === "string"
            ? stored.outputSuffixes.resize
            : DEFAULT_OUTPUT_SUFFIXES.resize,
        optimize:
          typeof stored.outputSuffixes?.optimize === "string"
            ? stored.outputSuffixes.optimize
            : DEFAULT_OUTPUT_SUFFIXES.optimize,
        exportImages:
          typeof stored.outputSuffixes?.exportImages === "string"
            ? stored.outputSuffixes.exportImages
            : DEFAULT_OUTPUT_SUFFIXES.exportImages,
        encodeVideo:
          typeof stored.outputSuffixes?.encodeVideo === "string"
            ? stored.outputSuffixes.encodeVideo
            : DEFAULT_OUTPUT_SUFFIXES.encodeVideo,
        compressAudio:
          typeof stored.outputSuffixes?.compressAudio === "string"
            ? stored.outputSuffixes.compressAudio
            : DEFAULT_OUTPUT_SUFFIXES.compressAudio,
        removeAudio:
          typeof stored.outputSuffixes?.removeAudio === "string"
            ? stored.outputSuffixes.removeAudio
            : DEFAULT_OUTPUT_SUFFIXES.removeAudio,
        extractSubtitles:
          typeof stored.outputSuffixes?.extractSubtitles === "string"
            ? stored.outputSuffixes.extractSubtitles
            : DEFAULT_OUTPUT_SUFFIXES.extractSubtitles,
        generateThumbnails:
          typeof stored.outputSuffixes?.generateThumbnails === "string"
            ? stored.outputSuffixes.generateThumbnails
            : DEFAULT_OUTPUT_SUFFIXES.generateThumbnails,
        removeMetadata:
          typeof stored.outputSuffixes?.removeMetadata === "string"
            ? stored.outputSuffixes.removeMetadata
            : DEFAULT_OUTPUT_SUFFIXES.removeMetadata,
        extractAudio:
          typeof stored.outputSuffixes?.extractAudio === "string"
            ? stored.outputSuffixes.extractAudio
            : DEFAULT_OUTPUT_SUFFIXES.extractAudio,
        transcribe:
          typeof stored.outputSuffixes?.transcribe === "string"
            ? stored.outputSuffixes.transcribe
            : DEFAULT_OUTPUT_SUFFIXES.transcribe,
        extractText:
          typeof stored.outputSuffixes?.extractText === "string"
            ? stored.outputSuffixes.extractText
            : DEFAULT_OUTPUT_SUFFIXES.extractText,
        recognizeText:
          typeof stored.outputSuffixes?.recognizeText === "string"
            ? stored.outputSuffixes.recognizeText
            : DEFAULT_OUTPUT_SUFFIXES.recognizeText,
        mergePdf:
          typeof stored.outputSuffixes?.mergePdf === "string"
            ? stored.outputSuffixes.mergePdf
            : DEFAULT_OUTPUT_SUFFIXES.mergePdf,
        splitPdf:
          typeof stored.outputSuffixes?.splitPdf === "string"
            ? stored.outputSuffixes.splitPdf
            : DEFAULT_OUTPUT_SUFFIXES.splitPdf,
        exportPdfPages:
          typeof stored.outputSuffixes?.exportPdfPages === "string"
            ? stored.outputSuffixes.exportPdfPages
            : DEFAULT_OUTPUT_SUFFIXES.exportPdfPages,
        compressPdf:
          typeof stored.outputSuffixes?.compressPdf === "string"
            ? stored.outputSuffixes.compressPdf
            : DEFAULT_OUTPUT_SUFFIXES.compressPdf,
        createArchive:
          typeof storedSuffixes.createArchive === "string"
            ? storedSuffixes.createArchive
            : typeof storedSuffixes.createZip === "string"
              ? storedSuffixes.createZip
              : DEFAULT_OUTPUT_SUFFIXES.createArchive,
        extractArchive:
          typeof storedSuffixes.extractArchive === "string"
            ? storedSuffixes.extractArchive
            : typeof storedSuffixes.extractZip === "string"
              ? storedSuffixes.extractZip
              : DEFAULT_OUTPUT_SUFFIXES.extractArchive,
        rename: "",
        inspect: "",
      },
    };
  } catch {
    return defaultOutputPreferences;
  }
}

function saveOutputPreferences(preferences: OutputPreferences) {
  try {
    window.localStorage.setItem(OUTPUT_PREFERENCES_KEY, JSON.stringify(preferences));
  } catch {
    // Conversion should keep working if preferences cannot be persisted.
  }
}

const createQueueItem = (): QueueItemState => ({
  status: "pending",
  progress: 0,
  stage: "Waiting",
  result: null,
  error: null,
});

interface OperationSession {
  state: AppState;
  files: FileInfo[];
  file: FileInfo | null;
  outputFormats: Record<string, string>;
  outputFormat: string | null;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  keepMetadata: boolean;
  imageOptimizationGoal: ImageOptimizationGoal;
  imageTargetSizeBytes: number | null;
  imageExportPresets: ImageExportPreset[];
  audioOutputFormat: AudioOutputFormat;
  audioTracks: Record<string, AudioTrack[]>;
  audioTrackIndexes: Record<string, number>;
  audioTrackErrors: Record<string, string>;
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
  subtitleTracks: Record<string, SubtitleTrack[]>;
  subtitleTrackIndexes: Record<string, number>;
  subtitleTrackErrors: Record<string, string>;
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
  archivePassword: string;
  archivePasswordConfirmation: string;
  extractArchivePasswords: Record<string, string>;
  renameSettings: RenameSettings;
  queueItems: Record<string, QueueItemState>;
  activePath: string | null;
  cancelBatchRequested: boolean;
  progress: number;
  progressStage: string;
  result: ConversionResult | null;
  results: ConversionResult[];
  error: ConversionError | null;
  jobId: string | null;
}

interface AppStore {
  state: AppState;
  operation: Operation;
  sessions: Record<Operation, OperationSession>;
  files: FileInfo[];
  file: FileInfo | null;
  outputFormats: Record<string, string>;
  outputFormat: string | null;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  keepMetadata: boolean;
  imageOptimizationGoal: ImageOptimizationGoal;
  imageTargetSizeBytes: number | null;
  imageExportPresets: ImageExportPreset[];
  audioOutputFormat: AudioOutputFormat;
  audioTracks: Record<string, AudioTrack[]>;
  audioTrackIndexes: Record<string, number>;
  audioTrackErrors: Record<string, string>;
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
  subtitleTracks: Record<string, SubtitleTrack[]>;
  subtitleTrackIndexes: Record<string, number>;
  subtitleTrackErrors: Record<string, string>;
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
  archivePassword: string;
  archivePasswordConfirmation: string;
  extractArchivePasswords: Record<string, string>;
  renameSettings: RenameSettings;
  queueItems: Record<string, QueueItemState>;
  activePath: string | null;
  cancelBatchRequested: boolean;
  progress: number; // 0-100, -1 for indeterminate
  progressStage: string;
  result: ConversionResult | null;
  results: ConversionResult[];
  error: ConversionError | null;
  jobId: string | null;
  rejectionMessage: string | null;
  outputDirectory: string | null;
  outputSuffixes: Record<Operation, string>;
  workspaceAdmission: WorkspaceAdmission;

  selectFile: (path: string) => void;
  addFiles: (files: FileInfo[]) => void;
  addFilesForOperation: (operation: Operation, files: FileInfo[]) => void;
  removeFile: (path: string) => void;
  moveFile: (path: string, direction: -1 | 1) => void;
  setOperation: (operation: Operation) => void;
  setRejection: (message: string) => void;
  clearRejection: () => void;
  setOutputFormat: (format: string, path?: string) => void;
  setCompatibleOutputFormats: (format: string) => void;
  setResizeDimensions: (width: number | null, height: number | null) => void;
  setPreserveAspect: (preserve: boolean) => void;
  setKeepMetadata: (keep: boolean) => void;
  setImageOptimizationGoal: (goal: ImageOptimizationGoal) => void;
  setImageTargetSizeBytes: (bytes: number | null) => void;
  setImageExportPresets: (presets: ImageExportPreset[]) => void;
  setAudioOutputFormat: (format: AudioOutputFormat) => void;
  setAudioTracks: (path: string, tracks: AudioTrack[], error?: string) => void;
  setAudioTrackIndex: (path: string, streamIndex: number) => void;
  setTranscriptionModel: (model: TranscriptionModel) => void;
  setTranscriptionLanguage: (language: string) => void;
  setTranscriptionOutputFormat: (format: TranscriptionOutputFormat) => void;
  setOcrOutputFormat: (format: OcrOutputFormat) => void;
  setVideoEncodingPreset: (preset: VideoEncodingPreset) => void;
  setVideoResolution: (resolution: VideoResolution) => void;
  setVideoQuality: (quality: VideoQuality) => void;
  setVideoCompressionGoal: (goal: VideoCompressionGoal) => void;
  setVideoTargetSizeBytes: (bytes: number | null) => void;
  setAudioCompressionPreset: (preset: AudioCompressionPreset) => void;
  setSubtitleOutputFormat: (format: SubtitleOutputFormat) => void;
  setSubtitleTracks: (path: string, tracks: SubtitleTrack[], error?: string) => void;
  setSubtitleTrackIndex: (path: string, streamIndex: number) => void;
  setThumbnailMode: (mode: ThumbnailMode) => void;
  setThumbnailOutputFormat: (format: ThumbnailOutputFormat) => void;
  setPdfSplitMode: (mode: PdfSplitMode) => void;
  setPdfPageSelection: (selection: string) => void;
  setPdfPageImageFormat: (format: PdfPageImageFormat) => void;
  setPdfPageImageResolution: (resolution: PdfPageImageResolution) => void;
  setPdfCompressionPreset: (preset: PdfCompressionPreset) => void;
  setPdfCompressionGoal: (goal: PdfCompressionGoal) => void;
  setPdfTargetSizeBytes: (bytes: number | null) => void;
  setArchiveFormat: (format: ArchiveFormat) => void;
  setArchivePassword: (password: string) => void;
  setArchivePasswordConfirmation: (password: string) => void;
  setExtractArchivePassword: (path: string, password: string) => void;
  setRenameSettings: (settings: Partial<RenameSettings>) => void;
  applyRecipeSettings: (settings?: SavedRecipeSettings) => void;
  setOutputDirectory: (directory: string | null) => void;
  setOutputSuffix: (operation: Operation, suffix: string) => void;
  beginWorkspaceRestore: () => string | null;
  completeWorkspaceRestore: (token: string) => boolean;
  claimRecentJobLoad: (operation: Operation, token: string) => boolean;
  commitRecentJobLoad: (
    operation: Operation,
    token: string,
    files: FileInfo[],
    setup: RecentJobSetup,
  ) => boolean;
  claimWorkspacePreflight: (operation: Operation, token: string) => boolean;
  releaseWorkspaceClaim: (token: string) => boolean;
  startClaimedBatch: (operation: Operation, token: string, paths: string[]) => boolean;
  startBatch: (paths: string[]) => void;
  startQueueItem: (path: string, jobId: string) => void;
  startGroupJob: (paths: string[], jobId: string) => void;
  completeQueueItem: (path: string, result: ConversionResult) => void;
  completeGroupJob: (paths: string[], result: ConversionResult) => void;
  applyRenamedPaths: (originalPaths: string[], renamedPaths: string[]) => void;
  failQueueItem: (path: string, error: ConversionError, status?: QueueItemStatus) => void;
  failGroupJob: (paths: string[], error: ConversionError, status?: QueueItemStatus) => void;
  skipQueueItem: (path: string) => void;
  requestBatchCancel: () => void;
  cancelRemainingItems: (paths: string[]) => void;
  finishBatch: () => void;
  startConversion: (jobId: string) => void;
  setActiveJob: (jobId: string) => void;
  updateProgress: (jobId: string, percent: number, stage: string) => void;
  setResult: (result: ConversionResult) => void;
  setResults: (results: ConversionResult[]) => void;
  setJobError: (jobId: string, error: ConversionError) => void;
  setError: (error: ConversionError) => void;
  reset: () => void;
}

const createEmptySession = (): OperationSession => ({
  state: "empty",
  files: [],
  file: null,
  outputFormats: {},
  outputFormat: null,
  resizeWidth: null,
  resizeHeight: null,
  preserveAspect: true,
  keepMetadata: false,
  imageOptimizationGoal: "quality",
  imageTargetSizeBytes: DEFAULT_IMAGE_TARGET_SIZE_BYTES,
  imageExportPresets: ["web"],
  audioOutputFormat: "mp3",
  audioTracks: {},
  audioTrackIndexes: {},
  audioTrackErrors: {},
  transcriptionModel: "base",
  transcriptionLanguage: "auto",
  transcriptionOutputFormat: "txt",
  ocrOutputFormat: "text",
  videoEncodingPreset: "compatible",
  videoResolution: "automatic",
  videoQuality: "balanced",
  videoCompressionGoal: "quality",
  videoTargetSizeBytes: DEFAULT_VIDEO_TARGET_SIZE_BYTES,
  audioCompressionPreset: "balanced",
  subtitleOutputFormat: "srt",
  subtitleTracks: {},
  subtitleTrackIndexes: {},
  subtitleTrackErrors: {},
  thumbnailMode: "frame",
  thumbnailOutputFormat: "jpeg",
  pdfSplitMode: "everyPage",
  pdfPageSelection: "",
  pdfPageImageFormat: "png",
  pdfPageImageResolution: "screen",
  pdfCompressionPreset: "balanced",
  pdfCompressionGoal: "quality",
  pdfTargetSizeBytes: DEFAULT_PDF_TARGET_SIZE_BYTES,
  archiveFormat: "zip",
  archivePassword: "",
  archivePasswordConfirmation: "",
  extractArchivePasswords: {},
  renameSettings: {
    find: "",
    replace: "",
    prefix: "",
    suffix: "",
    numbering: "none",
    start: 1,
    padding: 2,
  },
  queueItems: {},
  activePath: null,
  cancelBatchRequested: false,
  progress: 0,
  progressStage: "",
  result: null,
  results: [],
  error: null,
  jobId: null,
});

function sessionWithRecipeSettings(
  session: OperationSession,
  settings?: SavedRecipeSettings,
): OperationSession {
  if (!settings) return session;
  switch (settings.kind) {
    case "resize":
      return {
        ...session,
        resizeWidth: settings.width,
        resizeHeight: settings.height,
        preserveAspect: settings.preserveAspect,
      };
    case "optimize":
      return {
        ...session,
        keepMetadata: settings.keepMetadata,
        imageOptimizationGoal: settings.compressionGoal,
        imageTargetSizeBytes: settings.targetSizeBytes ?? DEFAULT_IMAGE_TARGET_SIZE_BYTES,
      };
    case "exportImages":
      return { ...session, imageExportPresets: [...settings.presets] };
    case "encodeVideo":
      return {
        ...session,
        videoEncodingPreset: settings.preset,
        videoResolution: settings.resolution,
        videoQuality: settings.quality,
        videoCompressionGoal: effectiveVideoCompressionGoal(
          settings.preset,
          settings.compressionGoal,
        ),
        videoTargetSizeBytes: settings.targetSizeBytes ?? DEFAULT_VIDEO_TARGET_SIZE_BYTES,
      };
    case "compressAudio":
      return { ...session, audioCompressionPreset: settings.preset };
    case "extractAudio":
      return { ...session, audioOutputFormat: settings.outputFormat };
    case "transcribe":
      return {
        ...session,
        transcriptionModel: settings.model,
        transcriptionLanguage: settings.language,
        transcriptionOutputFormat: settings.outputFormat,
      };
    case "recognizeText":
      return { ...session, ocrOutputFormat: settings.outputFormat };
    case "extractSubtitles":
      return { ...session, subtitleOutputFormat: settings.outputFormat };
    case "generateThumbnails":
      return {
        ...session,
        thumbnailMode: settings.mode,
        thumbnailOutputFormat: settings.outputFormat,
      };
    case "splitPdf":
      return { ...session, pdfSplitMode: settings.mode };
    case "exportPdfPages":
      return {
        ...session,
        pdfSplitMode: settings.mode,
        pdfPageImageFormat: settings.outputFormat,
        pdfPageImageResolution: settings.resolution,
      };
    case "compressPdf":
      return {
        ...session,
        pdfCompressionPreset: settings.preset,
        pdfCompressionGoal: settings.compressionGoal,
        pdfTargetSizeBytes: settings.targetSizeBytes ?? DEFAULT_PDF_TARGET_SIZE_BYTES,
      };
    case "createArchive":
      return { ...session, archiveFormat: settings.format };
  }
}

function terminalErrorState(state: OperationSession, error: ConversionError) {
  const status: QueueItemStatus = error.kind === "Cancelled" ? "cancelled" : "failed";
  const stage = status === "cancelled" ? "Cancelled" : "Failed";
  const queueItems = Object.fromEntries(
    Object.entries(state.queueItems).map(([path, item]) => [
      path,
      item.status === "running"
        ? { ...item, status, progress: 0, stage, result: null, error }
        : item,
    ]),
  );

  return {
    state: "error" as const,
    queueItems,
    progress: 0,
    progressStage: "",
    error,
    jobId: null,
    activePath: null,
    cancelBatchRequested: false,
  };
}

const snapshotSession = (state: AppStore): OperationSession => ({
  state: state.state,
  files: state.files,
  file: state.file,
  outputFormats: state.outputFormats,
  outputFormat: state.outputFormat,
  resizeWidth: state.resizeWidth,
  resizeHeight: state.resizeHeight,
  preserveAspect: state.preserveAspect,
  keepMetadata: state.keepMetadata,
  imageOptimizationGoal: state.imageOptimizationGoal,
  imageTargetSizeBytes: state.imageTargetSizeBytes,
  imageExportPresets: state.imageExportPresets,
  audioOutputFormat: state.audioOutputFormat,
  audioTracks: state.audioTracks,
  audioTrackIndexes: state.audioTrackIndexes,
  audioTrackErrors: state.audioTrackErrors,
  transcriptionModel: state.transcriptionModel,
  transcriptionLanguage: state.transcriptionLanguage,
  transcriptionOutputFormat: state.transcriptionOutputFormat,
  ocrOutputFormat: state.ocrOutputFormat,
  videoEncodingPreset: state.videoEncodingPreset,
  videoResolution: state.videoResolution,
  videoQuality: state.videoQuality,
  videoCompressionGoal: state.videoCompressionGoal,
  videoTargetSizeBytes: state.videoTargetSizeBytes,
  audioCompressionPreset: state.audioCompressionPreset,
  subtitleOutputFormat: state.subtitleOutputFormat,
  subtitleTracks: state.subtitleTracks,
  subtitleTrackIndexes: state.subtitleTrackIndexes,
  subtitleTrackErrors: state.subtitleTrackErrors,
  thumbnailMode: state.thumbnailMode,
  thumbnailOutputFormat: state.thumbnailOutputFormat,
  pdfSplitMode: state.pdfSplitMode,
  pdfPageSelection: state.pdfPageSelection,
  pdfPageImageFormat: state.pdfPageImageFormat,
  pdfPageImageResolution: state.pdfPageImageResolution,
  pdfCompressionPreset: state.pdfCompressionPreset,
  pdfCompressionGoal: state.pdfCompressionGoal,
  pdfTargetSizeBytes: state.pdfTargetSizeBytes,
  archiveFormat: state.archiveFormat,
  archivePassword: state.archivePassword,
  archivePasswordConfirmation: state.archivePasswordConfirmation,
  extractArchivePasswords: state.extractArchivePasswords,
  renameSettings: state.renameSettings,
  queueItems: state.queueItems,
  activePath: state.activePath,
  cancelBatchRequested: state.cancelBatchRequested,
  progress: state.progress,
  progressStage: state.progressStage,
  result: state.result,
  results: state.results,
  error: state.error,
  jobId: state.jobId,
});

function operationRequiresWholeQueue(operation: Operation) {
  return operation === "mergePdf" || operation === "createArchive" || operation === "rename";
}

function addFilesToSession(
  session: OperationSession,
  incoming: FileInfo[],
  resetExistingQueue: boolean,
) {
  if (incoming.length === 0) return { session, overflowCount: 0 };

  const knownPaths = new Set(session.files.map((file) => file.path));
  const unique = incoming.filter((file) => {
    if (knownPaths.has(file.path)) return false;
    knownPaths.add(file.path);
    return true;
  });
  const available = Math.max(0, MAX_QUEUE_ITEMS - session.files.length);
  const additions = unique.slice(0, available);
  const overflowCount = unique.length - additions.length;
  if (additions.length === 0) return { session, overflowCount };

  const files = [...session.files, ...additions];
  const file = files[0] ?? null;
  const outputFormats = { ...session.outputFormats };
  for (const item of additions) {
    outputFormats[item.path] = getCompatibleFormats(item.format)[0] ?? "";
  }
  const queueItems = Object.fromEntries(
    files.map((item) => [
      item.path,
      resetExistingQueue ? createQueueItem() : (session.queueItems[item.path] ?? createQueueItem()),
    ]),
  );

  return {
    session: {
      ...session,
      state: "loaded" as const,
      files,
      file,
      outputFormats,
      queueItems,
      outputFormat: file ? (outputFormats[file.path] ?? null) : null,
      progress: 0,
      progressStage: "",
      result: resetExistingQueue ? null : session.result,
      results: resetExistingQueue ? [] : session.results,
      error: null,
      jobId: null,
      activePath: null,
      cancelBatchRequested: false,
    },
    overflowCount,
  };
}

function recentJobSession(
  operation: Operation,
  files: FileInfo[],
  setup: RecentJobSetup,
): OperationSession {
  const stagedFiles = files.map((file) => ({
    ...file,
    relativePath: setup.inputs?.[file.path]?.relativePath ?? file.relativePath ?? null,
  }));
  let session = addFilesToSession(createEmptySession(), stagedFiles, true).session;
  session = sessionWithRecipeSettings(session, setup.settings);

  const outputFormats = { ...session.outputFormats };
  const audioTrackIndexes: Record<string, number> = {};
  const subtitleTrackIndexes: Record<string, number> = {};
  for (const file of stagedFiles) {
    const input = setup.inputs?.[file.path];
    if (
      operation === "convert" &&
      input?.outputFormat &&
      getCompatibleFormats(file.format).includes(input.outputFormat)
    ) {
      outputFormats[file.path] = input.outputFormat;
    } else if (operation === "extractAudio" && input?.trackIndex !== undefined) {
      audioTrackIndexes[file.path] = input.trackIndex;
    } else if (operation === "extractSubtitles" && input?.trackIndex !== undefined) {
      subtitleTrackIndexes[file.path] = input.trackIndex;
    }
  }

  const file = stagedFiles[0] ?? null;
  return {
    ...session,
    outputFormats,
    outputFormat: file ? (outputFormats[file.path] ?? null) : null,
    audioTrackIndexes,
    subtitleTrackIndexes,
    pdfPageSelection: setup.pageSelection ?? session.pdfPageSelection,
  };
}

function queueOverflowMessage(count: number) {
  return `Queue limit reached. ${count} file(s) skipped.`;
}

const initialState = {
  ...createEmptySession(),
  operation: "convert" as Operation,
  sessions: {
    convert: createEmptySession(),
    resize: createEmptySession(),
    optimize: createEmptySession(),
    exportImages: createEmptySession(),
    encodeVideo: createEmptySession(),
    compressAudio: createEmptySession(),
    removeAudio: createEmptySession(),
    extractSubtitles: createEmptySession(),
    generateThumbnails: createEmptySession(),
    removeMetadata: createEmptySession(),
    extractAudio: createEmptySession(),
    transcribe: createEmptySession(),
    extractText: createEmptySession(),
    recognizeText: createEmptySession(),
    mergePdf: createEmptySession(),
    splitPdf: createEmptySession(),
    exportPdfPages: createEmptySession(),
    compressPdf: createEmptySession(),
    createArchive: createEmptySession(),
    extractArchive: createEmptySession(),
    rename: createEmptySession(),
    inspect: createEmptySession(),
  },
  rejectionMessage: null,
  workspaceAdmission: initialWorkspaceAdmission(),
  ...loadOutputPreferences(),
};

export const useAppStore = create<AppStore>((set, get) => {
  const setWorkspace = (update: (state: AppStore) => Partial<AppStore> | AppStore) =>
    set((state) => (workspaceAdmissionIsPending(state.workspaceAdmission) ? state : update(state)));

  return {
    ...initialState,

    selectFile: (path) =>
      setWorkspace((state) => {
        const file = state.files.find((item) => item.path === path);
        if (!file) return state;
        return {
          file,
          outputFormat: state.outputFormats[file.path] ?? null,
        };
      }),

    addFiles: (incoming) =>
      set((state) => {
        if (workspaceAdmissionIsPending(state.workspaceAdmission)) return state;
        if (isInputIntakeBlocked(state.state)) {
          return { rejectionMessage: INPUT_INTAKE_BLOCKED_MESSAGE };
        }
        const original = snapshotSession(state);
        const merged = addFilesToSession(
          original,
          incoming,
          operationRequiresWholeQueue(state.operation),
        );
        if (merged.session === original && merged.overflowCount === 0) return state;
        return {
          ...merged.session,
          rejectionMessage:
            merged.overflowCount > 0
              ? queueOverflowMessage(merged.overflowCount)
              : state.rejectionMessage,
        };
      }),

    addFilesForOperation: (operation, incoming) =>
      set((state) => {
        if (!isActiveOperation(operation) || incoming.length === 0) return state;
        if (workspaceAdmissionIsPending(state.workspaceAdmission)) return state;

        const active = operation === state.operation;
        const original = active ? snapshotSession(state) : state.sessions[operation];
        if (isInputIntakeBlocked(original.state)) {
          return { rejectionMessage: INPUT_INTAKE_BLOCKED_MESSAGE };
        }
        const merged = addFilesToSession(
          original,
          incoming,
          operationRequiresWholeQueue(operation),
        );
        if (merged.session === original && merged.overflowCount === 0) return state;

        const rejectionMessage =
          merged.overflowCount > 0
            ? queueOverflowMessage(merged.overflowCount)
            : state.rejectionMessage;
        if (active) return { ...merged.session, rejectionMessage };

        return {
          sessions: { ...state.sessions, [operation]: merged.session },
          rejectionMessage,
        };
      }),

    removeFile: (path) =>
      setWorkspace((state) => {
        const files = state.files.filter((file) => file.path !== path);
        const file = files[0] ?? null;
        const outputFormats = { ...state.outputFormats };
        const audioTracks = { ...state.audioTracks };
        const audioTrackIndexes = { ...state.audioTrackIndexes };
        const audioTrackErrors = { ...state.audioTrackErrors };
        const subtitleTracks = { ...state.subtitleTracks };
        const subtitleTrackIndexes = { ...state.subtitleTrackIndexes };
        const subtitleTrackErrors = { ...state.subtitleTrackErrors };
        const extractArchivePasswords = { ...state.extractArchivePasswords };
        const queueItems = { ...state.queueItems };
        delete outputFormats[path];
        delete audioTracks[path];
        delete audioTrackIndexes[path];
        delete audioTrackErrors[path];
        delete subtitleTracks[path];
        delete subtitleTrackIndexes[path];
        delete subtitleTrackErrors[path];
        delete extractArchivePasswords[path];
        delete queueItems[path];

        if (!file) return createEmptySession();

        return {
          files,
          file,
          outputFormats,
          audioTracks,
          audioTrackIndexes,
          audioTrackErrors,
          subtitleTracks,
          subtitleTrackIndexes,
          subtitleTrackErrors,
          extractArchivePasswords,
          queueItems,
          outputFormat: outputFormats[file.path] ?? null,
          resizeWidth: state.file?.path === file.path ? state.resizeWidth : null,
          resizeHeight: state.file?.path === file.path ? state.resizeHeight : null,
        };
      }),

    moveFile: (path, direction) =>
      setWorkspace((state) => {
        const index = state.files.findIndex((file) => file.path === path);
        const target = index + direction;
        if (index < 0 || target < 0 || target >= state.files.length) return state;

        const files = [...state.files];
        [files[index], files[target]] = [files[target], files[index]];
        const file = files[0] ?? null;
        return {
          files,
          file,
          outputFormat: file ? (state.outputFormats[file.path] ?? null) : null,
        };
      }),

    setOperation: (operation) =>
      set((state) => {
        if (state.workspaceAdmission.phase !== "idle") return state;
        if (!isActiveOperation(operation) || operation === state.operation) return state;

        const sessions = {
          ...state.sessions,
          [state.operation]: snapshotSession(state),
        };

        return {
          ...sessions[operation],
          operation,
          sessions,
          rejectionMessage: null,
        };
      }),

    setOutputFormat: (format, path) =>
      setWorkspace((state) => {
        const targetPath = path ?? state.file?.path;
        if (!targetPath) return state;
        return {
          outputFormats: { ...state.outputFormats, [targetPath]: format },
          outputFormat: targetPath === state.file?.path ? format : state.outputFormat,
        };
      }),
    setCompatibleOutputFormats: (format) =>
      setWorkspace((state) => {
        const outputFormats = { ...state.outputFormats };
        let changed = false;
        for (const file of state.files) {
          if (!getCompatibleFormats(file.format).includes(format)) continue;
          if (outputFormats[file.path] === format) continue;
          outputFormats[file.path] = format;
          changed = true;
        }
        if (!changed) return state;
        return {
          outputFormats,
          outputFormat: state.file ? (outputFormats[state.file.path] ?? null) : null,
        };
      }),
    setResizeDimensions: (resizeWidth, resizeHeight) =>
      setWorkspace(() => ({ resizeWidth, resizeHeight })),
    setPreserveAspect: (preserveAspect) => setWorkspace(() => ({ preserveAspect })),
    setKeepMetadata: (keepMetadata) => setWorkspace(() => ({ keepMetadata })),
    setImageOptimizationGoal: (imageOptimizationGoal) =>
      setWorkspace(() => ({ imageOptimizationGoal })),
    setImageTargetSizeBytes: (imageTargetSizeBytes) =>
      setWorkspace(() => ({
        imageTargetSizeBytes: isImageTargetSizeBytes(imageTargetSizeBytes)
          ? imageTargetSizeBytes
          : null,
      })),
    setImageExportPresets: (presets) =>
      setWorkspace(() => ({
        imageExportPresets: (["web", "email", "social", "preview"] as ImageExportPreset[]).filter(
          (preset) => presets.includes(preset),
        ),
      })),
    setAudioOutputFormat: (audioOutputFormat) => setWorkspace(() => ({ audioOutputFormat })),
    setAudioTracks: (path, tracks, error) =>
      setWorkspace((state) => {
        if (!state.files.some((file) => file.path === path)) return state;
        const current = state.audioTrackIndexes[path];
        const selected =
          current === undefined
            ? (tracks.find((track) => track.isDefault)?.streamIndex ?? tracks[0]?.streamIndex)
            : tracks.some((track) => track.streamIndex === current)
              ? current
              : undefined;
        const audioTrackIndexes = { ...state.audioTrackIndexes };
        if (selected === undefined) delete audioTrackIndexes[path];
        else audioTrackIndexes[path] = selected;
        const audioTrackErrors = { ...state.audioTrackErrors };
        if (error) audioTrackErrors[path] = error;
        else delete audioTrackErrors[path];
        return {
          audioTracks: { ...state.audioTracks, [path]: tracks },
          audioTrackIndexes,
          audioTrackErrors,
        };
      }),
    setAudioTrackIndex: (path, streamIndex) =>
      setWorkspace((state) => {
        const track = state.audioTracks[path]?.find(
          (candidate) => candidate.streamIndex === streamIndex,
        );
        if (!track) return state;
        return {
          audioTrackIndexes: { ...state.audioTrackIndexes, [path]: streamIndex },
        };
      }),
    setTranscriptionModel: (transcriptionModel) => setWorkspace(() => ({ transcriptionModel })),
    setTranscriptionLanguage: (transcriptionLanguage) =>
      setWorkspace(() => ({ transcriptionLanguage })),
    setTranscriptionOutputFormat: (transcriptionOutputFormat) =>
      setWorkspace(() => ({ transcriptionOutputFormat })),
    setOcrOutputFormat: (ocrOutputFormat) => setWorkspace(() => ({ ocrOutputFormat })),
    setVideoEncodingPreset: (videoEncodingPreset) =>
      setWorkspace((state) => ({
        videoEncodingPreset,
        videoCompressionGoal: effectiveVideoCompressionGoal(
          videoEncodingPreset,
          state.videoCompressionGoal,
        ),
      })),
    setVideoResolution: (videoResolution) => setWorkspace(() => ({ videoResolution })),
    setVideoQuality: (videoQuality) => setWorkspace(() => ({ videoQuality })),
    setVideoCompressionGoal: (videoCompressionGoal) =>
      setWorkspace((state) => {
        const next = effectiveVideoCompressionGoal(state.videoEncodingPreset, videoCompressionGoal);
        return next === state.videoCompressionGoal ? state : { videoCompressionGoal: next };
      }),
    setVideoTargetSizeBytes: (videoTargetSizeBytes) =>
      setWorkspace((state) =>
        videoTargetSizeBytes === null || isVideoTargetSizeBytes(videoTargetSizeBytes)
          ? { videoTargetSizeBytes }
          : state,
      ),
    setAudioCompressionPreset: (audioCompressionPreset) =>
      setWorkspace(() => ({ audioCompressionPreset })),
    setSubtitleOutputFormat: (subtitleOutputFormat) =>
      setWorkspace(() => ({ subtitleOutputFormat })),
    setSubtitleTracks: (path, tracks, error) =>
      setWorkspace((state) => {
        if (!state.files.some((file) => file.path === path)) return state;
        const supported = tracks.filter((track) => track.supported);
        const current = state.subtitleTrackIndexes[path];
        const selected =
          current === undefined
            ? (supported.find((track) => track.isDefault)?.streamIndex ?? supported[0]?.streamIndex)
            : supported.some((track) => track.streamIndex === current)
              ? current
              : undefined;
        const subtitleTrackIndexes = { ...state.subtitleTrackIndexes };
        if (selected === undefined) delete subtitleTrackIndexes[path];
        else subtitleTrackIndexes[path] = selected;
        const subtitleTrackErrors = { ...state.subtitleTrackErrors };
        if (error) subtitleTrackErrors[path] = error;
        else delete subtitleTrackErrors[path];
        return {
          subtitleTracks: { ...state.subtitleTracks, [path]: tracks },
          subtitleTrackIndexes,
          subtitleTrackErrors,
        };
      }),
    setSubtitleTrackIndex: (path, streamIndex) =>
      setWorkspace((state) => {
        const track = state.subtitleTracks[path]?.find(
          (candidate) => candidate.streamIndex === streamIndex && candidate.supported,
        );
        if (!track) return state;
        return {
          subtitleTrackIndexes: { ...state.subtitleTrackIndexes, [path]: streamIndex },
        };
      }),
    setThumbnailMode: (thumbnailMode) => setWorkspace(() => ({ thumbnailMode })),
    setThumbnailOutputFormat: (thumbnailOutputFormat) =>
      setWorkspace(() => ({ thumbnailOutputFormat })),
    setPdfSplitMode: (pdfSplitMode) => setWorkspace(() => ({ pdfSplitMode })),
    setPdfPageSelection: (pdfPageSelection) => setWorkspace(() => ({ pdfPageSelection })),
    setPdfPageImageFormat: (pdfPageImageFormat) => setWorkspace(() => ({ pdfPageImageFormat })),
    setPdfPageImageResolution: (pdfPageImageResolution) =>
      setWorkspace(() => ({ pdfPageImageResolution })),
    setPdfCompressionPreset: (pdfCompressionPreset) =>
      setWorkspace(() => ({ pdfCompressionPreset })),
    setPdfCompressionGoal: (pdfCompressionGoal) => setWorkspace(() => ({ pdfCompressionGoal })),
    setPdfTargetSizeBytes: (pdfTargetSizeBytes) =>
      setWorkspace((state) =>
        pdfTargetSizeBytes === null || isPdfTargetSizeBytes(pdfTargetSizeBytes)
          ? { pdfTargetSizeBytes }
          : state,
      ),
    setArchiveFormat: (archiveFormat) => setWorkspace(() => ({ archiveFormat })),
    setArchivePassword: (archivePassword) => setWorkspace(() => ({ archivePassword })),
    setArchivePasswordConfirmation: (archivePasswordConfirmation) =>
      setWorkspace(() => ({ archivePasswordConfirmation })),
    setExtractArchivePassword: (path, password) =>
      setWorkspace((state) => {
        if (!state.files.some((file) => file.path === path)) return state;
        const extractArchivePasswords = { ...state.extractArchivePasswords };
        if (password) extractArchivePasswords[path] = password;
        else delete extractArchivePasswords[path];
        return { extractArchivePasswords };
      }),
    setRenameSettings: (settings) =>
      setWorkspace((state) => ({ renameSettings: { ...state.renameSettings, ...settings } })),
    applyRecipeSettings: (settings) =>
      setWorkspace((state) => sessionWithRecipeSettings(snapshotSession(state), settings)),
    setOutputDirectory: (outputDirectory) =>
      setWorkspace((state) => {
        const preferences = {
          outputDirectory,
          outputSuffixes: state.outputSuffixes,
        };
        saveOutputPreferences(preferences);
        return { outputDirectory };
      }),
    setOutputSuffix: (operation, suffix) =>
      setWorkspace((state) => {
        const outputSuffixes = { ...state.outputSuffixes, [operation]: suffix };
        saveOutputPreferences({
          outputDirectory: state.outputDirectory,
          outputSuffixes,
        });
        return { outputSuffixes };
      }),

    beginWorkspaceRestore: () => {
      const admission = get().workspaceAdmission;
      if (admission.phase === "restoring") return admission.token;
      if (admission.phase !== "idle") return null;
      const token = crypto.randomUUID();
      set({ workspaceAdmission: { phase: "restoring", token, operation: null } });
      return token;
    },

    completeWorkspaceRestore: (token) => {
      const admission = get().workspaceAdmission;
      if (admission.phase !== "restoring" || admission.token !== token) return false;
      set({ workspaceAdmission: IDLE_WORKSPACE_ADMISSION });
      return true;
    },

    claimRecentJobLoad: (operation, token) => {
      const state = get();
      const target =
        operation === state.operation ? snapshotSession(state) : state.sessions[operation];
      if (
        !isActiveOperation(operation) ||
        state.workspaceAdmission.phase !== "idle" ||
        state.state === "converting" ||
        target.state === "converting"
      ) {
        return false;
      }
      set({ workspaceAdmission: { phase: "restoring", token, operation } });
      return true;
    },

    commitRecentJobLoad: (operation, token, files, setup) => {
      const state = get();
      if (
        !isActiveOperation(operation) ||
        files.length === 0 ||
        files.length > MAX_QUEUE_ITEMS ||
        state.workspaceAdmission.phase !== "restoring" ||
        state.workspaceAdmission.token !== token ||
        state.workspaceAdmission.operation !== operation
      ) {
        return false;
      }

      const session = recentJobSession(operation, files, setup);
      const sessions = {
        ...state.sessions,
        [state.operation]: snapshotSession(state),
        [operation]: session,
      };
      const outputSuffixes = { ...state.outputSuffixes, [operation]: setup.suffix };
      saveOutputPreferences({ outputDirectory: setup.directory, outputSuffixes });
      set({
        ...session,
        operation,
        sessions,
        outputDirectory: setup.directory,
        outputSuffixes,
        rejectionMessage: null,
        workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
      });
      return true;
    },

    claimWorkspacePreflight: (operation, token) => {
      const state = get();
      if (
        state.workspaceAdmission.phase !== "idle" ||
        state.state === "converting" ||
        state.operation !== operation
      ) {
        return false;
      }
      set({ workspaceAdmission: { phase: "preflighting", token, operation } });
      return true;
    },

    releaseWorkspaceClaim: (token) => {
      const admission = get().workspaceAdmission;
      if (admission.token !== token || admission.phase === "idle") return false;
      set({ workspaceAdmission: IDLE_WORKSPACE_ADMISSION });
      return true;
    },

    startClaimedBatch: (operation, token, paths) => {
      const state = get();
      if (
        state.workspaceAdmission.phase !== "preflighting" ||
        state.workspaceAdmission.token !== token ||
        state.workspaceAdmission.operation !== operation ||
        state.operation !== operation
      ) {
        return false;
      }
      const queueItems = { ...state.queueItems };
      for (const path of paths) queueItems[path] = createQueueItem();
      set({
        state: "converting",
        queueItems,
        activePath: null,
        cancelBatchRequested: false,
        progress: -1,
        progressStage: "Starting...",
        result: null,
        results: [],
        error: null,
        jobId: null,
        workspaceAdmission: { phase: "running", token, operation },
      });
      return true;
    },

    startBatch: (paths) =>
      set((state) => {
        if (workspaceAdmissionIsPending(state.workspaceAdmission)) return state;
        const queueItems = { ...state.queueItems };
        for (const path of paths) queueItems[path] = createQueueItem();
        return {
          state: "converting",
          queueItems,
          activePath: null,
          cancelBatchRequested: false,
          progress: -1,
          progressStage: "Starting...",
          result: null,
          results: [],
          error: null,
          jobId: null,
          workspaceAdmission: {
            phase: "running",
            token: crypto.randomUUID(),
            operation: state.operation,
          },
        };
      }),

    startQueueItem: (path, jobId) =>
      set((state) => ({
        activePath: path,
        jobId,
        queueItems: {
          ...state.queueItems,
          [path]: {
            ...createQueueItem(),
            status: "running",
            progress: -1,
            stage: "Starting",
          },
        },
      })),

    startGroupJob: (paths, jobId) =>
      set((state) => {
        const queueItems = { ...state.queueItems };
        const stage =
          state.operation === "createArchive"
            ? `Creating ${archiveFormatLabel(state.archiveFormat)}`
            : state.operation === "rename"
              ? "Renaming files"
              : "Combining files";
        for (const path of paths) {
          queueItems[path] = {
            ...createQueueItem(),
            status: "running",
            progress: -1,
            stage,
          };
        }
        return {
          activePath: paths[0] ?? null,
          jobId,
          queueItems,
        };
      }),

    completeQueueItem: (path, result) =>
      set((state) => ({
        queueItems: {
          ...state.queueItems,
          [path]: {
            status: "completed",
            progress: 100,
            stage:
              state.operation === "optimize" && result.output_path === path
                ? "Already optimized"
                : "Complete",
            result,
            error: null,
          },
        },
        activePath: state.activePath === path ? null : state.activePath,
        jobId: state.activePath === path ? null : state.jobId,
      })),

    completeGroupJob: (paths, result) =>
      set((state) => {
        const queueItems = { ...state.queueItems };
        for (const path of paths) {
          queueItems[path] = {
            status: "completed",
            progress: 100,
            stage: state.operation === "rename" ? "Renamed" : "Included",
            result,
            error: null,
          };
        }
        return { queueItems, activePath: null, jobId: null };
      }),

    applyRenamedPaths: (originalPaths, renamedPaths) =>
      set((state) => {
        if (originalPaths.length !== renamedPaths.length) return state;
        const renamedByPath = new Map(
          originalPaths.map((path, index) => [path, renamedPaths[index]] as const),
        );
        const outputFormats = { ...state.outputFormats };
        const queueItems = { ...state.queueItems };
        const files = state.files.map((item) => {
          const renamedPath = renamedByPath.get(item.path);
          if (!renamedPath) return item;

          const previousQueueItem = queueItems[item.path];
          const previousFormat = outputFormats[item.path];
          delete queueItems[item.path];
          delete outputFormats[item.path];
          if (previousQueueItem) {
            queueItems[renamedPath] = {
              ...previousQueueItem,
              result: previousQueueItem.result
                ? {
                    ...previousQueueItem.result,
                    output_path: renamedPath,
                    output_paths: [renamedPath],
                  }
                : null,
            };
          }
          if (previousFormat !== undefined) outputFormats[renamedPath] = previousFormat;

          const segments = renamedPath.replaceAll("\\", "/").split("/");
          const name = segments[segments.length - 1] || item.name;
          const relativePath = item.relativePath
            ? [...item.relativePath.replaceAll("\\", "/").split("/").slice(0, -1), name].join("/")
            : item.relativePath;
          return { ...item, path: renamedPath, name, relativePath };
        });
        const selectedPath = state.file ? renamedByPath.get(state.file.path) : null;
        const file = selectedPath
          ? (files.find((item) => item.path === selectedPath) ?? files[0] ?? null)
          : (files[0] ?? null);
        return { files, file, outputFormats, queueItems };
      }),

    failQueueItem: (path, error, status = "failed") =>
      set((state) => ({
        queueItems: {
          ...state.queueItems,
          [path]: {
            status,
            progress: 0,
            stage: status === "cancelled" ? "Cancelled" : "Failed",
            result: null,
            error,
          },
        },
        activePath: state.activePath === path ? null : state.activePath,
        jobId: state.activePath === path ? null : state.jobId,
      })),

    failGroupJob: (paths, error, status = "failed") =>
      set((state) => {
        const queueItems = { ...state.queueItems };
        for (const path of paths) {
          queueItems[path] = {
            status,
            progress: 0,
            stage: status === "cancelled" ? "Cancelled" : "Failed",
            result: null,
            error,
          };
        }
        return { queueItems, activePath: null, jobId: null };
      }),

    skipQueueItem: (path) =>
      set((state) => {
        const item = state.queueItems[path];
        if (!item || item.status !== "pending") return state;
        return {
          queueItems: {
            ...state.queueItems,
            [path]: { ...item, status: "skipped", stage: "Skipped" },
          },
        };
      }),

    requestBatchCancel: () => set({ cancelBatchRequested: true }),

    cancelRemainingItems: (paths) =>
      set((state) => {
        const queueItems = { ...state.queueItems };
        for (const path of paths) {
          const item = queueItems[path];
          if (item?.status === "pending") {
            queueItems[path] = {
              ...item,
              status: "cancelled",
              stage: "Cancelled",
              error: { kind: "Cancelled", detail: {} },
            };
          }
        }
        return { queueItems };
      }),

    finishBatch: () =>
      set((state) => {
        const results: ConversionResult[] = [];
        const outputPaths = new Set<string>();
        for (const file of state.files) {
          const result = state.queueItems[file.path]?.result;
          if (result && !outputPaths.has(result.output_path)) {
            outputPaths.add(result.output_path);
            results.push(result);
          }
        }
        return {
          state: "done",
          progress: 100,
          progressStage: "Complete",
          result: results[results.length - 1] ?? null,
          results,
          activePath: null,
          jobId: null,
          cancelBatchRequested: false,
          workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
        };
      }),

    startConversion: (jobId) =>
      set((state) =>
        workspaceAdmissionIsPending(state.workspaceAdmission)
          ? state
          : {
              state: "converting",
              progress: -1,
              progressStage: "Starting...",
              result: null,
              error: null,
              jobId,
              workspaceAdmission: {
                phase: "running",
                token: jobId,
                operation: state.operation,
              },
            },
      ),

    setActiveJob: (jobId) => set({ jobId }),

    updateProgress: (jobId, percent, stage) =>
      set((state) => {
        if (state.state !== "converting" || state.jobId !== jobId) return state;
        if (!state.activePath) return { progress: percent, progressStage: stage };
        if (
          state.operation === "mergePdf" ||
          state.operation === "createArchive" ||
          state.operation === "rename"
        ) {
          const queueItems = { ...state.queueItems };
          for (const [path, item] of Object.entries(queueItems)) {
            if (item.status === "running") {
              queueItems[path] = { ...item, progress: percent, stage };
            }
          }
          return { progress: percent, progressStage: stage, queueItems };
        }
        const item = state.queueItems[state.activePath];
        return {
          progress: percent,
          progressStage: stage,
          queueItems: {
            ...state.queueItems,
            [state.activePath]: {
              ...item,
              progress: percent,
              stage,
            },
          },
        };
      }),

    setResult: (result) =>
      set({
        state: "done",
        progress: 100,
        progressStage: "Complete",
        result,
        results: [result],
        error: null,
        jobId: null,
        activePath: null,
        cancelBatchRequested: false,
        workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
      }),

    setResults: (results) =>
      set({
        state: "done",
        progress: 100,
        progressStage: "Complete",
        result: results[results.length - 1] ?? null,
        results,
        error: null,
        jobId: null,
        activePath: null,
        cancelBatchRequested: false,
        workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
      }),

    setJobError: (jobId, error) =>
      set((state) =>
        state.state === "converting" && state.jobId === jobId
          ? { ...terminalErrorState(state, error), workspaceAdmission: IDLE_WORKSPACE_ADMISSION }
          : state,
      ),

    setError: (error) =>
      set((state) => ({
        ...terminalErrorState(state, error),
        workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
      })),

    setRejection: (message) => set({ rejectionMessage: message }),
    clearRejection: () => set({ rejectionMessage: null }),

    reset: () =>
      setWorkspace((state) => {
        const session = createEmptySession();
        return {
          ...session,
          sessions: { ...state.sessions, [state.operation]: session },
          rejectionMessage: null,
          workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
        };
      }),
  };
});

function recordForPaths<T>(files: FileInfo[], values: Record<string, T>): Record<string, T> {
  return Object.fromEntries(
    files.flatMap((file) =>
      values[file.path] === undefined ? [] : [[file.path, values[file.path]] as const],
    ),
  );
}

function sessionDraft(session: OperationSession): WorkspaceSessionDraft {
  return {
    paths: session.files.map((file) => file.path),
    outputFormats: recordForPaths(session.files, session.outputFormats),
    resizeWidth: session.resizeWidth,
    resizeHeight: session.resizeHeight,
    preserveAspect: session.preserveAspect,
    keepMetadata: session.keepMetadata,
    imageOptimizationGoal: session.imageOptimizationGoal,
    imageTargetSizeBytes: session.imageTargetSizeBytes,
    imageExportPresets: session.imageExportPresets,
    audioOutputFormat: session.audioOutputFormat,
    audioTrackIndexes: recordForPaths(session.files, session.audioTrackIndexes),
    transcriptionModel: session.transcriptionModel,
    transcriptionLanguage: session.transcriptionLanguage,
    transcriptionOutputFormat: session.transcriptionOutputFormat,
    ocrOutputFormat: session.ocrOutputFormat,
    videoEncodingPreset: session.videoEncodingPreset,
    videoResolution: session.videoResolution,
    videoQuality: session.videoQuality,
    videoCompressionGoal: session.videoCompressionGoal,
    videoTargetSizeBytes: session.videoTargetSizeBytes,
    audioCompressionPreset: session.audioCompressionPreset,
    subtitleOutputFormat: session.subtitleOutputFormat,
    subtitleTrackIndexes: recordForPaths(session.files, session.subtitleTrackIndexes),
    thumbnailMode: session.thumbnailMode,
    thumbnailOutputFormat: session.thumbnailOutputFormat,
    pdfSplitMode: session.pdfSplitMode,
    pdfPageSelection: session.pdfPageSelection,
    pdfPageImageFormat: session.pdfPageImageFormat,
    pdfPageImageResolution: session.pdfPageImageResolution,
    pdfCompressionPreset: session.pdfCompressionPreset,
    pdfCompressionGoal: session.pdfCompressionGoal,
    pdfTargetSizeBytes: session.pdfTargetSizeBytes,
    archiveFormat: session.archiveFormat,
    renameSettings: session.renameSettings,
  };
}

export function currentWorkspaceDraft(): WorkspaceDraft {
  const state = useAppStore.getState();
  const sessions = {
    ...state.sessions,
    [state.operation]: snapshotSession(state),
  };
  return {
    version: WORKSPACE_DRAFT_VERSION,
    activeOperation: isActiveOperation(state.operation) ? state.operation : "convert",
    sessions: Object.fromEntries(
      ACTIVE_OPERATION_IDS.map((operation) => [operation, sessionDraft(sessions[operation])]),
    ),
  };
}

function restoredSession(
  draft: WorkspaceSessionDraft | undefined,
  files: FileInfo[],
): OperationSession {
  const session = createEmptySession();
  if (!draft) return session;

  const outputFormats = Object.fromEntries(
    files.map((file) => {
      const compatible = getCompatibleFormats(file.format);
      const stored = draft.outputFormats[file.path];
      return [file.path, stored && compatible.includes(stored) ? stored : (compatible[0] ?? "")];
    }),
  );
  const file = files[0] ?? null;
  return {
    ...session,
    state: files.length > 0 ? "loaded" : "empty",
    files,
    file,
    outputFormats,
    outputFormat: file ? (outputFormats[file.path] ?? null) : null,
    resizeWidth: draft.resizeWidth,
    resizeHeight: draft.resizeHeight,
    preserveAspect: draft.preserveAspect,
    keepMetadata: draft.keepMetadata,
    imageOptimizationGoal: draft.imageOptimizationGoal,
    imageTargetSizeBytes: draft.imageTargetSizeBytes,
    imageExportPresets: draft.imageExportPresets,
    audioOutputFormat: draft.audioOutputFormat,
    audioTrackIndexes: recordForPaths(files, draft.audioTrackIndexes),
    transcriptionModel: draft.transcriptionModel,
    transcriptionLanguage: draft.transcriptionLanguage,
    transcriptionOutputFormat: draft.transcriptionOutputFormat,
    ocrOutputFormat: draft.ocrOutputFormat,
    videoEncodingPreset: draft.videoEncodingPreset,
    videoResolution: draft.videoResolution,
    videoQuality: draft.videoQuality,
    videoCompressionGoal: draft.videoCompressionGoal,
    videoTargetSizeBytes: draft.videoTargetSizeBytes,
    audioCompressionPreset: draft.audioCompressionPreset,
    subtitleOutputFormat: draft.subtitleOutputFormat,
    subtitleTrackIndexes: recordForPaths(files, draft.subtitleTrackIndexes),
    thumbnailMode: draft.thumbnailMode,
    thumbnailOutputFormat: draft.thumbnailOutputFormat,
    pdfSplitMode: draft.pdfSplitMode,
    pdfPageSelection: draft.pdfPageSelection,
    pdfPageImageFormat: draft.pdfPageImageFormat,
    pdfPageImageResolution: draft.pdfPageImageResolution,
    pdfCompressionPreset: draft.pdfCompressionPreset,
    pdfCompressionGoal: draft.pdfCompressionGoal,
    pdfTargetSizeBytes: draft.pdfTargetSizeBytes,
    archiveFormat: draft.archiveFormat,
    renameSettings: draft.renameSettings,
    queueItems: Object.fromEntries(files.map((item) => [item.path, createQueueItem()])),
  };
}

export function restoreWorkspaceDraft(
  draft: WorkspaceDraft,
  filesByOperation: Partial<Record<Operation, FileInfo[]>>,
  restoreToken?: string,
): boolean {
  if (restoreToken) {
    const admission = useAppStore.getState().workspaceAdmission;
    if (admission.phase !== "restoring" || admission.token !== restoreToken) return false;
  }
  const sessions = Object.fromEntries(
    OPERATION_IDS.map((operation) => [
      operation,
      restoredSession(draft.sessions[operation], filesByOperation[operation] ?? []),
    ]),
  ) as Record<Operation, OperationSession>;
  const activeOperation = isActiveOperation(draft.activeOperation)
    ? draft.activeOperation
    : "convert";
  const active = sessions[activeOperation];
  useAppStore.setState({
    ...active,
    operation: activeOperation,
    sessions,
    rejectionMessage: null,
    workspaceAdmission: IDLE_WORKSPACE_ADMISSION,
  });
  return true;
}
