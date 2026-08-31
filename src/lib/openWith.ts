import {
  isActiveOperation,
  isPathSupportedForOperation,
  type ActiveOperation,
} from "./operations.ts";
import type { Operation } from "../types";

const OPEN_WITH_FALLBACKS: readonly ActiveOperation[] = [
  "convert",
  "extractArchive",
  "splitPdf",
  "inspect",
];

export function operationForOpenedPaths(
  paths: readonly string[],
  current: Operation,
): ActiveOperation {
  const active = isActiveOperation(current) ? current : "convert";
  if (paths.length === 0) return active;
  if (paths.every((path) => isPathSupportedForOperation(path, active))) return active;
  if (paths.length >= 2 && paths.every((path) => isPathSupportedForOperation(path, "mergePdf"))) {
    return "mergePdf";
  }
  return (
    OPEN_WITH_FALLBACKS.find((operation) =>
      paths.every((path) => isPathSupportedForOperation(path, operation)),
    ) ?? "inspect"
  );
}
