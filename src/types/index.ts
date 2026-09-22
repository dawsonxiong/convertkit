export type FileCategory = "image" | "video" | "audio" | "document" | "vector" | "other";

export interface FileInfo {
  path: string;
  name: string;
  extension: string;
  size: number;
  category: FileCategory;
  format: string;
  width: number | null;
  height: number | null;
  relativePath?: string | null;
  mimeType: string | null;
  createdAt: number | null;
  modifiedAt: number | null;
  readOnly: boolean;
}

export interface FileHashes {
  md5: string;
  sha1: string;
  sha256: string;
}

export interface InspectionProgress {
  jobId: string;
  percent: number;
}

export interface ChecksumManifestInput {
  inputPath: string;
  relativePath: string | null;
}

export interface ChecksumManifestCreationResult {
  outputPath: string;
  entryCount: number;
}

type ChecksumManifestAlgorithm = "md5" | "sha1" | "sha256";
type ChecksumManifestStatus = "match" | "mismatch" | "missing";

export interface ChecksumManifestVerificationEntry {
  relativePath: string;
  expectedDigest: string;
  actualDigest: string | null;
  status: ChecksumManifestStatus;
}

export interface ChecksumManifestVerificationResult {
  manifestPath: string;
  algorithm: ChecksumManifestAlgorithm;
  entries: ChecksumManifestVerificationEntry[];
}

export interface TechnicalMetadata {
  pageCount: number | null;
  pageSize: string | null;
  container: string | null;
  durationSeconds: number | null;
  bitRate: number | null;
  videoCodec: string | null;
  audioCodec: string | null;
  width: number | null;
  height: number | null;
  frameRate: number | null;
  audioSampleRate: number | null;
  audioChannels: number | null;
  audioTracks: AudioTrack[];
  subtitleTracks: SubtitleTrack[];
}

export interface AudioTrack {
  streamIndex: number;
  codec: string;
  language: string | null;
  title: string | null;
  channels: number | null;
  channelLayout: string | null;
  sampleRate: number | null;
  isDefault: boolean;
}

export interface SubtitleTrack {
  streamIndex: number;
  codec: string;
  language: string | null;
  title: string | null;
  isDefault: boolean;
  isForced: boolean;
  supported: boolean;
}

export interface InspectionReportEntry {
  file: FileInfo;
  technicalMetadata: TechnicalMetadata | null;
  checksums: FileHashes | null;
}

export interface InputCollectionResult {
  files: FileInfo[];
  skippedCount: number;
  truncated: boolean;
}

type CapabilityIssue = "missingDependency" | "unsupported" | "missingInput" | "invalidSettings";

export interface JobCapability {
  inputPath: string;
  available: boolean;
  engine: string | null;
  missingTools: string[];
  issue: CapabilityIssue | null;
  message: string | null;
}

export interface ConversionResult {
  output_path: string;
  output_paths: string[];
  output_size: number;
  duration_ms: number;
  undo_manifest?: string | null;
}

export interface ProgressPayload {
  jobId: string;
  percent: number;
  stage: string;
}

export type TranscriptionModel = "tiny" | "base" | "small";
export type TranscriptionOutputFormat = "txt" | "srt" | "vtt";

export interface TranscriptionModelStatus {
  model: TranscriptionModel;
  label: string;
  expectedSize: number;
  localSize: number;
  downloaded: boolean;
}

export interface ModelDownloadProgress {
  model: TranscriptionModel;
  percent: number;
  downloadedBytes: number;
  totalBytes: number;
}

export interface ConversionError {
  kind:
    | "MissingDependency"
    | "UnsupportedConversion"
    | "InputNotFound"
    | "ProcessFailed"
    | "Cancelled"
    | "Timeout"
    | "OutputMissing"
    | "OutputConflict"
    | "ArchivePasswordRequired"
    | "IncorrectArchivePassword"
    | "TargetSizeUnreachable"
    | "OutputNotSmaller"
    | "DiskFull";
  detail: Record<string, string>;
}

export type QueueItemStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "skipped"
  | "cancelled";

export interface QueueItemState {
  status: QueueItemStatus;
  progress: number;
  stage: string;
  result: ConversionResult | null;
  error: ConversionError | null;
}

export type AppState = "empty" | "loaded" | "converting" | "done" | "error";

export type Operation =
  | "convert"
  | "resize"
  | "optimize"
  | "exportImages"
  | "encodeVideo"
  | "compressAudio"
  | "removeAudio"
  | "extractSubtitles"
  | "generateThumbnails"
  | "removeMetadata"
  | "extractAudio"
  | "transcribe"
  | "extractText"
  | "recognizeText"
  | "mergePdf"
  | "splitPdf"
  | "exportPdfPages"
  | "compressPdf"
  | "createArchive"
  | "extractArchive"
  | "rename"
  | "inspect";

