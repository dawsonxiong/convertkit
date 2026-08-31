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
import { isActiveOperation, isRetiredOperation, JOB_OPERATIONS } from "./operations.ts";
import { TRANSCRIPTION_LANGUAGE_SET } from "./transcription.ts";
import { ARCHIVE_FORMATS } from "./archive.ts";
import {
  effectiveVideoCompressionGoal,
  isVideoTargetSizeBytes,
  videoTargetSizeForRequest,
} from "./videoCompression.ts";
import { isPdfTargetSizeBytes, pdfTargetSizeForRequest } from "./pdfCompression.ts";
import { isImageTargetSizeBytes, imageTargetSizeForRequest } from "./imageOptimization.ts";

export const MAX_SAVED_RECIPES = 24;
export const MAX_SAVED_RECIPE_NAME_LENGTH = 40;

export const SAVED_RECIPE_OPERATIONS = JOB_OPERATIONS.filter(
  (operation) => operation !== "rename" && isActiveOperation(operation),
);

const RECIPE_OPERATIONS = new Set<Operation>(SAVED_RECIPE_OPERATIONS);

const IMAGE_EXPORT_PRESETS = new Set<ImageExportPreset>(["web", "email", "social", "preview"]);
const IMAGE_OPTIMIZATION_GOALS = new Set<ImageOptimizationGoal>(["quality", "fileSize"]);
const AUDIO_FORMATS = new Set<AudioOutputFormat>(["mp3", "m4a", "wav", "flac"]);
const TRANSCRIPTION_MODELS = new Set<TranscriptionModel>(["tiny", "base", "small"]);
const TRANSCRIPTION_FORMATS = new Set<TranscriptionOutputFormat>(["txt", "srt", "vtt"]);
const OCR_OUTPUT_FORMATS = new Set<OcrOutputFormat>(["text", "searchablePdf"]);
const VIDEO_PRESETS = new Set<VideoEncodingPreset>(["compatible", "smaller", "web", "archive"]);
const VIDEO_RESOLUTIONS = new Set<VideoResolution>(["automatic", "original", "fullHd", "hd"]);
const VIDEO_QUALITIES = new Set<VideoQuality>(["high", "balanced", "smallest"]);
const VIDEO_COMPRESSION_GOALS = new Set<VideoCompressionGoal>(["quality", "fileSize"]);
const AUDIO_COMPRESSION_PRESETS = new Set<AudioCompressionPreset>(["high", "balanced", "smallest"]);
const SUBTITLE_FORMATS = new Set<SubtitleOutputFormat>(["srt", "vtt"]);
const THUMBNAIL_MODES = new Set<ThumbnailMode>(["frame", "contactSheet"]);
const THUMBNAIL_FORMATS = new Set<ThumbnailOutputFormat>(["jpeg", "png"]);
const PDF_SPLIT_MODES = new Set<PdfSplitMode>(["everyPage", "extract"]);
const PDF_PAGE_FORMATS = new Set<PdfPageImageFormat>(["png", "jpeg"]);
const PDF_PAGE_RESOLUTIONS = new Set<PdfPageImageResolution>(["screen", "print"]);
const PDF_COMPRESSION_PRESETS = new Set<PdfCompressionPreset>(["high", "balanced", "smallest"]);
const PDF_COMPRESSION_GOALS = new Set<PdfCompressionGoal>(["quality", "fileSize"]);
const ARCHIVE_FORMAT_SET = new Set<ArchiveFormat>(ARCHIVE_FORMATS);

