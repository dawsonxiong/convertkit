import type { AppState, Operation } from "../types";
import { JOB_OPERATIONS } from "./operations.ts";

type AppShortcutAction = "open" | "reveal" | "run" | "cancel";

interface AppShortcutContext {
  key: string;
  command: boolean;
  interactiveTarget: boolean;
  state: AppState;
  operation: Operation;
  auxiliaryViewActive: boolean;
  hasOutput: boolean;
  workspaceAdmissionPending?: boolean;
}

const JOB_OPERATION_SET = new Set<Operation>(JOB_OPERATIONS);
export const INPUT_INTAKE_BLOCKED_MESSAGE =
  "Wait for the current job to finish before adding files";

export function isInputIntakeBlocked(
  state: AppState,
  auxiliaryViewActive = false,
  workspaceAdmissionPending = false,
): boolean {
  return workspaceAdmissionPending || auxiliaryViewActive || state === "converting";
}

export function resolveAppShortcut(context: AppShortcutContext): AppShortcutAction | null {
  const key = context.key.toLocaleLowerCase();
  const intakeBlocked = isInputIntakeBlocked(
    context.state,
    context.auxiliaryViewActive,
    context.workspaceAdmissionPending,
  );

  if (context.auxiliaryViewActive || context.workspaceAdmissionPending) return null;

  if (context.command) {
    if (key === "o") return intakeBlocked ? null : "open";
    if (key === "r" && context.state === "done" && context.hasOutput) return "reveal";
    return null;
  }

  if (context.interactiveTarget) return null;

  if (key === "enter" && context.state === "loaded" && JOB_OPERATION_SET.has(context.operation)) {
    return "run";
  }

  if (key !== "escape") return null;
  if (context.state === "converting") return "cancel";
  return null;
}
