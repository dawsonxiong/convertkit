import { useCallback } from "react";
import type { FileInfo, Operation, OutputOptions } from "../types";
import { useAppStore } from "../store/useAppStore";

function nestedOutputDirectory(directory: string, relativePath?: string | null) {
  if (!relativePath) return directory;
  const parents = relativePath
    .split(/[\\/]/)
    .slice(0, -1)
    .filter((part) => part && part !== "." && part !== "..");
  if (parents.length === 0) return directory;

  const separator = directory.includes("\\") && !directory.includes("/") ? "\\" : "/";
  return `${directory.replace(/[\\/]+$/, "")}${separator}${parents.join(separator)}`;
}

export function useOutputOptions(operation: Operation): (file: FileInfo) => OutputOptions {
  const directory = useAppStore((state) => state.outputDirectory);
  const suffix = useAppStore((state) => state.outputSuffixes[operation]);

  return useCallback(
    (file: FileInfo) => ({
      directory:
        directory && operation !== "mergePdf" && operation !== "createArchive"
          ? nestedOutputDirectory(directory, file.relativePath)
          : directory,
      suffix,
    }),
    [directory, operation, suffix],
  );
}