export type SavedRecipeSettings =
  | {
      kind: "resize";
      width: number;
      height: number;
      preserveAspect: boolean;
    }
  | {
      kind: "optimize";
      keepMetadata: boolean;
      compressionGoal: ImageOptimizationGoal;
      targetSizeBytes: number | null;
    }
  | { kind: "exportImages"; presets: ImageExportPreset[] }
  | {
      kind: "encodeVideo";
      preset: VideoEncodingPreset;
      resolution: VideoResolution;
      quality: VideoQuality;
      compressionGoal: VideoCompressionGoal;
      targetSizeBytes: number | null;
    }
  | { kind: "compressAudio"; preset: AudioCompressionPreset }
  | { kind: "extractAudio"; outputFormat: AudioOutputFormat }
  | {
      kind: "transcribe";
      model: TranscriptionModel;
      language: string;
      outputFormat: TranscriptionOutputFormat;
    }
  | { kind: "extractSubtitles"; outputFormat: SubtitleOutputFormat }
  | { kind: "recognizeText"; outputFormat: OcrOutputFormat }
  | {
      kind: "generateThumbnails";
      mode: ThumbnailMode;
      outputFormat: ThumbnailOutputFormat;
    }
  | { kind: "splitPdf"; mode: PdfSplitMode }
  | {
      kind: "exportPdfPages";
      mode: PdfSplitMode;
      outputFormat: PdfPageImageFormat;
      resolution: PdfPageImageResolution;
    }
  | {
      kind: "compressPdf";
      preset: PdfCompressionPreset;
      compressionGoal: PdfCompressionGoal;
      targetSizeBytes: number | null;
    }
  | { kind: "createArchive"; format: ArchiveFormat };

export interface SavedRecipe {
  id: string;
  name: string;
  operation: Operation;
  directory: string | null;
  suffix: string;
  settings?: SavedRecipeSettings;
}

