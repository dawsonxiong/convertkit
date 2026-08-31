import type { FileInfo, PdfCompressionGoal } from "../types";

export const PDF_TARGET_SIZE_MIN_MB = 1;
export const PDF_TARGET_SIZE_MAX_MB = 102_400;
const DEFAULT_PDF_TARGET_SIZE_MB = 10;
export const PDF_BYTES_PER_MEBIBYTE = 1024 * 1024;

export const DEFAULT_PDF_TARGET_SIZE_BYTES = DEFAULT_PDF_TARGET_SIZE_MB * PDF_BYTES_PER_MEBIBYTE;
const PDF_TARGET_SIZE_MIN_BYTES = PDF_TARGET_SIZE_MIN_MB * PDF_BYTES_PER_MEBIBYTE;
const PDF_TARGET_SIZE_MAX_BYTES = PDF_TARGET_SIZE_MAX_MB * PDF_BYTES_PER_MEBIBYTE;

export function isPdfTargetSizeBytes(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= PDF_TARGET_SIZE_MIN_BYTES &&
    value <= PDF_TARGET_SIZE_MAX_BYTES &&
    value % PDF_BYTES_PER_MEBIBYTE === 0
  );
}

export function pdfTargetSizeBytesFromMb(value: number): number | null {
  if (!Number.isInteger(value) || value < PDF_TARGET_SIZE_MIN_MB || value > PDF_TARGET_SIZE_MAX_MB)
    return null;
  return value * PDF_BYTES_PER_MEBIBYTE;
}

export function pdfTargetSizeMbFromBytes(value: number | null): number | null {
  return isPdfTargetSizeBytes(value) ? value / PDF_BYTES_PER_MEBIBYTE : null;
}

export function pdfTargetSizeForRequest(
  goal: PdfCompressionGoal,
  targetSizeBytes: number | null,
): number | null {
  return goal === "fileSize" && isPdfTargetSizeBytes(targetSizeBytes) ? targetSizeBytes : null;
}

export function pdfCompressionSettingsAreReady(
  goal: PdfCompressionGoal,
  targetSizeBytes: number | null,
  files: Pick<FileInfo, "size">[],
): boolean {
  if (goal === "quality") return true;
  return (
    isPdfTargetSizeBytes(targetSizeBytes) &&
    files.length > 0 &&
    files.every((file) => targetSizeBytes < file.size)
  );
}
