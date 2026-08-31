import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { DropZone } from "./components/DropZone";
import { HistoryWorkspace } from "./components/HistoryWorkspace";
import { InspectorWorkspace } from "./components/InspectorWorkspace";
import { ToolNav } from "./components/ToolNav";
import { WorkspaceQueue } from "./components/WorkspaceQueue";
import { useFileDrop } from "./hooks/useFileDrop";
import { useProgress } from "./hooks/useProgress";
import { useJobRunner } from "./hooks/useJobRunner";
import { useAddPaths } from "./hooks/useAddPaths";
import { useWorkspaceRecovery } from "./hooks/useWorkspaceRecovery";
import { getOperationDialogFilter, OPERATIONS } from "./lib/operations";
import {
  type FinderQuickActionRequest,
  getFileInfo,
  getOpenedInputs,
  isTauriRuntime,
  removeFinderQuickAction,
  revealPathsInFinder,
  saveClipboardImage,
} from "./lib/tauri";
import { operationForOpenedPaths } from "./lib/openWith";
import { outputPathsForResults } from "./lib/outputHandoff";
import {
  INPUT_INTAKE_BLOCKED_MESSAGE,
  isInputIntakeBlocked,
  resolveAppShortcut,
} from "./lib/appShortcuts";
import {
  arrayBufferToBase64,
  operationAcceptsPastedImage,
  selectClipboardImage,
} from "./lib/clipboard";
import { isInteractiveKeyboardTarget, isTextEntryTarget } from "./lib/interactionTargets";
import { useAppStore, workspaceAdmissionIsPending } from "./store/useAppStore";
import { useSavedRecipes } from "./store/useSavedRecipes";
import type { Operation } from "./types";

