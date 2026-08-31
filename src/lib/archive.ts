import type { ArchiveEntryRequest, ArchiveFormat, FileInfo } from "../types";

export const ARCHIVE_FORMATS = [
  "zip",
  "tar",
  "tarGz",
  "sevenZ",
  "gzip",
] as const satisfies readonly ArchiveFormat[];

export function archiveFormatLabel(format: ArchiveFormat): string {
  switch (format) {
    case "zip":
      return "ZIP";
    case "tar":
      return "TAR";
    case "tarGz":
      return "TAR.GZ";
    case "sevenZ":
      return "7Z";
    case "gzip":
      return "GZIP";
  }
}

export function gzipCreationInputError(
  files: readonly Pick<FileInfo, "relativePath">[],
): string | null {
  if (files.length !== 1) return "GZIP compresses exactly one file at a time.";
  if (files[0]?.relativePath != null) {
    return "GZIP cannot compress a folder. Choose one regular file directly.";
  }
  return null;
}

export function archiveEntryRequest(
  file: Pick<FileInfo, "name" | "path" | "relativePath">,
): ArchiveEntryRequest {
  return {
    inputPath: file.path,
    archivePath: (file.relativePath || file.name).replaceAll("\\", "/"),
    folderDerived: file.relativePath != null,
  };
}

export function isSupportedArchivePath(path: string): boolean {
  return archivePathLabel(path) !== null;
}

export function archivePathLabel(path: string): string | null {
  const normalized = path.trim().toLowerCase();
  if (normalized.endsWith(".tar.gz") || normalized.endsWith(".tgz")) return "TAR.GZ";
  if (normalized.endsWith(".gz")) return "GZIP";
  if (normalized.endsWith(".7z")) return "7Z";
  if (normalized.endsWith(".zip")) return "ZIP";
  if (normalized.endsWith(".tar")) return "TAR";
  return null;
}
