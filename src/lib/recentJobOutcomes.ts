import type { FileInfo, QueueItemState, RecentJobItem } from "../types";
import { outputPathsForResult } from "./outputHandoff.ts";

export function buildRecentJobItems(
  files: FileInfo[],
  queueItems: Record<string, QueueItemState>,
): RecentJobItem[] {
  const recentItems: RecentJobItem[] = [];

  for (const file of files) {
    const item = queueItems[file.path];
    if (!item || item.status === "pending" || item.status === "running") continue;

    if (item.status === "completed") {
      if (!item.result?.output_path) continue;
      const outputPaths = outputPathsForResult(item.result);
      recentItems.push({
        inputPath: file.path,
        inputName: file.name,
        outputPath: item.result.output_path,
        outputPaths,
        status: "completed",
      });
      continue;
    }

    recentItems.push({
      inputPath: file.path,
      inputName: file.name,
      status: item.status,
      errorKind:
        item.status === "cancelled"
          ? "Cancelled"
          : item.status === "failed"
            ? (item.error?.kind ?? "ProcessFailed")
            : undefined,
    });
  }

  return recentItems;
}
