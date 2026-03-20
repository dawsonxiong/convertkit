import { invoke } from "@tauri-apps/api/core";
import type { FileInfo, ConversionResult, DependencyStatus } from "../types";

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
