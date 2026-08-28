import { useCallback } from "react";
import { resizeImage, cancelConversion } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import type { ConversionError } from "../types";

export function useResize() {
  const files = useAppStore((s) => s.files);
  const width = useAppStore((s) => s.resizeWidth);
  const height = useAppStore((s) => s.resizeHeight);
  const preserveAspect = useAppStore((s) => s.preserveAspect);
  const jobId = useAppStore((s) => s.jobId);
  const startConversion = useAppStore((s) => s.startConversion);
  const setActiveJob = useAppStore((s) => s.setActiveJob);
  const setResults = useAppStore((s) => s.setResults);
  const setError = useAppStore((s) => s.setError);

  const resize = useCallback(async () => {
    if (files.length === 0 || !width || !height) return;
    const firstJobId = crypto.randomUUID();
    startConversion(firstJobId);
    try {
      const results = [];
      for (const [index, file] of files.entries()) {
        const id = index === 0 ? firstJobId : crypto.randomUUID();
        if (index > 0) setActiveJob(id);
        results.push(await resizeImage(file.path, width, height, preserveAspect, id));
      }
      setResults(results);
    } catch (err: unknown) {
      setError(
        typeof err === "object" && err !== null && "kind" in err
          ? (err as ConversionError)
          : { kind: "ProcessFailed", detail: { message: String(err) } },
      );
    }
  }, [files, height, preserveAspect, setActiveJob, setError, setResults, startConversion, width]);

  const cancel = useCallback(async () => {
    if (jobId) await cancelConversion(jobId);
  }, [jobId]);

  return { resize, cancel };
}
