import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { useAppStore } from "../store/useAppStore";
import type { ProgressPayload } from "../types";

export function useProgress() {
  const updateProgress = useAppStore((s) => s.updateProgress);

  useEffect(() => {
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
