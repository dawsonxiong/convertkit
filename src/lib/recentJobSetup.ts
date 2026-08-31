import type {
  FileInfo,
  JobRequest,
  Operation,
  RecentJobInputSetup,
  RecentJobSetup,
} from "../types/index.ts";
import { isValidPageSelection } from "./pdfPages.ts";
import {
  isValidRecipeSuffix,
  normalizeSavedRecipeSettings,
  SAVED_RECIPE_OPERATIONS,
  type SavedRecipeSettings,
} from "./savedRecipes.ts";

const MAX_PATH_LENGTH = 4_096;
const MAX_INPUT_SETUPS = 100;
const MAX_OUTPUT_FORMAT_LENGTH = 32;
export const MAX_RECENT_PAGE_SELECTION_LENGTH = 512;

const RECIPE_OPERATION_SET = new Set<Operation>(SAVED_RECIPE_OPERATIONS);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function settingsForRequest(request: JobRequest): SavedRecipeSettings | undefined {
  switch (request.operation) {
    case "resize":
      return {
        kind: request.operation,
        width: request.width,
        height: request.height,
        preserveAspect: request.preserveAspect,
      };
    case "optimize":
      return {
        kind: request.operation,
        keepMetadata: request.keepMetadata,
        compressionGoal: request.compressionGoal,
        targetSizeBytes: request.targetSizeBytes,
      };
    case "exportImages":
      return { kind: request.operation, presets: request.presets };
    case "encodeVideo":
      return {
        kind: request.operation,
        preset: request.preset,
        resolution: request.resolution,
        quality: request.quality,
        compressionGoal: request.compressionGoal,
        targetSizeBytes: request.targetSizeBytes,
      };
    case "compressAudio":
      return { kind: request.operation, preset: request.preset };
    case "extractAudio":
      return { kind: request.operation, outputFormat: request.outputFormat };
    case "transcribe":
      return {
        kind: request.operation,
        model: request.model,
        language: request.language,
        outputFormat: request.outputFormat,
      };
    case "extractSubtitles":
      return { kind: request.operation, outputFormat: request.outputFormat };
    case "recognizeText":
      return { kind: request.operation, outputFormat: request.outputFormat };
    case "generateThumbnails":
      return {
        kind: request.operation,
        mode: request.mode,
        outputFormat: request.outputFormat,
      };
    case "splitPdf":
      return { kind: request.operation, mode: request.mode };
    case "exportPdfPages":
      return {
        kind: request.operation,
        mode: request.mode,
        outputFormat: request.outputFormat,
        resolution: request.resolution,
      };
    case "compressPdf":
      return {
        kind: request.operation,
        preset: request.preset,
        compressionGoal: request.compressionGoal,
        targetSizeBytes: request.targetSizeBytes,
      };
    case "createArchive":
      return { kind: request.operation, format: request.format };
    default:
      return undefined;
  }
}

function inputSettingsForRequests(requests: readonly JobRequest[], files: readonly FileInfo[]) {
  const inputs: Record<string, RecentJobInputSetup> = {};
  for (const file of files) {
    if (file.relativePath) inputs[file.path] = { relativePath: file.relativePath };
  }
  for (const request of requests) {
    if (!("inputPath" in request)) continue;
    if (request.operation === "convert") {
      inputs[request.inputPath] = {
        ...inputs[request.inputPath],
        outputFormat: request.outputFormat,
      };
    } else if (request.operation === "extractAudio" || request.operation === "extractSubtitles") {
      inputs[request.inputPath] = { ...inputs[request.inputPath], trackIndex: request.streamIndex };
    }
  }
  return Object.keys(inputs).length > 0 ? inputs : undefined;
}

function isSafeRelativePath(value: string): boolean {
  if (!value || value.length > MAX_PATH_LENGTH || value.startsWith("/") || value.startsWith("\\"))
    return false;
  const segments = value.split(/[\\/]/);
  return segments.every((segment) => segment && segment !== "." && segment !== "..");
}

