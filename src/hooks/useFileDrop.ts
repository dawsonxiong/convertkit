import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getFileInfo } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import { isSupportedFile } from "../lib/formats";

export function useFileDrop() {
  const [isDragging, setIsDragging] = useState(false);
  const setFile = useAppStore((s) => s.setFile);
  const setRejection = useAppStore((s) => s.setRejection);

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
            const path = paths[0];
            if (!isSupportedFile(path)) {
              const ext = path.split(".").pop()?.toLowerCase() ?? "unknown";
              setRejection(`".${ext}" files are not supported`);
              return;
            }
            try {
              const info = await getFileInfo(path);
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
  }, [setFile, setRejection]);

  return { isDragging };
}
