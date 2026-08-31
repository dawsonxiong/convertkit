import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { isTauriRuntime } from "../lib/tauri";
import { useAddPaths } from "./useAddPaths";

export function useFileDrop(enabled = true) {
  const [isDragging, setIsDragging] = useState(false);
  const addPaths = useAddPaths();

  useEffect(() => {
    if (!isTauriRuntime()) return;

    const appWindow = getCurrentWebviewWindow();
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      unlisten = await appWindow.onDragDropEvent(async (event) => {
        if (!enabled) {
          setIsDragging(false);
        } else if (event.payload.type === "over") {
          setIsDragging(true);
        } else if (event.payload.type === "leave") {
          setIsDragging(false);
        } else if (event.payload.type === "drop") {
          setIsDragging(false);
          const paths = event.payload.paths;
          if (paths.length > 0) {
            await addPaths(paths);
          }
        }
      });
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, [addPaths, enabled]);

  return { isDragging };
}
