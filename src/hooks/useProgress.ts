import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../store/useAppStore";
import type { ProgressPayload } from "../types";
import { isTauriRuntime } from "../lib/tauri";

export function useProgress() {
  const updateProgress = useAppStore((s) => s.updateProgress);

  useEffect(() => {
    if (!isTauriRuntime()) return;

    let unlisten: (() => void) | undefined;

    const setup = async () => {
      unlisten = await listen<ProgressPayload>("conversion-progress", (event) => {
        updateProgress(event.payload.percent, event.payload.stage);
      });
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, [updateProgress]);
}
