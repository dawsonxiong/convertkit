import { useEffect, useState } from "react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { getFileInfo, isTauriRuntime } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import { isPathSupportedForOperation } from "../lib/operations";

export function useFileDrop() {
  const [isDragging, setIsDragging] = useState(false);
  const addFiles = useAppStore((s) => s.addFiles);
  const setRejection = useAppStore((s) => s.setRejection);
  const operation = useAppStore((s) => s.operation);

  useEffect(() => {
    if (!isTauriRuntime()) return;

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
            const supported = paths.filter((path) => isPathSupportedForOperation(path, operation));
            if (supported.length === 0) {
              setRejection(
                operation !== "convert"
                  ? `${operation === "resize" ? "Resize" : "Optimize"} works with raster images`
                  : "Those file types are not supported",
              );
              return;
            }
            try {
              addFiles(await Promise.all(supported.map(getFileInfo)));
              if (supported.length < paths.length) {
                setRejection(`${paths.length - supported.length} unsupported file(s) skipped`);
              }
            } catch (err) {
              console.error("Failed to get file info:", err);
              setRejection("One or more files could not be opened");
            }
          }
        }
      });
    };

    setup();

    return () => {
      unlisten?.();
    };
  }, [addFiles, operation, setRejection]);

  return { isDragging };
}
