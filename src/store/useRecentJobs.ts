import { create } from "zustand";
import type { Operation, RecentJob, RecentJobItem } from "../types";
import { isActiveOperation, JOB_OPERATIONS } from "../lib/operations.ts";
import { normalizeRecentJobSetup } from "../lib/recentJobSetup.ts";

const RECENT_JOBS_KEY = "convertkit.recentJobs.v1";
const RECENT_JOBS_RECORDING_KEY = "convertkit.recentJobsRecording.v1";
const MAX_RECENT_JOBS = 12;
const MAX_RECENT_JOB_ITEMS = 100;
const MAX_PATH_LENGTH = 4_096;
const MAX_NAME_LENGTH = 512;
const RECENT_JOB_OPERATIONS = new Set<Operation>(JOB_OPERATIONS.filter(isActiveOperation));
const RECENT_ITEM_STATUSES = new Set(["completed", "failed", "skipped", "cancelled"]);
const ERROR_KINDS = new Set([
  "MissingDependency",
  "UnsupportedConversion",
  "InputNotFound",
  "ProcessFailed",
  "Cancelled",
  "Timeout",
  "OutputMissing",
  "OutputConflict",
  "DiskFull",
]);

function normalizeLegacyOperation(value: unknown): unknown {
  if (!value || typeof value !== "object") return value;
  const job = value as Record<string, unknown>;
  if (job.operation === "createZip") return { ...job, operation: "createArchive" };
  if (job.operation === "extractZip") return { ...job, operation: "extractArchive" };
  return value;
}

function isRecentItem(value: unknown): value is RecentJobItem {
  if (!value || typeof value !== "object") return false;
  const item = value as Partial<RecentJobItem>;
  const status = item.status ?? "completed";
  const outputPathIsValid =
    typeof item.outputPath === "string" &&
    item.outputPath.length > 0 &&
    item.outputPath.length <= MAX_PATH_LENGTH;
  const errorKindIsValid =
    item.errorKind === undefined ||
    (typeof item.errorKind === "string" && ERROR_KINDS.has(item.errorKind));
  return (
    typeof item.inputPath === "string" &&
    item.inputPath.length > 0 &&
    item.inputPath.length <= MAX_PATH_LENGTH &&
    typeof item.inputName === "string" &&
    item.inputName.length > 0 &&
    item.inputName.length <= MAX_NAME_LENGTH &&
    RECENT_ITEM_STATUSES.has(status) &&
    (status === "completed" ? outputPathIsValid : item.outputPath === undefined) &&
    errorKindIsValid &&
    (status === "failed" ? item.errorKind !== undefined : true) &&
    (status === "cancelled" ? item.errorKind === "Cancelled" : true) &&
    (status === "completed" || status === "skipped" ? item.errorKind === undefined : true) &&
    (item.outputPaths === undefined ||
      (Array.isArray(item.outputPaths) &&
        item.outputPaths.length > 0 &&
        item.outputPaths.length <= MAX_RECENT_JOB_ITEMS &&
        item.outputPaths.every(
          (path) => typeof path === "string" && path.length > 0 && path.length <= MAX_PATH_LENGTH,
        ))) &&
    (status === "completed" || item.outputPaths === undefined)
  );
}

function isRecentJob(value: unknown): value is RecentJob {
  if (!value || typeof value !== "object") return false;
  const job = value as Partial<RecentJob>;
  const operationIsValid =
    typeof job.operation === "string" && RECENT_JOB_OPERATIONS.has(job.operation as Operation);
  const itemsAreValid =
    Array.isArray(job.items) &&
    job.items.length > 0 &&
    job.items.length <= MAX_RECENT_JOB_ITEMS &&
    job.items.every(isRecentItem);
  return (
    typeof job.id === "string" &&
    job.id.length > 0 &&
    job.id.length <= 100 &&
    operationIsValid &&
    typeof job.completedAt === "number" &&
    Number.isFinite(job.completedAt) &&
    job.completedAt >= 0 &&
    (job.undoManifest === undefined ||
      job.undoManifest === null ||
      (typeof job.undoManifest === "string" &&
        job.undoManifest.length > 0 &&
        job.undoManifest.length <= MAX_PATH_LENGTH)) &&
    itemsAreValid
  );
}

export function parseRecentJobs(serialized: string | null): RecentJob[] {
  if (!serialized) return [];
  try {
    const stored = JSON.parse(serialized) as unknown;
    if (!Array.isArray(stored)) return [];
    return stored
      .map(normalizeLegacyOperation)
      .filter(isRecentJob)
      .map((job) => {
        const setup = normalizeRecentJobSetup(
          job.operation,
          job.setup,
          new Set(job.items.map((item) => item.inputPath)),
        );
        if (!setup) {
          const legacy = { ...job };
          delete legacy.setup;
          return legacy;
        }
        return { ...job, setup };
      })
      .slice(0, MAX_RECENT_JOBS);
  } catch {
    return [];
  }
}

export function parseRecentJobsRecording(serialized: string | null): boolean {
  return serialized !== "false";
}

function loadRecentJobs(): RecentJob[] {
  try {
    return parseRecentJobs(window.localStorage.getItem(RECENT_JOBS_KEY));
  } catch {
    return [];
  }
}

function loadRecentJobsRecording(): boolean {
  try {
    return parseRecentJobsRecording(window.localStorage.getItem(RECENT_JOBS_RECORDING_KEY));
  } catch {
    return true;
  }
}

function saveRecentJobs(jobs: RecentJob[]) {
  try {
    window.localStorage.setItem(RECENT_JOBS_KEY, JSON.stringify(jobs));
  } catch {
    // History is optional and must never interfere with file work.
  }
}

function saveRecentJobsRecording(recordingEnabled: boolean) {
  try {
    window.localStorage.setItem(RECENT_JOBS_RECORDING_KEY, String(recordingEnabled));
  } catch {
    // History preferences are optional and must never interfere with file work.
  }
}

interface RecentJobsStore {
  jobs: RecentJob[];
  recordingEnabled: boolean;
  addJob: (job: Omit<RecentJob, "id" | "completedAt">) => void;
  setRecordingEnabled: (enabled: boolean) => void;
  removeJob: (id: string) => void;
  removeByUndoManifest: (manifestPath: string) => void;
  clear: () => void;
}

export const useRecentJobs = create<RecentJobsStore>((set) => ({
  jobs: loadRecentJobs(),
  recordingEnabled: loadRecentJobsRecording(),
  addJob: (job) =>
    set((state) => {
      if (!state.recordingEnabled || !isActiveOperation(job.operation)) return state;
      const jobs = [
        {
          ...job,
          id: crypto.randomUUID(),
          completedAt: Date.now(),
        },
        ...state.jobs,
      ].slice(0, MAX_RECENT_JOBS);
      saveRecentJobs(jobs);
      return { jobs };
    }),
  setRecordingEnabled: (recordingEnabled) => {
    saveRecentJobsRecording(recordingEnabled);
    set({ recordingEnabled });
  },
  removeJob: (id) =>
    set((state) => {
      const jobs = state.jobs.filter((job) => job.id !== id);
      saveRecentJobs(jobs);
      return { jobs };
    }),
  removeByUndoManifest: (manifestPath) =>
    set((state) => {
      const jobs = state.jobs.filter((job) => job.undoManifest !== manifestPath);
      saveRecentJobs(jobs);
      return { jobs };
    }),
  clear: () => {
    saveRecentJobs([]);
    set({ jobs: [] });
  },
}));
