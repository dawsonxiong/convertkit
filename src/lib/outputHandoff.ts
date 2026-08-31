import type { ConversionResult, Operation } from "../types/index.ts";
import { isPathSupportedForOperation, type ActiveOperation } from "./operations.ts";
import { SIDEBAR_OPERATIONS } from "./toolNavigation.ts";

export function uniqueOutputPaths(paths: readonly (string | null | undefined)[]): string[] {
  return Array.from(new Set(paths.filter((path): path is string => Boolean(path))));
}

export function outputPathsForResult(result: ConversionResult): string[] {
  return uniqueOutputPaths([result.output_path, ...result.output_paths]);
}

export function outputPathsForResults(results: readonly ConversionResult[]): string[] {
  return uniqueOutputPaths(
    results.flatMap((result) => [result.output_path, ...result.output_paths]),
  );
}

export function outputHandoffOperations(
  paths: readonly string[],
  sourceOperation: Operation,
): ActiveOperation[] {
  const uniquePaths = uniqueOutputPaths(paths);
  if (uniquePaths.length === 0) return [];

  return SIDEBAR_OPERATIONS.filter(
    (operation) =>
      operation !== sourceOperation &&
      uniquePaths.every((path) => isPathSupportedForOperation(path, operation)),
  );
}
