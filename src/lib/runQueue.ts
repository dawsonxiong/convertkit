import { useAppStore } from "../store/useAppStore.ts";
import { useRecentJobs } from "../store/useRecentJobs.ts";
import { checkJobCapabilities, runJob } from "./tauri.ts";
import { normalizeConversionError } from "./errors.ts";
import { buildRecentJobItems } from "./recentJobOutcomes.ts";
import { captureRecentJobSetup } from "./recentJobSetup.ts";
import { unfinishedQueuePaths } from "./queueSelection.ts";
import type { FileInfo, JobRequest } from "../types/index.ts";

type QueueRequestBuilder = (file: FileInfo, jobId?: string | null) => JobRequest;

interface QueueRunnerDependencies {
  checkCapabilities: typeof checkJobCapabilities;
  executeJob: typeof runJob;
  createId: () => string;
}

interface QueueRunOptions {
  authorize?: () => Promise<boolean>;
  dependencies?: QueueRunnerDependencies;
}

const DEFAULT_QUEUE_RUNNER_DEPENDENCIES: QueueRunnerDependencies = {
  checkCapabilities: checkJobCapabilities,
  executeJob: runJob,
  createId: () => crypto.randomUUID(),
};

function runClaimIsCurrent(operation: JobRequest["operation"], token: string) {
  const state = useAppStore.getState();
  return (
    state.state === "converting" &&
    state.operation === operation &&
    state.workspaceAdmission.phase === "running" &&
    state.workspaceAdmission.operation === operation &&
    state.workspaceAdmission.token === token
  );
}

export async function runQueue(
  files: FileInfo[],
  buildRequest: QueueRequestBuilder,
  requestedPaths?: string[],
  options: QueueRunOptions = {},
) {
  const dependencies = options.dependencies ?? DEFAULT_QUEUE_RUNNER_DEPENDENCIES;
  if (files.length === 0) return;

  const store = useAppStore.getState();
  const previewRequest = buildRequest(files[0], null);
  const isGroupJob =
    previewRequest.operation === "mergePdf" ||
    previewRequest.operation === "createArchive" ||
    previewRequest.operation === "rename";
  const defaultPaths = isGroupJob
    ? files.map((file) => file.path)
    : unfinishedQueuePaths(files, store.queueItems);
  const requested = new Set(requestedPaths ?? defaultPaths);
  const requestedFiles = files.filter((file) => requested.has(file.path));
  if (requestedFiles.length === 0) return;

  const isRename = previewRequest.operation === "rename";
  const selected = isGroupJob ? files : requestedFiles;
  const capabilityRequests = isGroupJob
    ? [previewRequest]
    : selected.map((file) => buildRequest(file, null));
  const runToken = dependencies.createId();
  if (!store.claimWorkspacePreflight(previewRequest.operation, runToken)) return;
  const recentSetup = captureRecentJobSetup(
    capabilityRequests,
    selected,
    store.outputDirectory,
    store.outputSuffixes[previewRequest.operation],
  );
  if (options.authorize) {
    try {
      if (!(await options.authorize())) {
        useAppStore.getState().releaseWorkspaceClaim(runToken);
        return;
      }
    } catch (error) {
      console.error("Job confirmation failed:", error);
      if (useAppStore.getState().releaseWorkspaceClaim(runToken)) {
        useAppStore.getState().setRejection("Could not confirm this job");
      }
      return;
    }
  }
  try {
    const capabilities = await dependencies.checkCapabilities(capabilityRequests);
    const unavailable = capabilities.find((capability) => !capability.available);
    if (unavailable || capabilities.length !== capabilityRequests.length) {
      if (useAppStore.getState().releaseWorkspaceClaim(runToken)) {
        useAppStore
          .getState()
          .setRejection(unavailable?.message ?? "This job is not available on this Mac");
      }
      return;
    }
  } catch (error) {
    console.error("Capability preflight failed:", error);
    if (useAppStore.getState().releaseWorkspaceClaim(runToken)) {
      useAppStore.getState().setRejection("Could not verify the required local tools");
    }
    return;
  }

  if (
    !useAppStore.getState().startClaimedBatch(
      previewRequest.operation,
      runToken,
      selected.map((file) => file.path),
    )
  ) {
    return;
  }

  let renamedOutputs: string[] | null = null;
  let renameUndoManifest: string | null = null;
  if (isGroupJob) {
    const paths = selected.map((file) => file.path);
    const jobId = dependencies.createId();
    useAppStore.getState().startGroupJob(paths, jobId);
    try {
      const result = await dependencies.executeJob(buildRequest(selected[0], jobId));
      if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
      useAppStore.getState().completeGroupJob(paths, result);
      if (isRename && result.output_paths.length === paths.length) {
        renamedOutputs = result.output_paths;
        renameUndoManifest = result.undo_manifest ?? null;
        useAppStore.getState().applyRenamedPaths(paths, result.output_paths);
      }
    } catch (error: unknown) {
      if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
      const normalized = normalizeConversionError(error);
      useAppStore
        .getState()
        .failGroupJob(paths, normalized, normalized.kind === "Cancelled" ? "cancelled" : "failed");
    }
    if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
    useAppStore.getState().finishBatch();
  } else {
    for (const file of selected) {
      const current = useAppStore.getState();
      if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
      if (current.cancelBatchRequested) {
        current.cancelRemainingItems(selected.map((item) => item.path));
        break;
      }
      if (current.queueItems[file.path]?.status === "skipped") continue;

      const jobId = dependencies.createId();
      current.startQueueItem(file.path, jobId);

      try {
        const result = await dependencies.executeJob(buildRequest(file, jobId));
        if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
        useAppStore.getState().completeQueueItem(file.path, result);
      } catch (error: unknown) {
        if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
        const normalized = normalizeConversionError(error);
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

    if (!runClaimIsCurrent(previewRequest.operation, runToken)) return;
    useAppStore.getState().finishBatch();
  }
  const finished = useAppStore.getState();
  const recentItems = renamedOutputs
    ? selected.map((file, index) => {
        const outputPath = renamedOutputs[index];
        const inputName = outputPath.replaceAll("\\", "/").split("/").pop() ?? file.name;
        return {
          inputPath: outputPath,
          inputName,
          outputPath,
          outputPaths: [outputPath],
          status: "completed" as const,
        };
      })
    : buildRecentJobItems(selected, finished.queueItems);
  if (recentItems.length > 0) {
    useRecentJobs.getState().addJob({
      operation: previewRequest.operation,
      items: recentItems,
      ...(recentSetup ? { setup: recentSetup } : {}),
      undoManifest: renameUndoManifest,
    });
  }
}
