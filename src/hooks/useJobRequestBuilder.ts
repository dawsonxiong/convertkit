import { useCallback } from "react";
import type { FileInfo, JobRequest, Operation } from "../types";
import { useAppStore } from "../store/useAppStore";
import { archiveEntryRequest } from "../lib/archive";
import { useOutputOptions } from "./useOutputOptions";
import { buildRenamePreviews, renameRequests } from "../lib/rename";
import { effectiveVideoCompressionGoal, videoTargetSizeForRequest } from "../lib/videoCompression";
import { pdfTargetSizeForRequest } from "../lib/pdfCompression";
import { imageTargetSizeForRequest } from "../lib/imageOptimization";

export function useJobRequestBuilder(operation: Operation) {
  const files = useAppStore((state) => state.files);
  const outputFormats = useAppStore((state) => state.outputFormats);
  const width = useAppStore((state) => state.resizeWidth);
  const height = useAppStore((state) => state.resizeHeight);
  const preserveAspect = useAppStore((state) => state.preserveAspect);
  const keepMetadata = useAppStore((state) => state.keepMetadata);
  const imageOptimizationGoal = useAppStore((state) => state.imageOptimizationGoal);
  const imageTargetSizeBytes = useAppStore((state) => state.imageTargetSizeBytes);
  const imageExportPresets = useAppStore((state) => state.imageExportPresets);
  const audioOutputFormat = useAppStore((state) => state.audioOutputFormat);
  const audioTracks = useAppStore((state) => state.audioTracks);
  const audioTrackIndexes = useAppStore((state) => state.audioTrackIndexes);
  const transcriptionModel = useAppStore((state) => state.transcriptionModel);
  const transcriptionLanguage = useAppStore((state) => state.transcriptionLanguage);
  const transcriptionOutputFormat = useAppStore((state) => state.transcriptionOutputFormat);
  const ocrOutputFormat = useAppStore((state) => state.ocrOutputFormat);
  const videoEncodingPreset = useAppStore((state) => state.videoEncodingPreset);
  const videoResolution = useAppStore((state) => state.videoResolution);
  const videoQuality = useAppStore((state) => state.videoQuality);
  const videoCompressionGoal = useAppStore((state) => state.videoCompressionGoal);
  const videoTargetSizeBytes = useAppStore((state) => state.videoTargetSizeBytes);
  const audioCompressionPreset = useAppStore((state) => state.audioCompressionPreset);
  const subtitleOutputFormat = useAppStore((state) => state.subtitleOutputFormat);
  const subtitleTracks = useAppStore((state) => state.subtitleTracks);
  const subtitleTrackIndexes = useAppStore((state) => state.subtitleTrackIndexes);
  const thumbnailMode = useAppStore((state) => state.thumbnailMode);
  const thumbnailOutputFormat = useAppStore((state) => state.thumbnailOutputFormat);
  const pdfSplitMode = useAppStore((state) => state.pdfSplitMode);
  const pdfPageSelection = useAppStore((state) => state.pdfPageSelection);
  const pdfPageImageFormat = useAppStore((state) => state.pdfPageImageFormat);
  const pdfPageImageResolution = useAppStore((state) => state.pdfPageImageResolution);
  const pdfCompressionPreset = useAppStore((state) => state.pdfCompressionPreset);
  const pdfCompressionGoal = useAppStore((state) => state.pdfCompressionGoal);
  const pdfTargetSizeBytes = useAppStore((state) => state.pdfTargetSizeBytes);
  const archiveFormat = useAppStore((state) => state.archiveFormat);
  const archivePassword = useAppStore((state) => state.archivePassword);
  const extractArchivePasswords = useAppStore((state) => state.extractArchivePasswords);
  const renameSettings = useAppStore((state) => state.renameSettings);
  const getOutputOptions = useOutputOptions(operation);

  return useCallback(
    (file: FileInfo, jobId: string | null = null): JobRequest => {
      const base = {
        inputPath: file.path,
        jobId,
        outputOptions: getOutputOptions(file),
      };

      if (operation === "resize") {
        return {
          ...base,
          operation,
          width: width ?? 0,
          height: height ?? 0,
          preserveAspect,
        };
      }
      if (operation === "optimize") {
        return {
          ...base,
          operation,
          keepMetadata,
          compressionGoal: imageOptimizationGoal,
          targetSizeBytes: imageTargetSizeForRequest(imageOptimizationGoal, imageTargetSizeBytes),
        };
      }
      if (operation === "exportImages") {
        return { ...base, operation, presets: imageExportPresets };
      }
      if (operation === "removeMetadata") {
        return { ...base, operation };
      }
      if (operation === "extractAudio") {
        const streamIndex = audioTrackIndexes[file.path] ?? 0;
        const streamCodec =
          audioTracks[file.path]?.find((track) => track.streamIndex === streamIndex)?.codec ?? "";
        return { ...base, operation, streamIndex, streamCodec, outputFormat: audioOutputFormat };
      }
      if (operation === "transcribe") {
        return {
          ...base,
          operation,
          model: transcriptionModel,
          language: transcriptionLanguage,
          outputFormat: transcriptionOutputFormat,
        };
      }
      if (operation === "encodeVideo") {
        return {
          ...base,
          operation,
          preset: videoEncodingPreset,
          resolution: videoResolution,
          quality: videoQuality,
          compressionGoal: effectiveVideoCompressionGoal(videoEncodingPreset, videoCompressionGoal),
          targetSizeBytes: videoTargetSizeForRequest(
            videoEncodingPreset,
            videoCompressionGoal,
            videoTargetSizeBytes,
          ),
        };
      }
      if (operation === "removeAudio") {
        return { ...base, operation };
      }
      if (operation === "compressAudio") {
        return { ...base, operation, preset: audioCompressionPreset };
      }
      if (operation === "extractSubtitles") {
        const streamIndex = subtitleTrackIndexes[file.path] ?? 0;
        const streamCodec =
          subtitleTracks[file.path]?.find((track) => track.streamIndex === streamIndex)?.codec ??
          "";
        return {
          ...base,
          operation,
          streamIndex,
          streamCodec,
          outputFormat: subtitleOutputFormat,
        };
      }
      if (operation === "generateThumbnails") {
        return {
          ...base,
          operation,
          mode: thumbnailMode,
          outputFormat: thumbnailOutputFormat,
        };
      }
      if (operation === "extractText") {
        return { ...base, operation };
      }
      if (operation === "recognizeText") {
        return { ...base, operation, outputFormat: ocrOutputFormat };
      }
      if (operation === "mergePdf") {
        return {
          operation,
          inputPaths: files.map((item) => item.path),
          jobId,
          outputOptions: getOutputOptions(files[0] ?? file),
        };
      }
      if (operation === "createArchive") {
        return {
          operation,
          entries: files.map(archiveEntryRequest),
          format: archiveFormat,
          password: archiveFormat === "sevenZ" && archivePassword ? archivePassword : null,
          jobId,
          outputOptions: getOutputOptions(files[0] ?? file),
        };
      }
      if (operation === "splitPdf") {
        return {
          ...base,
          operation,
          mode: pdfSplitMode,
          pageSelection: pdfPageSelection.trim(),
        };
      }
      if (operation === "exportPdfPages") {
        return {
          ...base,
          operation,
          mode: pdfSplitMode,
          pageSelection: pdfPageSelection.trim(),
          outputFormat: pdfPageImageFormat,
          resolution: pdfPageImageResolution,
        };
      }
      if (operation === "extractArchive") {
        return {
          ...base,
          operation,
          password: extractArchivePasswords[file.path] || null,
        };
      }
      if (operation === "compressPdf") {
        return {
          ...base,
          operation,
          preset: pdfCompressionPreset,
          compressionGoal: pdfCompressionGoal,
          targetSizeBytes: pdfTargetSizeForRequest(pdfCompressionGoal, pdfTargetSizeBytes),
        };
      }
      if (operation === "rename") {
        return {
          operation,
          items: renameRequests(buildRenamePreviews(files, renameSettings)),
          jobId,
        };
      }
      if (operation === "inspect") {
        throw new Error(`${operation} does not create conversion jobs`);
      }
      return {
        ...base,
        operation,
        outputFormat: outputFormats[file.path] ?? "",
      };
    },
    [
      files,
      archiveFormat,
      archivePassword,
      audioOutputFormat,
      audioCompressionPreset,
      audioTracks,
      audioTrackIndexes,
      transcriptionLanguage,
      transcriptionModel,
      transcriptionOutputFormat,
      ocrOutputFormat,
      videoEncodingPreset,
      videoCompressionGoal,
      videoQuality,
      videoResolution,
      videoTargetSizeBytes,
      subtitleOutputFormat,
      subtitleTrackIndexes,
      subtitleTracks,
      thumbnailMode,
      thumbnailOutputFormat,
      getOutputOptions,
      height,
      keepMetadata,
      imageOptimizationGoal,
      imageTargetSizeBytes,
      imageExportPresets,
      extractArchivePasswords,
      operation,
      outputFormats,
      pdfPageSelection,
      pdfPageImageFormat,
      pdfPageImageResolution,
      pdfCompressionPreset,
      pdfCompressionGoal,
      pdfTargetSizeBytes,
      pdfSplitMode,
      preserveAspect,
      renameSettings,
      width,
    ],
  );
}
