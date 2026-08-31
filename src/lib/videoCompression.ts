import type { VideoCompressionGoal, VideoEncodingPreset } from "../types";

export const VIDEO_TARGET_SIZE_MIN_MB = 1;
export const VIDEO_TARGET_SIZE_MAX_MB = 102_400;
const DEFAULT_VIDEO_TARGET_SIZE_MB = 100;
export const BYTES_PER_MEBIBYTE = 1024 * 1024;

export const DEFAULT_VIDEO_TARGET_SIZE_BYTES = DEFAULT_VIDEO_TARGET_SIZE_MB * BYTES_PER_MEBIBYTE;
const VIDEO_TARGET_SIZE_MIN_BYTES = VIDEO_TARGET_SIZE_MIN_MB * BYTES_PER_MEBIBYTE;
const VIDEO_TARGET_SIZE_MAX_BYTES = VIDEO_TARGET_SIZE_MAX_MB * BYTES_PER_MEBIBYTE;

export function isVideoTargetSizeBytes(value: unknown): value is number {
  return (
    typeof value === "number" &&
    Number.isSafeInteger(value) &&
    value >= VIDEO_TARGET_SIZE_MIN_BYTES &&
    value <= VIDEO_TARGET_SIZE_MAX_BYTES
  );
}

export function videoTargetSizeBytesFromMb(value: number): number | null {
  if (
    !Number.isInteger(value) ||
    value < VIDEO_TARGET_SIZE_MIN_MB ||
    value > VIDEO_TARGET_SIZE_MAX_MB
  )
    return null;
  return value * BYTES_PER_MEBIBYTE;
}

export function videoTargetSizeMbFromBytes(value: number | null): number | null {
  return isVideoTargetSizeBytes(value) ? value / BYTES_PER_MEBIBYTE : null;
}

export function effectiveVideoCompressionGoal(
  preset: VideoEncodingPreset,
  goal: VideoCompressionGoal,
): VideoCompressionGoal {
  return preset === "archive" ? "quality" : goal;
}

export function videoTargetSizeForRequest(
  preset: VideoEncodingPreset,
  goal: VideoCompressionGoal,
  targetSizeBytes: number | null,
): number | null {
  return effectiveVideoCompressionGoal(preset, goal) === "fileSize" &&
    isVideoTargetSizeBytes(targetSizeBytes)
    ? targetSizeBytes
    : null;
}

export function videoCompressionSettingsAreReady(
  preset: VideoEncodingPreset,
  goal: VideoCompressionGoal,
  targetSizeBytes: number | null,
): boolean {
  const effectiveGoal = effectiveVideoCompressionGoal(preset, goal);
  return effectiveGoal === "quality" || isVideoTargetSizeBytes(targetSizeBytes);
}
