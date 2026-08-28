import { useCallback } from "react";
import { convert, cancelConversion } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import type { ConversionError } from "../types";

export function useConvert() {
  const files = useAppStore((s) => s.files);
  const outputFormats = useAppStore((s) => s.outputFormats);
  const state = useAppStore((s) => s.state);
  const jobId = useAppStore((s) => s.jobId);
  const startConversion = useAppStore((s) => s.startConversion);
  const setActiveJob = useAppStore((s) => s.setActiveJob);
  const setResults = useAppStore((s) => s.setResults);
  const setError = useAppStore((s) => s.setError);

  const doConvert = useCallback(async () => {
    if (files.length === 0 || files.some((file) => !outputFormats[file.path])) return;

    const firstJobId = crypto.randomUUID();
    startConversion(firstJobId);

    try {
      const results = [];
      for (const [index, file] of files.entries()) {
        const id = index === 0 ? firstJobId : crypto.randomUUID();
        if (index > 0) setActiveJob(id);
        results.push(await convert(file.path, outputFormats[file.path], id));
      }
      setResults(results);
    } catch (err: unknown) {
      if (typeof err === "object" && err !== null && "kind" in err) {
        setError(err as ConversionError);
      } else {
        setError({
          kind: "ProcessFailed",
          detail: { message: String(err) },
        });
      }
    }
  }, [files, outputFormats, setActiveJob, setError, setResults, startConversion]);

  const cancel = useCallback(async () => {
    if (!jobId) return;
    // Safety net: force-reset if backend doesn't respond within 5s
    const timeout = setTimeout(() => {
      if (useAppStore.getState().state === "converting") {
        useAppStore.getState().setError({ kind: "Cancelled", detail: {} });
      }
    }, 5000);
    try {
      await cancelConversion(jobId);
    } catch (err) {
      console.error("Failed to cancel conversion:", err);
      clearTimeout(timeout);
    }
  }, [jobId]);

  return {
    convert: doConvert,
    cancel,
    isConverting: state === "converting",
  };
}
