import { useCallback } from "react";
import { cancelConversion, optimizeImage } from "../lib/tauri";
import { runQueue } from "../lib/runQueue";
import { useAppStore } from "../store/useAppStore";

export function useOptimize() {
  const files = useAppStore((store) => store.files);
  const keepMetadata = useAppStore((store) => store.keepMetadata);
  const jobId = useAppStore((store) => store.jobId);

  const optimize = useCallback(
    async (paths?: string[]) => {
      if (files.length === 0) return;
      await runQueue(files, (file, jobId) => optimizeImage(file.path, keepMetadata, jobId), paths);
    },
    [files, keepMetadata],
  );

  const cancel = useCallback(async () => {
    if (jobId) await cancelConversion(jobId);
  }, [jobId]);

  return { optimize, cancel };
}
