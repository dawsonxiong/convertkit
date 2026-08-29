import { useCallback } from "react";
import { cancelConversion, optimizeImage } from "../lib/tauri";
import { runQueue } from "../lib/runQueue";
import { useAppStore } from "../store/useAppStore";
import { useOutputOptions } from "./useOutputOptions";

export function useOptimize() {
  const files = useAppStore((store) => store.files);
  const keepMetadata = useAppStore((store) => store.keepMetadata);
  const jobId = useAppStore((store) => store.jobId);
  const outputOptions = useOutputOptions("optimize");

  const optimize = useCallback(
    async (paths?: string[]) => {
      if (files.length === 0) return;
      await runQueue(
        files,
        (file, jobId) => optimizeImage(file.path, keepMetadata, jobId, outputOptions),
        paths,
      );
    },
    [files, keepMetadata, outputOptions],
  );

  const cancel = useCallback(async () => {
    if (jobId) await cancelConversion(jobId);
  }, [jobId]);

  return { optimize, cancel };
}
