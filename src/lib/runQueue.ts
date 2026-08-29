import { useAppStore } from "../store/useAppStore";
import type { ConversionError, ConversionResult, FileInfo } from "../types";

type QueueExecutor = (file: FileInfo, jobId: string) => Promise<ConversionResult>;

function normalizeError(error: unknown): ConversionError {
  return typeof error === "object" && error !== null && "kind" in error
    ? (error as ConversionError)
    : { kind: "ProcessFailed", detail: { message: String(error) } };
}

export async function runQueue(
  files: FileInfo[],
  execute: QueueExecutor,
  requestedPaths?: string[],
) {
  const requested = new Set(requestedPaths ?? files.map((file) => file.path));
  const selected = files.filter((file) => requested.has(file.path));
  if (selected.length === 0) return;

  const store = useAppStore.getState();
  store.startBatch(selected.map((file) => file.path));

  for (const file of selected) {
    const current = useAppStore.getState();
    if (current.cancelBatchRequested) {
      current.cancelRemainingItems(selected.map((item) => item.path));
      break;
    }
    if (current.queueItems[file.path]?.status === "skipped") continue;

    const jobId = crypto.randomUUID();
    current.startQueueItem(file.path, jobId);

    try {
      const result = await execute(file, jobId);
      useAppStore.getState().completeQueueItem(file.path, result);
    } catch (error: unknown) {
      const normalized = normalizeError(error);
      useAppStore
        .getState()
        .failQueueItem(
          file.path,
          normalized,
          normalized.kind === "Cancelled" ? "cancelled" : "failed",
        );
    }

    const latest = useAppStore.getState();
    if (latest.cancelBatchRequested) {
      latest.cancelRemainingItems(selected.map((item) => item.path));
      break;
    }
  }

  useAppStore.getState().finishBatch();
}
