import { useCallback } from "react";
import { convert, cancelConversion } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import type { ConversionError } from "../types";

export function useConvert() {
  const file = useAppStore((s) => s.file);
  const outputFormat = useAppStore((s) => s.outputFormat);
  const state = useAppStore((s) => s.state);
  const jobId = useAppStore((s) => s.jobId);
  const startConversion = useAppStore((s) => s.startConversion);
  const setResult = useAppStore((s) => s.setResult);
  const setError = useAppStore((s) => s.setError);

  const doConvert = useCallback(async () => {
    if (!file || !outputFormat) return;

    const id = crypto.randomUUID();
    startConversion(id);

    try {
      const result = await convert(file.path, outputFormat, id);
      setResult(result);
    } catch (err: unknown) {
      const convErr = err as ConversionError;
      setError(
        convErr?.kind
          ? convErr
          : {
              kind: "ProcessFailed",
              detail: { message: String(err) },
            },
      );
    }
  }, [file, outputFormat, startConversion, setResult, setError]);

  const cancel = useCallback(async () => {
    if (!jobId) return;
    try {
      await cancelConversion(jobId);
    } catch (err) {
      console.error("Failed to cancel conversion:", err);
    }
  }, [jobId]);

  return {
    convert: doConvert,
    cancel,
    isConverting: state === "converting",
  };
}
