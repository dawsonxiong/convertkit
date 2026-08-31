import type { ConversionError } from "../types";

const ERROR_KINDS = new Set<ConversionError["kind"]>([
  "MissingDependency",
  "UnsupportedConversion",
  "InputNotFound",
  "ProcessFailed",
  "Cancelled",
  "Timeout",
  "OutputMissing",
  "OutputConflict",
  "ArchivePasswordRequired",
  "IncorrectArchivePassword",
  "TargetSizeUnreachable",
  "OutputNotSmaller",
  "DiskFull",
]);

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

export function normalizeConversionError(error: unknown): ConversionError {
  if (
    isRecord(error) &&
    typeof error.kind === "string" &&
    ERROR_KINDS.has(error.kind as ConversionError["kind"])
  ) {
    const detail = isRecord(error.detail)
      ? Object.fromEntries(
          Object.entries(error.detail).flatMap(([key, value]) => {
            if (
              typeof value === "string" ||
              ((error.kind === "TargetSizeUnreachable" || error.kind === "OutputNotSmaller") &&
                typeof value === "number")
            ) {
              return [[key, String(value)]];
            }
            return [];
          }),
        )
      : {};
    return { kind: error.kind as ConversionError["kind"], detail };
  }

  return {
    kind: "ProcessFailed",
    detail: { message: error instanceof Error ? error.message : String(error) },
  };
}