export function normalizeRecentJobSetup(
  operation: Operation,
  value: unknown,
  allowedInputPaths: ReadonlySet<string>,
): RecentJobSetup | undefined | null {
  if (value === undefined) return undefined;
  if (!RECIPE_OPERATION_SET.has(operation) || !isRecord(value) || value.version !== 1) return null;
  if (value.directory !== null && typeof value.directory !== "string") return null;
  const directory =
    typeof value.directory === "string" && value.directory.trim() ? value.directory : null;
  if (directory && directory.length > MAX_PATH_LENGTH) return null;
  if (typeof value.suffix !== "string" || !isValidRecipeSuffix(value.suffix)) return null;

  const settings = normalizeSavedRecipeSettings(operation, value.settings);
  if (settings === null) return null;

  let inputs: Record<string, RecentJobInputSetup> | undefined;
  if (value.inputs !== undefined) {
    if (!isRecord(value.inputs)) return null;
    const entries = Object.entries(value.inputs);
    if (entries.length === 0 || entries.length > MAX_INPUT_SETUPS) return null;
    inputs = {};
    for (const [path, candidate] of entries) {
      if (
        !path ||
        path.length > MAX_PATH_LENGTH ||
        !allowedInputPaths.has(path) ||
        !isRecord(candidate)
      )
        return null;
      const relativePath = candidate.relativePath;
      if (
        relativePath !== undefined &&
        (typeof relativePath !== "string" || !isSafeRelativePath(relativePath))
      )
        return null;
      if (operation === "convert") {
        if (
          typeof candidate.outputFormat !== "string" ||
          !candidate.outputFormat ||
          candidate.outputFormat.length > MAX_OUTPUT_FORMAT_LENGTH
        )
          return null;
        inputs[path] = {
          outputFormat: candidate.outputFormat,
          ...(relativePath ? { relativePath } : {}),
        };
      } else if (operation === "extractAudio" || operation === "extractSubtitles") {
        if (
          typeof candidate.trackIndex !== "number" ||
          !Number.isInteger(candidate.trackIndex) ||
          candidate.trackIndex < 0 ||
          candidate.trackIndex > 16_384
        )
          return null;
        inputs[path] = {
          trackIndex: candidate.trackIndex,
          ...(relativePath ? { relativePath } : {}),
        };
      } else {
        if (!relativePath) return null;
        inputs[path] = { relativePath };
      }
    }
    if (
      (operation === "convert" ||
        operation === "extractAudio" ||
        operation === "extractSubtitles") &&
      [...allowedInputPaths].some((path) => inputs?.[path] === undefined)
    )
      return null;
  }

  let pageSelection: string | undefined;
  if (value.pageSelection !== undefined) {
    if (
      (operation !== "splitPdf" && operation !== "exportPdfPages") ||
      settings?.kind !== operation ||
      settings.mode !== "extract" ||
      typeof value.pageSelection !== "string" ||
      value.pageSelection.length > MAX_RECENT_PAGE_SELECTION_LENGTH ||
      !isValidPageSelection(value.pageSelection)
    )
      return null;
    pageSelection = value.pageSelection.trim();
  } else if (
    (operation === "splitPdf" || operation === "exportPdfPages") &&
    settings?.kind === operation &&
    settings.mode === "extract"
  ) {
    return null;
  }

  return {
    version: 1,
    directory,
    suffix: value.suffix,
    ...(settings ? { settings } : {}),
    ...(inputs ? { inputs } : {}),
    ...(pageSelection ? { pageSelection } : {}),
  };
}

export function captureRecentJobSetup(
  requests: readonly JobRequest[],
  files: readonly FileInfo[],
  directory: string | null,
  suffix: string,
): RecentJobSetup | undefined {
  const first = requests[0];
  if (!first || !RECIPE_OPERATION_SET.has(first.operation)) return undefined;
  if (requests.some((request) => request.operation !== first.operation)) return undefined;

  const settings = settingsForRequest(first);
  const inputs = inputSettingsForRequests(requests, files);
  const pageSelection =
    (first.operation === "splitPdf" || first.operation === "exportPdfPages") &&
    first.mode === "extract"
      ? first.pageSelection
      : undefined;
  const candidate = {
    version: 1,
    directory,
    suffix,
    ...(settings ? { settings } : {}),
    ...(inputs ? { inputs } : {}),
    ...(pageSelection ? { pageSelection } : {}),
  };
  const inputPaths = new Set(files.map((file) => file.path));
  const normalized = normalizeRecentJobSetup(first.operation, candidate, inputPaths);
  return normalized ?? undefined;
}
