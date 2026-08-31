import { invoke } from "@tauri-apps/api/core";
import type {
  AudioTrack,
  ChecksumManifestCreationResult,
  ChecksumManifestInput,
  ChecksumManifestVerificationResult,
  FileInfo,
  FileHashes,
  ConversionResult,
  InputCollectionResult,
  JobCapability,
  JobRequest,
  InspectionReportEntry,
  Operation,
  SubtitleTrack,
  TechnicalMetadata,
  TranscriptionModel,
  TranscriptionModelStatus,
} from "../types";

export interface FinderQuickActionRequest {
  recipeId: string;
  paths: string[];
}

interface OpenedInputs {
  paths: string[];
  quickActions: FinderQuickActionRequest[];
}

export interface FinderQuickActionStatus {
  supported: boolean;
  installed: boolean;
  displayName: string;
}

/** Whether the frontend is running inside Tauri rather than a browser preview. */
export function isTauriRuntime(): boolean {
  return "__TAURI_INTERNALS__" in window;
}

/** Cancel an in-progress conversion. */
export async function cancelConversion(jobId: string): Promise<void> {
  return invoke<void>("cancel_conversion", { jobId });
}

/** Probe a file and return its metadata. */
export async function getFileInfo(path: string): Promise<FileInfo> {
  return invoke<FileInfo>("get_file_info", { path });
}

/** Stream a file once and return its common checksums. */
export async function computeFileHashes(path: string, jobId: string): Promise<FileHashes> {
  return invoke<FileHashes>("compute_file_hashes", { path, jobId });
}

/** Create a collision-safe SHA-256 manifest for one same-root Inspect queue. */
export async function createChecksumManifest(
  inputs: ChecksumManifestInput[],
  jobId: string,
): Promise<ChecksumManifestCreationResult> {
  return invoke<ChecksumManifestCreationResult>("create_checksum_manifest", { inputs, jobId });
}

/** Verify every safe relative entry in an imported checksum manifest. */
export async function verifyChecksumManifest(
  manifestPath: string,
  jobId: string,
): Promise<ChecksumManifestVerificationResult> {
  return invoke<ChecksumManifestVerificationResult>("verify_checksum_manifest", {
    manifestPath,
    jobId,
  });
}

/** Read bounded format-aware details for PDF, video, and audio files. */
export async function getTechnicalMetadata(path: string): Promise<TechnicalMetadata> {
  return invoke<TechnicalMetadata>("get_technical_metadata", { path });
}

/** Inspect embedded audio streams for per-file extraction selection. */
export async function getAudioTracks(path: string): Promise<AudioTrack[]> {
  return invoke<AudioTrack[]>("get_audio_tracks", { path });
}

/** Inspect embedded subtitle streams and identify text tracks that can be exported. */
export async function getSubtitleTracks(path: string): Promise<SubtitleTrack[]> {
  return invoke<SubtitleTrack[]>("get_subtitle_tracks", { path });
}

/** List integrity-checked local Whisper models and their download state. */
export async function getTranscriptionModels(): Promise<TranscriptionModelStatus[]> {
  return invoke<TranscriptionModelStatus[]>("get_transcription_models");
}

/** Download and verify one managed Whisper model. */
export async function downloadTranscriptionModel(
  model: TranscriptionModel,
  jobId: string,
): Promise<TranscriptionModelStatus> {
  return invoke<TranscriptionModelStatus>("download_transcription_model", { model, jobId });
}

/** Delete one managed Whisper model. It can be downloaded again later. */
export async function deleteTranscriptionModel(
  model: TranscriptionModel,
): Promise<TranscriptionModelStatus[]> {
  return invoke<TranscriptionModelStatus[]>("delete_transcription_model", { model });
}

/** Atomically write a multi-file inspection report chosen by the user. */
export async function exportInspectionReport(
  outputPath: string,
  format: "json" | "csv",
  entries: InspectionReportEntry[],
): Promise<string> {
  return invoke<string>("export_inspection_report", { outputPath, format, entries });
}

/** Expand file and folder paths into supported, deduplicated queue items. */
export async function collectInputPaths(
  paths: string[],
  operation: Operation,
  limit: number,
  excludedPaths: string[],
): Promise<InputCollectionResult> {
  return invoke<InputCollectionResult>("collect_input_paths", {
    paths,
    operation,
    limit,
    excludedPaths,
  });
}

/** Check exact engine and dependency availability without starting work. */
export async function checkJobCapabilities(requests: JobRequest[]): Promise<JobCapability[]> {
  return invoke<JobCapability[]>("check_job_capabilities", { requests });
}

/** Execute a typed job through the shared backend command. */
export async function runJob(request: JobRequest): Promise<ConversionResult> {
  return invoke<ConversionResult>("run_job", { request });
}

/** Restore filenames from an app-managed batch rename undo record. */
export async function undoRename(manifestPath: string): Promise<string[]> {
  return invoke<string[]>("undo_rename", { manifestPath });
}

/** Open the containing folder and highlight an output item in Finder. */
export async function revealInFinder(path: string): Promise<void> {
  return invoke<void>("reveal_in_finder", { path });
}

/** Open Finder and reveal every file or directory created by one result. */
export async function revealPathsInFinder(paths: string[]): Promise<void> {
  return invoke<void>("reveal_paths_in_finder", { paths });
}

/** Read a file as a base64 data URL for thumbnail display. */
export async function readFileThumbnail(path: string): Promise<string> {
  return invoke<string>("read_file_thumbnail", { path });
}

/** Mark the frontend ready and get buffered Open With and Quick Action inputs. */
export async function getOpenedInputs(): Promise<OpenedInputs> {
  return invoke<OpenedInputs>("get_opened_inputs");
}

/** Check whether a saved recipe is installed as a Finder Quick Action. */
export async function getFinderQuickActionStatus(
  recipeId: string,
  recipeName: string,
): Promise<FinderQuickActionStatus> {
  return invoke<FinderQuickActionStatus>("get_finder_quick_action_status", {
    recipeId,
    recipeName,
  });
}

/** Install an opt-in Finder Quick Action for a saved recipe. */
export async function installFinderQuickAction(
  recipeId: string,
  recipeName: string,
): Promise<FinderQuickActionStatus> {
  return invoke<FinderQuickActionStatus>("install_finder_quick_action", {
    recipeId,
    recipeName,
  });
}

/** Remove the Finder Quick Action associated with a saved recipe. */
export async function removeFinderQuickAction(recipeId: string): Promise<void> {
  return invoke<void>("remove_finder_quick_action", { recipeId });
}

/** Save base64-encoded clipboard image data to a temp file and return its path. */
export async function saveClipboardImage(data: string, mime: string): Promise<string> {
  return invoke<string>("save_clipboard_image", { data, mime });
}
