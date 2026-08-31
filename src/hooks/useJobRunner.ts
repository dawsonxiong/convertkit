import { useCallback } from "react";
import { confirm } from "@tauri-apps/plugin-dialog";
import { cancelConversion } from "../lib/tauri";
import { runQueue } from "../lib/runQueue";
import { isValidPageSelection } from "../lib/pdfPages";
import { useAppStore } from "../store/useAppStore";
import type { Operation } from "../types";
import { useJobRequestBuilder } from "./useJobRequestBuilder";
import { buildRenamePreviews, renamePlanIsReady } from "../lib/rename";
import { cancelJobWithFallback } from "../lib/jobCancellation";
import { gzipCreationInputError } from "../lib/archive";

export function useJobRunner(operation: Operation) {
  const files = useAppStore((state) => state.files);
  const outputFormats = useAppStore((state) => state.outputFormats);
  const width = useAppStore((state) => state.resizeWidth);
  const height = useAppStore((state) => state.resizeHeight);
  const imageExportPresets = useAppStore((state) => state.imageExportPresets);
  const pdfSplitMode = useAppStore((store) => store.pdfSplitMode);
  const pdfPageSelection = useAppStore((store) => store.pdfPageSelection);
  const renameSettings = useAppStore((store) => store.renameSettings);
  const archiveFormat = useAppStore((store) => store.archiveFormat);
  const state = useAppStore((store) => store.state);
  const jobId = useAppStore((store) => store.jobId);
  const buildRequest = useJobRequestBuilder(operation);

  const run = useCallback(
    async (paths?: string[]) => {
      if (useAppStore.getState().state === "converting") return;
      if (files.length === 0) return;
      if (operation === "inspect") return;
      if (operation === "convert" && files.some((file) => !outputFormats[file.path])) return;
      if (operation === "resize" && (!width || !height)) return;
      if (operation === "exportImages" && imageExportPresets.length === 0) return;
      if (operation === "mergePdf" && files.length < 2) return;
      if (operation === "createArchive" && archiveFormat === "gzip") {
        const message = gzipCreationInputError(files);
        if (message) {
          useAppStore.getState().setRejection(message);
          return;
        }
      }
      if (
        (operation === "splitPdf" || operation === "exportPdfPages") &&
        pdfSplitMode === "extract" &&
        !isValidPageSelection(pdfPageSelection)
      )
        return;
      if (operation === "rename") {
        const previews = buildRenamePreviews(files, renameSettings);
        if (!renamePlanIsReady(previews)) return;
      }
      await runQueue(files, buildRequest, paths, {
        authorize:
          operation === "rename"
            ? () =>
                confirm(
                  `Rename ${files.length} ${files.length === 1 ? "file" : "files"}? You can undo this batch from the result screen.`,
                  {
                    title: "Confirm batch rename",
                    kind: "warning",
                    okLabel: "Rename",
                    cancelLabel: "Cancel",
                  },
                )
            : undefined,
      });
    },
    [
      buildRequest,
      archiveFormat,
      files,
      height,
      imageExportPresets,
      operation,
      outputFormats,
      pdfPageSelection,
      pdfSplitMode,
      renameSettings,
      width,
    ],
  );

  const cancel = useCallback(async () => {
    if (!jobId) return;
    try {
      await cancelJobWithFallback(jobId, cancelConversion, (targetJobId) => {
        useAppStore.getState().setJobError(targetJobId, { kind: "Cancelled", detail: {} });
      });
    } catch (error) {
      console.error("Failed to cancel job:", error);
    }
  }, [jobId]);

  return { run, cancel, isRunning: state === "converting", buildRequest };
}
