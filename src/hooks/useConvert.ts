import { useCallback } from "react";
import { convert, cancelConversion } from "../lib/tauri";
import { runQueue } from "../lib/runQueue";
import { useAppStore } from "../store/useAppStore";

export function useConvert() {
  const files = useAppStore((s) => s.files);
  const outputFormats = useAppStore((s) => s.outputFormats);
  const state = useAppStore((s) => s.state);
  const jobId = useAppStore((s) => s.jobId);

  const doConvert = useCallback(
    async (paths?: string[]) => {
      if (files.length === 0 || files.some((file) => !outputFormats[file.path])) return;
      await runQueue(
        files,
        (file, jobId) => convert(file.path, outputFormats[file.path], jobId),
        paths,
      );
    },
    [files, outputFormats],
  );

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