export default function App() {
  const [activityActive, setActivityActive] = useState(
    () =>
      import.meta.env.DEV &&
      new URLSearchParams(window.location.search).get("fixture") === "activity",
  );
  const state = useAppStore((store) => store.state);
  const operation = useAppStore((store) => store.operation);
  const workspaceAdmission = useAppStore((store) => store.workspaceAdmission);
  const setOperation = useAppStore((store) => store.setOperation);
  const results = useAppStore((store) => store.results);
  const rejectionMessage = useAppStore((store) => store.rejectionMessage);
  const clearRejection = useAppStore((store) => store.clearRejection);
  const setRejection = useAppStore((store) => store.setRejection);
  const requestBatchCancel = useAppStore((store) => store.requestBatchCancel);
  const auxiliaryViewActive = activityActive;
  const admissionPending = workspaceAdmissionIsPending(workspaceAdmission);
  const intakeBlocked = isInputIntakeBlocked(state, auxiliaryViewActive, admissionPending);
  const { isDragging } = useFileDrop(!intakeBlocked);
  const addPaths = useAddPaths();
  const { run: runOperation, cancel: cancelOperation } = useJobRunner(operation);
  const workspaceReady = useWorkspaceRecovery();
  const retiredRecipeIds = useSavedRecipes((store) => store.retiredRecipeIds);
  const finishRetiredRecipeMigration = useSavedRecipes(
    (store) => store.finishRetiredRecipeMigration,
  );
  const visualFixtureApplied = useRef(false);
  const deferredOpenedPaths = useRef<string[]>([]);
  const deferredQuickAction = useRef<FinderQuickActionRequest | null>(null);
  const completedOutputPaths = useMemo(() => outputPathsForResults(results), [results]);

  useProgress();

  useEffect(() => {
    if (!isTauriRuntime() || retiredRecipeIds.length === 0) return;
    let active = true;
    void Promise.all(retiredRecipeIds.map((recipeId) => removeFinderQuickAction(recipeId)))
      .then(() => {
        if (active) finishRetiredRecipeMigration();
      })
      .catch((error) => console.error("Could not remove a retired Finder Quick Action:", error));
    return () => {
      active = false;
    };
  }, [finishRetiredRecipeMigration, retiredRecipeIds]);

  useEffect(() => {
    if (!import.meta.env.DEV || !workspaceReady || visualFixtureApplied.current) return;
    const visualFixture = new URLSearchParams(window.location.search).get("fixture");
    if (!visualFixture) return;

    visualFixtureApplied.current = true;
    void import("./dev/visualFixture").then(({ applyVisualFixture }) => {
      applyVisualFixture(visualFixture);
    });
  }, [workspaceReady]);

  const isInspect = operation === "inspect";
  const meta = OPERATIONS[operation];
  const cancelBatch = useCallback(() => {
    requestBatchCancel();
    void cancelOperation();
  }, [cancelOperation, requestBatchCancel]);
  const startOperation = useCallback(() => {
    if (!workspaceReady || useAppStore.getState().workspaceAdmission.phase !== "idle") return;
    void runOperation();
  }, [runOperation, workspaceReady]);

  const loadExternalPaths = useCallback(
    async (paths: string[]) => {
      if (paths.length === 0) return;
      if (useAppStore.getState().workspaceAdmission.phase !== "idle") {
        deferredOpenedPaths.current = Array.from(
          new Set([...deferredOpenedPaths.current, ...paths]),
        ).slice(0, 100);
        setRejection("Opened files will be added when the workspace is ready");
        return;
      }

      const targetOperation = operationForOpenedPaths(paths, useAppStore.getState().operation);
      setActivityActive(false);
      if (targetOperation !== useAppStore.getState().operation) setOperation(targetOperation);
      await addPaths(paths, targetOperation);
    },
    [addPaths, setOperation, setRejection],
  );

  const stageFinderQuickAction = useCallback(
    async (request: FinderQuickActionRequest) => {
      if (request.paths.length === 0) return;
      if (useAppStore.getState().workspaceAdmission.phase !== "idle") {
        deferredQuickAction.current = request;
        setRejection("The Finder recipe will open when the workspace is ready");
        return;
      }

      const recipe = useSavedRecipes
        .getState()
        .recipes.find((candidate) => candidate.id === request.recipeId);
      if (!recipe) {
        setRejection("That Finder recipe no longer exists");
        return;
      }

      if (recipe.operation !== useAppStore.getState().operation) setOperation(recipe.operation);
      setActivityActive(false);
      const target = useAppStore.getState();
      target.reset();
      target.setOutputDirectory(recipe.directory);
      target.setOutputSuffix(recipe.operation, recipe.suffix);
      target.applyRecipeSettings(recipe.settings);
      await addPaths(request.paths, recipe.operation);
    },
    [addPaths, setOperation, setRejection],
  );

  useEffect(() => {
    if (!isTauriRuntime() || !workspaceReady) return;

    let active = true;
    let disposeFiles: (() => void) | undefined;
    let disposeQuickActions: (() => void) | undefined;
    void Promise.all([
      listen<string[]>("files-opened", (event) => {
        if (active) void loadExternalPaths(event.payload);
      }),
      listen<FinderQuickActionRequest[]>("finder-quick-actions-opened", (event) => {
        if (active && event.payload.length > 0) {
          void stageFinderQuickAction(event.payload[event.payload.length - 1]);
        }
      }),
    ])
      .then(async ([unlistenFiles, unlistenQuickActions]) => {
        if (!active) {
          unlistenFiles();
          unlistenQuickActions();
          return;
        }
        disposeFiles = unlistenFiles;
        disposeQuickActions = unlistenQuickActions;
        const buffered = await getOpenedInputs();
        if (!active) return;
        if (buffered.paths.length > 0) await loadExternalPaths(buffered.paths);
        if (buffered.quickActions.length > 0) {
          await stageFinderQuickAction(buffered.quickActions[buffered.quickActions.length - 1]);
        }
      })
      .catch((error) => console.error("Failed to initialize Finder integration:", error));

    return () => {
      active = false;
      disposeFiles?.();
      disposeQuickActions?.();
    };
  }, [loadExternalPaths, stageFinderQuickAction, workspaceReady]);

  useEffect(() => {
    if (state === "converting" || workspaceAdmission.phase !== "idle") return;
    if (deferredQuickAction.current) {
      const request = deferredQuickAction.current;
      deferredQuickAction.current = null;
      void stageFinderQuickAction(request);
      return;
    }
    if (deferredOpenedPaths.current.length > 0) {
      const paths = deferredOpenedPaths.current;
      deferredOpenedPaths.current = [];
      void loadExternalPaths(paths);
    }
  }, [loadExternalPaths, stageFinderQuickAction, state, workspaceAdmission.phase]);

  useEffect(() => {
    if (!rejectionMessage) return;
    const timer = setTimeout(clearRejection, 3200);
    return () => clearTimeout(timer);
  }, [rejectionMessage, clearRejection]);

  const openFileBrowser = useCallback(async () => {
    if (intakeBlocked) return;
    const selected = await open({
      multiple: true,
      title: meta.fileDialogTitle,
      filters: getOperationDialogFilter(operation),
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];

    await addPaths(paths);
  }, [addPaths, intakeBlocked, meta.fileDialogTitle, operation]);

  const openOperation = useCallback(
    (value: Operation) => {
      if (useAppStore.getState().workspaceAdmission.phase !== "idle") return;
      setActivityActive(false);
      if (value !== useAppStore.getState().operation) setOperation(value);
    },
    [setOperation],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const action = resolveAppShortcut({
        key: event.key,
        command: event.metaKey || event.ctrlKey,
        interactiveTarget: isInteractiveKeyboardTarget(event.target),
        state,
        operation,
        auxiliaryViewActive,
        hasOutput: completedOutputPaths.length > 0,
        workspaceAdmissionPending: admissionPending,
      });
      if (!action) return;

      event.preventDefault();
      if (action === "open") void openFileBrowser();
      else if (action === "reveal" && completedOutputPaths.length > 0) {
        void revealPathsInFinder(completedOutputPaths).catch(() =>
          setRejection("That output is no longer available"),
        );
      } else if (action === "run") startOperation();
      else if (action === "cancel") cancelBatch();
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [
    state,
    completedOutputPaths,
    startOperation,
    cancelBatch,
    openFileBrowser,
    operation,
    setRejection,
    auxiliaryViewActive,
    admissionPending,
  ]);

  useEffect(() => {
    const handlePaste = async (event: ClipboardEvent) => {
      if (isTextEntryTarget(event.target)) return;
      const items = event.clipboardData?.items;
      if (!items) return;
      const selection = selectClipboardImage(items);
      if (selection.kind === "none") return;
      if (intakeBlocked) {
        event.preventDefault();
        setRejection(
          state === "converting" || admissionPending
            ? INPUT_INTAKE_BLOCKED_MESSAGE
            : "Open a file tool first",
        );
        return;
      }
      if (selection.kind === "unsupported") {
        setRejection("That pasted image format is not supported");
        return;
      }

      event.preventDefault();
      if (!operationAcceptsPastedImage(operation)) {
        setRejection("Pasted images are not supported by this tool");
        return;
      }
      const blob = selection.item.getAsFile();
      if (!blob) {
        setRejection("Could not read that pasted image");
        return;
      }

      const targetOperation = operation;
      try {
        const path = await saveClipboardImage(
          arrayBufferToBase64(await blob.arrayBuffer()),
          selection.item.type,
        );
        const file = await getFileInfo(path);
        useAppStore.getState().addFilesForOperation(targetOperation, [file]);
      } catch (error) {
        console.error("Failed to paste image:", error);
        setRejection("Could not paste that image");
      }
    };

    window.addEventListener("paste", handlePaste);
    return () => window.removeEventListener("paste", handlePaste);
  }, [admissionPending, intakeBlocked, operation, setRejection, state]);

  return (
    <div className="app-background flex h-screen w-screen select-none overflow-hidden bg-surface text-white">
      <ToolNav
        operation={operation}
        activityActive={activityActive}
        disabled={!workspaceReady || workspaceAdmission.phase !== "idle"}
        onChange={openOperation}
        onOpenActivity={() => setActivityActive(true)}
      />

      <section className="relative flex min-w-0 flex-1 flex-col">
        <header className="h-10 shrink-0 border-b border-[#3b3d46]" data-tauri-drag-region />

        <main className="workspace-grid min-h-0 flex-1 overflow-hidden p-6">
          {activityActive ? (
            <HistoryWorkspace
              disabled={admissionPending || state === "converting"}
              onOpen={openOperation}
            />
          ) : isInspect ? (
            <InspectorWorkspace isDragging={isDragging} />
          ) : (
            <div className="flex h-full min-h-0 flex-col">
              <header className="shrink-0">
                <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">
                  {meta.pageTitle}
                </h1>
                <p className="mt-1 text-sm text-[#a8a8b1]">{meta.description}</p>
              </header>

              <div className="mt-5 grid min-h-0 flex-1 grid-cols-[minmax(0,7fr)_minmax(280px,5fr)] gap-4">
                <DropZone isDragging={isDragging} disabled={intakeBlocked} />
                <WorkspaceQueue
                  onStart={startOperation}
                  onCancel={cancelBatch}
                  onCancelItem={cancelOperation}
                  onOpenOperation={openOperation}
                  interactionBlocked={admissionPending}
                />
              </div>
            </div>
          )}
        </main>

        {rejectionMessage && (
          <div
            role="alert"
            aria-atomic="true"
            className="absolute bottom-5 left-1/2 -translate-x-1/2 border border-red-400/20 bg-[#2a171a] px-3 py-2 text-xs text-red-200"
          >
            {rejectionMessage}
          </div>
        )}
      </section>
    </div>
  );
}
