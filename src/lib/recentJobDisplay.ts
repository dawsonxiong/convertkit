import type { RecentJob } from "../types/index.ts";
import { uniqueOutputPaths } from "./outputHandoff.ts";

export function relativeJobTime(timestamp: number, now = Date.now()): string {
  const elapsedMinutes = Math.max(0, Math.floor((now - timestamp) / 60_000));
  if (elapsedMinutes < 1) return "Now";
  if (elapsedMinutes < 60) return `${elapsedMinutes}m`;
  const hours = Math.floor(elapsedMinutes / 60);
  if (hours < 24) return `${hours}h`;
  return `${Math.floor(hours / 24)}d`;
}

export function outputPathsForRecentJob(job: RecentJob): string[] {
  return uniqueOutputPaths(
    job.items.flatMap((item) => [item.outputPath, ...(item.outputPaths ?? [])]),
  );
}

export function recentJobIssue(job: RecentJob): string {
  const failed = job.items.filter((item) => item.status === "failed").length;
  const cancelled = job.items.filter((item) => item.status === "cancelled").length;
  const skipped = job.items.filter((item) => item.status === "skipped").length;

  if (failed === job.items.length) return "Failed";
  if (cancelled === job.items.length) return "Cancelled";
  return [
    failed > 0 ? `${failed} failed` : null,
    cancelled > 0 ? `${cancelled} cancelled` : null,
    skipped > 0 ? `${skipped} skipped` : null,
  ]
    .filter(Boolean)
    .join(", ");
}

export function recentJobOutcome(job: RecentJob): string {
  const issue = recentJobIssue(job);
  if (issue) return issue;
  const completed = job.items.filter((item) => (item.status ?? "completed") === "completed").length;
  return completed === 1 ? "Completed" : `${completed} completed`;
}