interface SavedRecipeSource {
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  keepMetadata: boolean;
  imageOptimizationGoal: ImageOptimizationGoal;
  imageTargetSizeBytes: number | null;
  imageExportPresets: ImageExportPreset[];
  audioOutputFormat: AudioOutputFormat;
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
  thumbnailMode: ThumbnailMode;
  thumbnailOutputFormat: ThumbnailOutputFormat;
  pdfSplitMode: PdfSplitMode;
  pdfPageImageFormat: PdfPageImageFormat;
  pdfPageImageResolution: PdfPageImageResolution;
  pdfCompressionPreset: PdfCompressionPreset;
  pdfCompressionGoal: PdfCompressionGoal;
  pdfTargetSizeBytes: number | null;
  archiveFormat: ArchiveFormat;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isEnumValue<T extends string>(values: Set<T>, value: unknown): value is T {
  return typeof value === "string" && values.has(value as T);
}

function isDimension(value: unknown): value is number {
  return typeof value === "number" && Number.isInteger(value) && value > 0 && value <= 32_768;
}

export function isValidRecipeSuffix(value: string): boolean {
  return (
    Array.from(value).length <= 80 &&
    !Array.from(value).some((character) => ["/", "\\", ":", "\0"].includes(character))
  );
}

export function normalizeSavedRecipeSettings(
  operation: Operation,
  value: unknown,
): SavedRecipeSettings | undefined | null {
  if (value === undefined) {
    return operation === "recognizeText"
      ? { kind: "recognizeText", outputFormat: "text" }
      : undefined;
  }
  if (!isRecord(value) || value.kind !== operation) return null;

  switch (operation) {
    case "resize":
      return isDimension(value.width) &&
        isDimension(value.height) &&
        typeof value.preserveAspect === "boolean"
        ? {
            kind: operation,
            width: value.width,
            height: value.height,
            preserveAspect: value.preserveAspect,
          }
        : null;
    case "optimize": {
      if (typeof value.keepMetadata !== "boolean") return null;
      const compressionGoal =
        value.compressionGoal === undefined
          ? "quality"
          : isEnumValue(IMAGE_OPTIMIZATION_GOALS, value.compressionGoal)
            ? value.compressionGoal
            : null;
      if (!compressionGoal) return null;
      if (compressionGoal === "fileSize" && !isImageTargetSizeBytes(value.targetSizeBytes))
        return null;
      if (
        compressionGoal === "quality" &&
        value.targetSizeBytes !== undefined &&
        value.targetSizeBytes !== null
      )
        return null;
      return {
        kind: operation,
        keepMetadata: value.keepMetadata,
        compressionGoal,
        targetSizeBytes: imageTargetSizeForRequest(
          compressionGoal,
          isImageTargetSizeBytes(value.targetSizeBytes) ? value.targetSizeBytes : null,
        ),
      };
    }
    case "exportImages": {
      if (!Array.isArray(value.presets)) return null;
      const presets = (value.presets as unknown[]).filter(
        (candidate): candidate is ImageExportPreset => isEnumValue(IMAGE_EXPORT_PRESETS, candidate),
      );
      if (presets.length !== value.presets.length || new Set(presets).size !== presets.length)
        return null;
      return presets.length > 0 ? { kind: operation, presets } : null;
    }
    case "encodeVideo": {
      if (
        !isEnumValue(VIDEO_PRESETS, value.preset) ||
        !isEnumValue(VIDEO_RESOLUTIONS, value.resolution) ||
        !isEnumValue(VIDEO_QUALITIES, value.quality)
      )
        return null;
      const storedGoal =
        value.compressionGoal === undefined
          ? "quality"
          : isEnumValue(VIDEO_COMPRESSION_GOALS, value.compressionGoal)
            ? value.compressionGoal
            : null;
      if (!storedGoal) return null;
      const compressionGoal = effectiveVideoCompressionGoal(value.preset, storedGoal);
      if (compressionGoal !== storedGoal) return null;
      if (compressionGoal === "fileSize" && !isVideoTargetSizeBytes(value.targetSizeBytes))
        return null;
      if (
        compressionGoal === "quality" &&
        value.targetSizeBytes !== undefined &&
        value.targetSizeBytes !== null
      )
        return null;
      const targetSizeBytes = isVideoTargetSizeBytes(value.targetSizeBytes)
        ? value.targetSizeBytes
        : null;
      return {
        kind: operation,
        preset: value.preset,
        resolution: value.resolution,
        quality: value.quality,
        compressionGoal,
        targetSizeBytes: compressionGoal === "fileSize" ? targetSizeBytes : null,
      };
    }
    case "compressAudio":
      return isEnumValue(AUDIO_COMPRESSION_PRESETS, value.preset)
        ? { kind: operation, preset: value.preset }
        : null;
    case "extractAudio":
      return isEnumValue(AUDIO_FORMATS, value.outputFormat)
        ? { kind: operation, outputFormat: value.outputFormat }
        : null;
    case "transcribe":
      return isEnumValue(TRANSCRIPTION_MODELS, value.model) &&
        typeof value.language === "string" &&
        TRANSCRIPTION_LANGUAGE_SET.has(value.language) &&
        isEnumValue(TRANSCRIPTION_FORMATS, value.outputFormat)
        ? {
            kind: operation,
            model: value.model,
            language: value.language,
            outputFormat: value.outputFormat,
          }
        : null;
    case "extractSubtitles":
      return isEnumValue(SUBTITLE_FORMATS, value.outputFormat)
        ? { kind: operation, outputFormat: value.outputFormat }
        : null;
    case "recognizeText":
      return isEnumValue(OCR_OUTPUT_FORMATS, value.outputFormat)
        ? { kind: operation, outputFormat: value.outputFormat }
        : null;
    case "generateThumbnails":
      return isEnumValue(THUMBNAIL_MODES, value.mode) &&
        isEnumValue(THUMBNAIL_FORMATS, value.outputFormat)
        ? { kind: operation, mode: value.mode, outputFormat: value.outputFormat }
        : null;
    case "splitPdf":
      return isEnumValue(PDF_SPLIT_MODES, value.mode)
        ? { kind: operation, mode: value.mode }
        : null;
    case "exportPdfPages":
      return isEnumValue(PDF_SPLIT_MODES, value.mode) &&
        isEnumValue(PDF_PAGE_FORMATS, value.outputFormat) &&
        isEnumValue(PDF_PAGE_RESOLUTIONS, value.resolution)
        ? {
            kind: operation,
            mode: value.mode,
            outputFormat: value.outputFormat,
            resolution: value.resolution,
          }
        : null;
    case "compressPdf": {
      if (!isEnumValue(PDF_COMPRESSION_PRESETS, value.preset)) return null;
      const compressionGoal =
        value.compressionGoal === undefined
          ? "quality"
          : isEnumValue(PDF_COMPRESSION_GOALS, value.compressionGoal)
            ? value.compressionGoal
            : null;
      if (!compressionGoal) return null;
      if (compressionGoal === "fileSize" && !isPdfTargetSizeBytes(value.targetSizeBytes))
        return null;
      if (
        compressionGoal === "quality" &&
        value.targetSizeBytes !== undefined &&
        value.targetSizeBytes !== null
      )
        return null;
      return {
        kind: operation,
        preset: value.preset,
        compressionGoal,
        targetSizeBytes: pdfTargetSizeForRequest(
          compressionGoal,
          isPdfTargetSizeBytes(value.targetSizeBytes) ? value.targetSizeBytes : null,
        ),
      };
    }
    case "createArchive":
      return isEnumValue(ARCHIVE_FORMAT_SET, value.format)
        ? { kind: operation, format: value.format }
        : null;
    default:
      return null;
  }
}

function normalizeRecipe(value: unknown): SavedRecipe | null {
  if (!isRecord(value)) return null;
  if (typeof value.id !== "string" || !value.id.trim() || value.id.length > 100) return null;
  if (typeof value.name !== "string") return null;
  const name = value.name.trim();
  if (!name || Array.from(name).length > MAX_SAVED_RECIPE_NAME_LENGTH) return null;
  if (typeof value.operation !== "string" || !RECIPE_OPERATIONS.has(value.operation as Operation))
    return null;
  if (value.directory !== null && typeof value.directory !== "string") return null;
  const directory =
    typeof value.directory === "string" && value.directory.trim() ? value.directory : null;
  if (directory && directory.length > 4_096) return null;
  if (typeof value.suffix !== "string" || !isValidRecipeSuffix(value.suffix)) return null;
  const operation = value.operation as Operation;
  const settings = normalizeSavedRecipeSettings(operation, value.settings);
  if (settings === null) return null;

  return {
    id: value.id,
    name,
    operation,
    directory,
    suffix: value.suffix,
    ...(settings ? { settings } : {}),
  };
}

function storedRecipeCandidates(serialized: string | null): unknown[] {
  if (!serialized) return [];
  const parsed = JSON.parse(serialized) as unknown;
  if (Array.isArray(parsed)) return parsed;
  if (!isRecord(parsed) || (parsed.version !== 1 && parsed.version !== 2)) return [];
  const stored = parsed.recipes ?? parsed.presets;
  return Array.isArray(stored) ? stored : [];
}

export function retiredSavedRecipeIds(serialized: string | null): string[] {
  try {
    const ids: string[] = [];
    const seen = new Set<string>();
    for (const candidate of storedRecipeCandidates(serialized)) {
      if (!isRecord(candidate) || !isRetiredOperation(candidate.operation)) continue;
      if (
        typeof candidate.id !== "string" ||
        !candidate.id.trim() ||
        candidate.id.length > 100 ||
        seen.has(candidate.id)
      )
        continue;
      seen.add(candidate.id);
      ids.push(candidate.id);
      if (ids.length === MAX_SAVED_RECIPES) break;
    }
    return ids;
  } catch {
    return [];
  }
}

export function captureSavedRecipeSettings(
  operation: Operation,
  source: SavedRecipeSource,
): SavedRecipeSettings | undefined {
  switch (operation) {
    case "resize":
      return source.resizeWidth && source.resizeHeight
        ? {
            kind: operation,
            width: source.resizeWidth,
            height: source.resizeHeight,
            preserveAspect: source.preserveAspect,
          }
        : undefined;
    case "optimize":
      if (
        source.imageOptimizationGoal === "fileSize" &&
        !isImageTargetSizeBytes(source.imageTargetSizeBytes)
      )
        return undefined;
      return {
        kind: operation,
        keepMetadata: source.keepMetadata,
        compressionGoal: source.imageOptimizationGoal,
        targetSizeBytes: imageTargetSizeForRequest(
          source.imageOptimizationGoal,
          source.imageTargetSizeBytes,
        ),
      };
    case "exportImages":
      return { kind: operation, presets: [...source.imageExportPresets] };
    case "encodeVideo": {
      const compressionGoal = source.videoCompressionGoal === "fileSize" ? "fileSize" : "quality";
      if (
        effectiveVideoCompressionGoal(source.videoEncodingPreset, compressionGoal) === "fileSize" &&
        !isVideoTargetSizeBytes(source.videoTargetSizeBytes)
      )
        return undefined;
      return {
        kind: operation,
        preset: source.videoEncodingPreset,
        resolution: source.videoResolution,
        quality: source.videoQuality,
        compressionGoal: effectiveVideoCompressionGoal(source.videoEncodingPreset, compressionGoal),
        targetSizeBytes: videoTargetSizeForRequest(
          source.videoEncodingPreset,
          compressionGoal,
          source.videoTargetSizeBytes,
        ),
      };
    }
    case "compressAudio":
      return { kind: operation, preset: source.audioCompressionPreset };
    case "extractAudio":
      return { kind: operation, outputFormat: source.audioOutputFormat };
    case "transcribe":
      return {
        kind: operation,
        model: source.transcriptionModel,
        language: source.transcriptionLanguage,
        outputFormat: source.transcriptionOutputFormat,
      };
    case "extractSubtitles":
      return { kind: operation, outputFormat: source.subtitleOutputFormat };
    case "recognizeText":
      return { kind: operation, outputFormat: source.ocrOutputFormat };
    case "generateThumbnails":
      return {
        kind: operation,
        mode: source.thumbnailMode,
        outputFormat: source.thumbnailOutputFormat,
      };
    case "splitPdf":
      return { kind: operation, mode: source.pdfSplitMode };
    case "exportPdfPages":
      return {
        kind: operation,
        mode: source.pdfSplitMode,
        outputFormat: source.pdfPageImageFormat,
        resolution: source.pdfPageImageResolution,
      };
    case "compressPdf":
      if (
        source.pdfCompressionGoal === "fileSize" &&
        !isPdfTargetSizeBytes(source.pdfTargetSizeBytes)
      )
        return undefined;
      return {
        kind: operation,
        preset: source.pdfCompressionPreset,
        compressionGoal: source.pdfCompressionGoal,
        targetSizeBytes: pdfTargetSizeForRequest(
          source.pdfCompressionGoal,
          source.pdfTargetSizeBytes,
        ),
      };
    case "createArchive":
      return { kind: operation, format: source.archiveFormat };
    default:
      return undefined;
  }
}

export function parseSavedRecipes(serialized: string | null): SavedRecipe[] {
  if (!serialized) return [];
  try {
    const candidates = storedRecipeCandidates(serialized);
    const seenIds = new Set<string>();
    const seenNames = new Set<string>();
    const recipes: SavedRecipe[] = [];
    for (const candidate of candidates) {
      const recipe = normalizeRecipe(candidate);
      if (!recipe || seenIds.has(recipe.id)) continue;
      const nameKey = `${recipe.operation}\0${recipe.name.toLocaleLowerCase()}`;
      if (seenNames.has(nameKey)) continue;
      seenIds.add(recipe.id);
      seenNames.add(nameKey);
      recipes.push(recipe);
      if (recipes.length === MAX_SAVED_RECIPES) break;
    }
    return recipes;
  } catch {
    return [];
  }
}

export function serializeSavedRecipes(recipes: SavedRecipe[]): string {
  return JSON.stringify({ version: 2, recipes });
}

export function savedRecipeNameExists(
  recipes: SavedRecipe[],
  operation: Operation,
  name: string,
): boolean {
  const normalized = name.trim().toLocaleLowerCase();
  return recipes.some(
    (recipe) => recipe.operation === operation && recipe.name.toLocaleLowerCase() === normalized,
  );
}

export function savedRecipeMatches(
  recipe: SavedRecipe,
  operation: Operation,
  directory: string | null,
  suffix: string,
  settings?: SavedRecipeSettings,
): boolean {
  return (
    recipe.operation === operation &&
    recipe.directory === directory &&
    recipe.suffix === suffix &&
    (recipe.settings === undefined || JSON.stringify(recipe.settings) === JSON.stringify(settings))
  );
}
