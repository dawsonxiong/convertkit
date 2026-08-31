import { useEffect, useState } from "react";
import { collectInputPaths, isTauriRuntime } from "../lib/tauri.ts";
import { loadWorkspaceDraft, OPERATION_IDS, saveWorkspaceDraft } from "../lib/workspaceDraft.ts";
import {
  currentWorkspaceDraft,
  MAX_QUEUE_ITEMS,
  restoreWorkspaceDraft,
  useAppStore,
} from "../store/useAppStore.ts";
import type { FileInfo, Operation } from "../types/index.ts";

const SAVE_DELAY_MS = 250;
type InputCollector = typeof collectInputPaths;

export async function restoreWorkspaceSessions(
  draft: ReturnType<typeof loadWorkspaceDraft>,
  restoreToken: string,
  collect: InputCollector = collectInputPaths,
  isActive: () => boolean = () => true,
): Promise<boolean> {
  if (!draft) {
    return isActive() && useAppStore.getState().completeWorkspaceRestore(restoreToken);
  }

  const restoredEntries = await Promise.all(
    OPERATION_IDS.map(async (operation) => {
      const paths = draft.sessions[operation]?.paths ?? [];
      if (paths.length === 0) return [operation, []] as const;
      try {
        const result = await collect(paths, operation, MAX_QUEUE_ITEMS, []);
        return [operation, result.files] as const;
      } catch {
        return [operation, []] as const;
      }
    }),
  );
  if (!isActive()) return false;
  return restoreWorkspaceDraft(
    draft,
    Object.fromEntries(restoredEntries) as Partial<Record<Operation, FileInfo[]>>,
    restoreToken,
  );
}

export function useWorkspaceRecovery() {
  const [ready, setReady] = useState(() => !isTauriRuntime());

  useEffect(() => {
    if (!isTauriRuntime()) {
      setReady(true);
      return;
    }

    let disposed = false;
    let initialized = false;
    let saveTimer: number | null = null;
    let unsubscribe: (() => void) | null = null;

    const persist = () => {
      if (saveTimer !== null) {
        window.clearTimeout(saveTimer);
        saveTimer = null;
      }
      saveWorkspaceDraft(currentWorkspaceDraft());
    };

    const schedulePersist = () => {
      if (saveTimer !== null) window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(persist, SAVE_DELAY_MS);
    };

    const restore = async () => {
      const draft = loadWorkspaceDraft();
      const restoreToken = useAppStore.getState().beginWorkspaceRestore();
      if (!restoreToken) return;
      const restored = await restoreWorkspaceSessions(
        draft,
        restoreToken,
        collectInputPaths,
        () => !disposed,
      );
      if (!restored || disposed) return;
      persist();
      unsubscribe = useAppStore.subscribe(schedulePersist);
      window.addEventListener("beforeunload", persist);
      initialized = true;
      setReady(true);
    };

    void restore();
    return () => {
      disposed = true;
      unsubscribe?.();
      window.removeEventListener("beforeunload", persist);
      if (initialized) persist();
    };
  }, []);

  return ready;
}
