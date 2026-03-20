import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getFileInfo } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";

export function useFileDrop() {
  const [isDragging, setIsDragging] = useState(false);
  const setFile = useAppStore((s) => s.setFile);

  useEffect(() => {
    const appWindow = getCurrentWebviewWindow();
    let unlisten: (() => void) | undefined;

    const setup = async () => {
      unlisten = await appWindow.onDragDropEvent(async (event) => {
        if (event.payload.type === "over") {
          setIsDragging(true);
        } else if (event.payload.type === "leave") {
          setIsDragging(false);
        } else if (event.payload.type === "drop") {
          setIsDragging(false);
          const paths = event.payload.paths;
          if (paths.length > 0) {
            try {
              const info = await getFileInfo(paths[0]);
              setFile(info);
            } catch (err) {
              console.error("Failed to get file info:", err);
            }
          }
        }
      });
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, [setFile]);

  return { isDragging };
}
