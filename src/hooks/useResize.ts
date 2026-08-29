import { useCallback } from "react";
import { resizeImage, cancelConversion } from "../lib/tauri";
import { runQueue } from "../lib/runQueue";
import { useAppStore } from "../store/useAppStore";
import { useOutputOptions } from "./useOutputOptions";

export function useResize() {
  const files = useAppStore((s) => s.files);
  const width = useAppStore((s) => s.resizeWidth);
  const height = useAppStore((s) => s.resizeHeight);
  const preserveAspect = useAppStore((s) => s.preserveAspect);
  const jobId = useAppStore((s) => s.jobId);
  const outputOptions = useOutputOptions("resize");

  const resize = useCallback(
    async (paths?: string[]) => {
      if (files.length === 0 || !width || !height) return;
      await runQueue(
        files,
        (file, jobId) =>
          resizeImage(file.path, width, height, preserveAspect, jobId, outputOptions),
        paths,
      );
    },
    [files, height, outputOptions, preserveAspect, width],
  );

  const cancel = useCallback(async () => {
    if (jobId) await cancelConversion(jobId);
  }, [jobId]);

  return { resize, cancel };
}