export type PdfSplitMode = "everyPage" | "extract";
export type PdfPageImageFormat = "png" | "jpeg";
export type PdfPageImageResolution = "screen" | "print";
export type PdfCompressionPreset = "high" | "balanced" | "smallest";
export type PdfCompressionGoal = "quality" | "fileSize";
export type ImageOptimizationGoal = "quality" | "fileSize";
export type ArchiveFormat = "zip" | "tar" | "tarGz" | "sevenZ" | "gzip";
export type AudioOutputFormat = "mp3" | "m4a" | "wav" | "flac";
export type VideoEncodingPreset = "compatible" | "smaller" | "web" | "archive";
export type VideoResolution = "automatic" | "original" | "fullHd" | "hd";
export type VideoQuality = "high" | "balanced" | "smallest";
export type AudioCompressionPreset = "high" | "balanced" | "smallest";
export type VideoCompressionGoal = "quality" | "fileSize";
export type OcrOutputFormat = "text" | "searchablePdf";
export type SubtitleOutputFormat = "srt" | "vtt";
export type ThumbnailMode = "frame";
export type ThumbnailOutputFormat = "jpeg" | "png";
export type ImageExportPreset = "web" | "email" | "social" | "preview";
export type RenameNumbering = "none" | "prefix" | "suffix";

export interface RenameSettings {
  find: string;
  replace: string;
  prefix: string;
  suffix: string;
  numbering: RenameNumbering;
  start: number;
  padding: number;
}

export interface OutputOptions {
  directory: string | null;
  suffix: string;
}

export interface ArchiveEntryRequest {
  inputPath: string;
  archivePath: string;
  folderDerived: boolean;
}

export interface RenameItemRequest {
  inputPath: string;
  outputName: string;
}

interface BaseJobRequest {
  inputPath: string;
  jobId: string | null;
  outputOptions: OutputOptions;
}

export type JobRequest =
  | (BaseJobRequest & {
      operation: "convert";
      outputFormat: string;
    })
  | (BaseJobRequest & {
      operation: "resize";
      width: number;
      height: number;
      preserveAspect: boolean;
    })
  | (BaseJobRequest & {
      operation: "optimize";
      keepMetadata: boolean;
      compressionGoal: ImageOptimizationGoal;
      targetSizeBytes: number | null;
    })
  | (BaseJobRequest & {
      operation: "exportImages";
      presets: ImageExportPreset[];
    })
  | (BaseJobRequest & {
      operation: "removeMetadata";
    })
  | (BaseJobRequest & {
      operation: "extractAudio";
      streamIndex: number;
      streamCodec: string;
      outputFormat: AudioOutputFormat;
    })
  | (BaseJobRequest & {
      operation: "transcribe";
      model: TranscriptionModel;
      language: string;
      outputFormat: TranscriptionOutputFormat;
    })
  | (BaseJobRequest & {
      operation: "encodeVideo";
      preset: VideoEncodingPreset;
      resolution: VideoResolution;
      quality: VideoQuality;
      compressionGoal: VideoCompressionGoal;
      targetSizeBytes: number | null;
    })
  | (BaseJobRequest & {
      operation: "compressAudio";
      preset: AudioCompressionPreset;
    })
  | (BaseJobRequest & {
      operation: "removeAudio";
    })
  | (BaseJobRequest & {
      operation: "extractSubtitles";
      streamIndex: number;
      streamCodec: string;
      outputFormat: SubtitleOutputFormat;
    })
  | (BaseJobRequest & {
      operation: "generateThumbnails";
      mode: ThumbnailMode;
      outputFormat: ThumbnailOutputFormat;
    })
  | (BaseJobRequest & {
      operation: "extractText";
    })
  | (BaseJobRequest & {
      operation: "recognizeText";
      outputFormat: OcrOutputFormat;
    })
  | {
      operation: "mergePdf";
      inputPaths: string[];
      jobId: string | null;
      outputOptions: OutputOptions;
    }
  | (BaseJobRequest & {
      operation: "splitPdf";
      mode: PdfSplitMode;
      pageSelection: string;
    })
  | (BaseJobRequest & {
      operation: "exportPdfPages";
      mode: PdfSplitMode;
      pageSelection: string;
      outputFormat: PdfPageImageFormat;
      resolution: PdfPageImageResolution;
    })
  | (BaseJobRequest & {
      operation: "compressPdf";
      preset: PdfCompressionPreset;
      compressionGoal: PdfCompressionGoal;
      targetSizeBytes: number | null;
    })
  | {
      operation: "createArchive";
      entries: ArchiveEntryRequest[];
      format: ArchiveFormat;
      password: string | null;
      jobId: string | null;
      outputOptions: OutputOptions;
    }
  | (BaseJobRequest & {
      operation: "extractArchive";
      password: string | null;
    })
  | {
      operation: "rename";
      items: RenameItemRequest[];
      jobId: string | null;
    };

export interface RecentJobItem {
  inputPath: string;
  inputName: string;
  outputPath?: string;
  outputPaths?: string[];
  status?: Exclude<QueueItemStatus, "pending" | "running">;
  errorKind?: ConversionError["kind"];
}

export interface RecentJobInputSetup {
  outputFormat?: string;
  trackIndex?: number;
  relativePath?: string;
}

export interface RecentJobSetup {
  version: 1;
  directory: string | null;
  suffix: string;
  settings?: import("../lib/savedRecipes").SavedRecipeSettings;
  inputs?: Record<string, RecentJobInputSetup>;
  pageSelection?: string;
}

export interface RecentJob {
  id: string;
  operation: Operation;
  completedAt: number;
  items: RecentJobItem[];
  setup?: RecentJobSetup;
  undoManifest?: string | null;
}
