import { useCallback } from "react";
import { collectInputPaths, getFileInfo, undoRename } from "../lib/tauri.ts";
import { MAX_QUEUE_ITEMS, useAppStore } from "../store/useAppStore.ts";
import { useRecentJobs } from "../store/useRecentJobs.ts";
import type { Operation, RecentJob } from "../types/index.ts";
import { useAddPaths } from "./useAddPaths.ts";

type InputCollector = typeof collectInputPaths;

export async function loadRecentJobSetup(
  job: RecentJob,
  collect: InputCollector = collectInputPaths,
): Promise<boolean> {
  if (!job.setup) return false;
  const token = crypto.randomUUID();
  const store = useAppStore.getState();
  if (!store.claimRecentJobLoad(job.operation, token)) {
    store.setRejection("Wait for the current workspace task to finish");
    return false;
  }

  try {
    const result = await collect(
      job.items.map((item) => item.inputPath),
      job.operation,
      MAX_QUEUE_ITEMS,
      [],
    );
    if (result.files.length === 0) {
      if (useAppStore.getState().releaseWorkspaceClaim(token)) {
        useAppStore
          .getState()
          .setRejection("The source files for that job are no longer available");
      }
      return false;
    }
    if (
      !useAppStore.getState().commitRecentJobLoad(job.operation, token, result.files, job.setup)
    ) {
      return false;
    }
    if (result.skippedCount > 0 || result.truncated) {
      useAppStore
        .getState()
        .setRejection(
          `${result.files.length} ${result.files.length === 1 ? "source" : "sources"} loaded. ${result.skippedCount} unavailable.`,
        );
    }
    return true;
  } catch (error) {
    console.error("Could not load recent job:", error);
    if (useAppStore.getState().releaseWorkspaceClaim(token)) {
      useAppStore.getState().setRejection("That job could not be loaded");
    }
    return false;
  }
}

export function useRecentJobActions(onOpen: (operation: Operation) => void) {
  const addPaths = useAddPaths();
  const removeJob = useRecentJobs((state) => state.removeJob);
  const setRejection = useAppStore((state) => state.setRejection);

  const reopen = useCallback(
    async (job: RecentJob) => {
      onOpen(job.operation);
      await addPaths(
        job.items.map((item) => item.inputPath),
        job.operation,
      );
    },
    [addPaths, onOpen],
  );

  const load = useCallback(
    async (job: RecentJob) => {
      if (await loadRecentJobSetup(job)) onOpen(job.operation);
    },
    [onOpen],
  );

  const undo = useCallback(
    async (job: RecentJob) => {
      if (!job.undoManifest) return;
      try {
        const paths = await undoRename(job.undoManifest);
        const restored = await Promise.all(paths.map((path) => getFileInfo(path)));
        removeJob(job.id);
        onOpen("rename");
        useAppStore.getState().reset();
        useAppStore.getState().addFiles(restored);
        setRejection("Batch rename undone");
      } catch (error) {
        console.error("Could not undo recent batch rename:", error);
        setRejection("The batch rename could not be undone");
      }
    },
    [onOpen, removeJob, setRejection],
  );

  return { reopen, load, undo };
}
