import { invoke } from "@tauri-apps/api/core";
import type { FileInfo, ConversionResult, DependencyStatus } from "../types";

/** Whether the frontend is running inside Tauri rather than a browser preview. */
export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Start a conversion job. Returns the result on success. */
export async function convert(
  inputPath: string,
  outputFormat: string,
  jobId: string,
): Promise<ConversionResult> {
  return invoke<ConversionResult>("convert", {
    inputPath,
    outputFormat,
    jobId,
  });
}

/** Resize a raster image while preserving the original file. */
export async function resizeImage(
  inputPath: string,
  width: number,
  height: number,
  preserveAspect: boolean,
  jobId: string,
): Promise<ConversionResult> {
  return invoke<ConversionResult>("resize_image", {
    inputPath,
    width,
    height,
    preserveAspect,
    jobId,
  });
}

/** Optimize an image while preserving the original file. */
export async function optimizeImage(
  inputPath: string,
  keepMetadata: boolean,
  jobId: string,
): Promise<ConversionResult> {
  return invoke<ConversionResult>("optimize_image", {
    inputPath,
    keepMetadata,
    jobId,
  });
}

/** Cancel an in-progress conversion. */
export async function cancelConversion(jobId: string): Promise<void> {
  return invoke<void>("cancel_conversion", { jobId });
}

/** Check which external tools (ffmpeg, pandoc, etc.) are available. */
export async function checkDependencies(): Promise<DependencyStatus[]> {
  return invoke<DependencyStatus[]>("check_dependencies");
}

/** Probe a file and return its metadata. */
export async function getFileInfo(path: string): Promise<FileInfo> {
  return invoke<FileInfo>("get_file_info", { path });
}

/** Open the containing folder and highlight the file in Finder. */
export async function revealInFinder(path: string): Promise<void> {
  return invoke<void>("reveal_in_finder", { path });
}

/** Read a file as a base64 data URL for thumbnail display. */
export async function readFileThumbnail(path: string): Promise<string> {
  return invoke<string>("read_file_thumbnail", { path });
}

/** Get a file that was opened via Finder "Open With" before the frontend loaded. */
export async function getOpenedFile(): Promise<string | null> {
  return invoke<string | null>("get_opened_file");
}

/** Save base64-encoded clipboard image data to a temp file and return its path. */
export async function saveClipboardImage(data: string, mime: string): Promise<string> {
  return invoke<string>("save_clipboard_image", { data, mime });
}
