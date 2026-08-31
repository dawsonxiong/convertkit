import { useEffect, useMemo, useState } from "react";
import { checkJobCapabilities } from "../lib/tauri";
import { isValidPageSelection } from "../lib/pdfPages";
import { useAppStore } from "../store/useAppStore";
import type { JobCapability, Operation } from "../types";
import { useJobRequestBuilder } from "./useJobRequestBuilder";
import { buildRenamePreviews, renamePlanIsReady } from "../lib/rename";
import { useTranscriptionModels } from "../store/useTranscriptionModels";
import { videoCompressionSettingsAreReady } from "../lib/videoCompression";
import { pdfCompressionSettingsAreReady } from "../lib/pdfCompression";
import { imageOptimizationSettingsAreReady } from "../lib/imageOptimization";

export function useJobCapabilities(operation: Operation) {
  const state = useAppStore((store) => store.state);
  const files = useAppStore((store) => store.files);
  const outputFormats = useAppStore((store) => store.outputFormats);
  const width = useAppStore((store) => store.resizeWidth);
  const height = useAppStore((store) => store.resizeHeight);
  const imageExportPresets = useAppStore((store) => store.imageExportPresets);
  const imageOptimizationGoal = useAppStore((store) => store.imageOptimizationGoal);
  const imageTargetSizeBytes = useAppStore((store) => store.imageTargetSizeBytes);
  const pdfSplitMode = useAppStore((store) => store.pdfSplitMode);
  const pdfPageSelection = useAppStore((store) => store.pdfPageSelection);
  const renameSettings = useAppStore((store) => store.renameSettings);
  const audioTracks = useAppStore((store) => store.audioTracks);
  const audioTrackIndexes = useAppStore((store) => store.audioTrackIndexes);
  const subtitleTracks = useAppStore((store) => store.subtitleTracks);
  const subtitleTrackIndexes = useAppStore((store) => store.subtitleTrackIndexes);
  const videoEncodingPreset = useAppStore((store) => store.videoEncodingPreset);
  const videoCompressionGoal = useAppStore((store) => store.videoCompressionGoal);
  const videoTargetSizeBytes = useAppStore((store) => store.videoTargetSizeBytes);
  const pdfCompressionGoal = useAppStore((store) => store.pdfCompressionGoal);
  const pdfTargetSizeBytes = useAppStore((store) => store.pdfTargetSizeBytes);
  const transcriptionModels = useTranscriptionModels((store) => store.models);
  const buildRequest = useJobRequestBuilder(operation);
  const [capabilities, setCapabilities] = useState<JobCapability[]>([]);
  const [checking, setChecking] = useState(false);
  const settingsReady =
    operation === "convert"
      ? files.every((file) => Boolean(outputFormats[file.path]))
      : operation === "extractAudio"
        ? files.every((file) => {
            const selected = audioTrackIndexes[file.path];
            return audioTracks[file.path]?.some((track) => track.streamIndex === selected);
          })
        : operation === "extractSubtitles"
          ? files.every((file) => {
              const selected = subtitleTrackIndexes[file.path];
              return subtitleTracks[file.path]?.some(
                (track) => track.supported && track.streamIndex === selected,
              );
            })
          : operation === "resize"
            ? Boolean(width && height)
            : operation === "optimize"
              ? imageOptimizationSettingsAreReady(
                  imageOptimizationGoal,
                  imageTargetSizeBytes,
                  files,
                )
              : operation === "encodeVideo"
                ? videoCompressionSettingsAreReady(
                    videoEncodingPreset,
                    videoCompressionGoal,
                    videoTargetSizeBytes,
                  )
                : operation === "exportImages"
                  ? imageExportPresets.length > 0
                  : operation === "compressPdf"
                    ? pdfCompressionSettingsAreReady(pdfCompressionGoal, pdfTargetSizeBytes, files)
                    : operation === "mergePdf"
                      ? files.length >= 2
                      : (operation === "splitPdf" || operation === "exportPdfPages") &&
                          pdfSplitMode === "extract"
                        ? isValidPageSelection(pdfPageSelection)
                        : operation === "rename"
                          ? renamePlanIsReady(buildRenamePreviews(files, renameSettings))
                          : true;

  useEffect(() => {
    if (state !== "loaded" || files.length === 0 || !settingsReady) {
      setCapabilities([]);
      setChecking(false);
      return;
    }

    let active = true;
    setCapabilities([]);
    setChecking(true);
    const requests =
      operation === "mergePdf" || operation === "createArchive" || operation === "rename"
        ? [buildRequest(files[0])]
        : files.map((file) => buildRequest(file));
    checkJobCapabilities(requests)
      .then((result) => {
        if (active) setCapabilities(result);
      })
      .catch((error) => {
        console.error("Capability check failed:", error);
        if (active) setCapabilities([]);
      })
      .finally(() => {
        if (active) setChecking(false);
      });

    return () => {
      active = false;
    };
  }, [
    buildRequest,
    files,
    imageExportPresets,
    imageOptimizationGoal,
    imageTargetSizeBytes,
    operation,
    pdfPageSelection,
    pdfSplitMode,
    pdfCompressionGoal,
    pdfTargetSizeBytes,
    renameSettings,
    settingsReady,
    state,
    transcriptionModels,
    videoCompressionGoal,
    videoEncodingPreset,
    videoTargetSizeBytes,
  ]);

  const unavailable = useMemo(
    () => capabilities.find((capability) => !capability.available) ?? null,
    [capabilities],
  );

  return { capabilities, unavailable, checking };
}
