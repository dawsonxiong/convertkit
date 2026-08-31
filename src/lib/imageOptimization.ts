import type { FileInfo, ImageOptimizationGoal } from "../types";

export const IMAGE_TARGET_SIZE_MIN_KB = 16;
export const IMAGE_TARGET_SIZE_MAX_KB = 100 * 1024;
export const IMAGE_BYTES_PER_KIBIBYTE = 1024;
export const DEFAULT_IMAGE_TARGET_SIZE_BYTES = 500 * IMAGE_BYTES_PER_KIBIBYTE;

const TARGET_SIZE_FORMATS = new Set(["jpg", "jpeg", "webp", "avif", "heic", "heif"]);

export function supportsImageTargetSize(format: string): boolean {
  return TARGET_SIZE_FORMATS.has(format.toLowerCase().replace(/^\./, ""));
}

export function isImageTargetSizeBytes(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value % IMAGE_BYTES_PER_KIBIBYTE === 0 &&
    value >= IMAGE_TARGET_SIZE_MIN_KB * IMAGE_BYTES_PER_KIBIBYTE &&
    value <= IMAGE_TARGET_SIZE_MAX_KB * IMAGE_BYTES_PER_KIBIBYTE
  );
}

export function imageTargetSizeBytesFromKb(value: number): number | null {
  if (!Number.isInteger(value)) return null;
  const bytes = value * IMAGE_BYTES_PER_KIBIBYTE;
  return isImageTargetSizeBytes(bytes) ? bytes : null;
}

export function imageTargetSizeKbFromBytes(value: number | null): number | null {
  return isImageTargetSizeBytes(value) ? value / IMAGE_BYTES_PER_KIBIBYTE : null;
}

export function imageTargetSizeForRequest(
  goal: ImageOptimizationGoal,
  targetSizeBytes: number | null,
): number | null {
  return goal === "fileSize" && isImageTargetSizeBytes(targetSizeBytes) ? targetSizeBytes : null;
}

export function imageOptimizationSettingsAreReady(
  goal: ImageOptimizationGoal,
  targetSizeBytes: number | null,
  files: readonly FileInfo[],
): boolean {
  if (goal === "quality") return true;
  return (
    isImageTargetSizeBytes(targetSizeBytes) &&
    files.length > 0 &&
    files.every((file) => supportsImageTargetSize(file.format) && targetSizeBytes < file.size)
  );
}
