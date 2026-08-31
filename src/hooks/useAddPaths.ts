import { useCallback } from "react";
import { isActiveOperation, type ActiveOperation } from "../lib/operations.ts";
import { collectInputPaths } from "../lib/tauri.ts";
import { MAX_QUEUE_ITEMS, useAppStore, workspaceAdmissionIsPending } from "../store/useAppStore.ts";
import { INPUT_INTAKE_BLOCKED_MESSAGE, isInputIntakeBlocked } from "../lib/appShortcuts.ts";

type InputCollector = typeof collectInputPaths;

function targetIntakeIsBlocked(targetOperation: ActiveOperation): boolean {
  const state = useAppStore.getState();
  if (workspaceAdmissionIsPending(state.workspaceAdmission)) return true;
  const sessionState =
    targetOperation === state.operation ? state.state : state.sessions[targetOperation].state;
  return isInputIntakeBlocked(sessionState);
}

export async function addPathsToOperation(
  paths: string[],
  targetOperation = useAppStore.getState().operation,
  collect: InputCollector = collectInputPaths,
) {
  if (paths.length === 0) return;

  const state = useAppStore.getState();
  if (!isActiveOperation(targetOperation)) {
    state.setRejection("That utility is no longer available");
    return;
  }
  if (targetIntakeIsBlocked(targetOperation)) {
    state.setRejection(INPUT_INTAKE_BLOCKED_MESSAGE);
    return;
  }
  const targetFiles =
    targetOperation === state.operation ? state.files : state.sessions[targetOperation].files;
  const remaining = MAX_QUEUE_ITEMS - targetFiles.length;
  if (remaining <= 0) {
    state.setRejection("Queue limit reached");
    return;
  }

  try {
    const result = await collect(
      paths,
      targetOperation,
      remaining,
      targetFiles.map((file) => file.path),
    );
    if (targetIntakeIsBlocked(targetOperation)) {
      useAppStore.getState().setRejection(INPUT_INTAKE_BLOCKED_MESSAGE);
      return;
    }
    if (result.files.length === 0) {
      useAppStore.getState().setRejection("No supported files found");
      return;
    }

    useAppStore.getState().addFilesForOperation(targetOperation, result.files);
    if (result.truncated) {
      useAppStore
        .getState()
        .setRejection(`${result.files.length} files added. Queue limit reached.`);
    } else if (result.skippedCount > 0) {
      useAppStore
        .getState()
        .setRejection(
          `${result.files.length} ${result.files.length === 1 ? "file" : "files"} added. ${result.skippedCount} skipped.`,
        );
    }
  } catch (error) {
    console.error("Failed to add files:", error);
    useAppStore.getState().setRejection("Those files could not be opened");
  }
}

export function useAddPaths() {
  return useCallback(addPathsToOperation, []);
}
