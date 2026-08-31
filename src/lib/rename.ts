import type { FileInfo, RenameItemRequest, RenameSettings } from "../types";

interface RenamePreview {
  inputPath: string;
  originalName: string;
  outputName: string;
  changed: boolean;
  valid: boolean;
  conflict: boolean;
}

function splitFilename(name: string) {
  const dot = name.lastIndexOf(".");
  if (dot <= 0) return { stem: name, extension: "" };
  return { stem: name.slice(0, dot), extension: name.slice(dot) };
}

function parentPath(path: string) {
  const normalized = path.replaceAll("\\", "/");
  const separator = normalized.lastIndexOf("/");
  return separator >= 0 ? normalized.slice(0, separator) : "";
}

function buildRenamedName(file: FileInfo, index: number, settings: RenameSettings) {
  const { stem: originalStem, extension } = splitFilename(file.name);
  const replaced = settings.find
    ? originalStem.split(settings.find).join(settings.replace)
    : originalStem;
  let stem = `${settings.prefix}${replaced}${settings.suffix}`;

  if (settings.numbering !== "none") {
    const number = Math.max(0, Math.trunc(settings.start) + index)
      .toString()
      .padStart(Math.max(1, Math.min(6, Math.trunc(settings.padding))), "0");
    stem = settings.numbering === "prefix" ? `${number}-${stem}` : `${stem}-${number}`;
  }

  return `${stem}${extension}`;
}

export function buildRenamePreviews(files: FileInfo[], settings: RenameSettings): RenamePreview[] {
  const previews = files.map((file, index) => {
    const outputName = buildRenamedName(file, index, settings);
    const { stem } = splitFilename(outputName);
    const valid =
      stem.trim().length > 0 &&
      !outputName.includes("/") &&
      !outputName.includes(":") &&
      !outputName.includes("\\") &&
      !outputName.includes("\0") &&
      new TextEncoder().encode(outputName).length <= 255;
    return {
      inputPath: file.path,
      originalName: file.name,
      outputName,
      changed: outputName !== file.name,
      valid,
      conflict: false,
    };
  });

  const counts = new Map<string, number>();
  for (const preview of previews) {
    const key = `${parentPath(preview.inputPath).toLocaleLowerCase()}/${preview.outputName.toLocaleLowerCase()}`;
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }

  return previews.map((preview) => {
    const key = `${parentPath(preview.inputPath).toLocaleLowerCase()}/${preview.outputName.toLocaleLowerCase()}`;
    return { ...preview, conflict: (counts.get(key) ?? 0) > 1 };
  });
}

export function renamePlanIsReady(previews: RenamePreview[]) {
  return (
    previews.some((preview) => preview.changed) &&
    previews.every((preview) => preview.valid && !preview.conflict)
  );
}

export function renameRequests(previews: RenamePreview[]): RenameItemRequest[] {
  return previews.map((preview) => ({
    inputPath: preview.inputPath,
    outputName: preview.outputName,
  }));
}
