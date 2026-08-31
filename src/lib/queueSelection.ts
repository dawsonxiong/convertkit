import type { FileInfo, QueueItemState } from "../types";

export function unfinishedQueuePaths(
  files: Pick<FileInfo, "path">[],
  queueItems: Record<string, QueueItemState>,
) {
  return files
    .filter((file) => queueItems[file.path]?.status !== "completed")
    .map((file) => file.path);
}
