import { useCallback, useEffect, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { DropZone } from "./components/DropZone";
import { OnboardingCheck } from "./components/OnboardingCheck";
import { ToolNav } from "./components/ToolNav";
import { WorkspaceQueue } from "./components/WorkspaceQueue";
import { useConvert } from "./hooks/useConvert";
import { useFileDrop } from "./hooks/useFileDrop";
import { useOptimize } from "./hooks/useOptimize";
import { useProgress } from "./hooks/useProgress";
import { useResize } from "./hooks/useResize";
import { getOperationDialogFilter, isPathSupportedForOperation } from "./lib/operations";
import { isSupportedFile } from "./lib/formats";
import {
  getFileInfo,
  getOpenedFile,
  isTauriRuntime,
  revealInFinder,
  saveClipboardImage,
} from "./lib/tauri";
import { useAppStore } from "./store/useAppStore";

export default function App() {
  const [ready, setReady] = useState(false);
  const state = useAppStore((store) => store.state);
  const operation = useAppStore((store) => store.operation);
  const setOperation = useAppStore((store) => store.setOperation);
  const result = useAppStore((store) => store.result);
  const setFile = useAppStore((store) => store.setFile);
  const addFiles = useAppStore((store) => store.addFiles);
  const reset = useAppStore((store) => store.reset);
  const rejectionMessage = useAppStore((store) => store.rejectionMessage);
  const clearRejection = useAppStore((store) => store.clearRejection);
  const setRejection = useAppStore((store) => store.setRejection);
  const requestBatchCancel = useAppStore((store) => store.requestBatchCancel);
  const { isDragging } = useFileDrop();
  const { convert, cancel } = useConvert();
  const { resize, cancel: cancelResize } = useResize();
  const { optimize, cancel: cancelOptimize } = useOptimize();

  useProgress();

  const isResize = operation === "resize";
  const isOptimize = operation === "optimize";
  const startOperation = isResize ? resize : isOptimize ? optimize : convert;
  const cancelOperation = isResize ? cancelResize : isOptimize ? cancelOptimize : cancel;
  const cancelBatch = useCallback(() => {
    requestBatchCancel();
    void cancelOperation();
  }, [cancelOperation, requestBatchCancel]);

  const loadExternalFile = useCallback(
    async (path: string) => {
      if (!isSupportedFile(path)) return;
      try {
        if (!isPathSupportedForOperation(path, operation)) {
          if (isPathSupportedForOperation(path, "convert")) {
            setOperation("convert");
          } else {
            setRejection("There is not a compatible tool for that file yet");
            return;
          }
        }
        setFile(await getFileInfo(path));
      } catch (error) {
        console.error("Failed to load opened file:", error);
      }
    },
    [operation, setFile, setOperation, setRejection],
  );

  useEffect(() => {
    if (!isTauriRuntime()) return;

    getOpenedFile().then((path) => {
      if (path) loadExternalFile(path);
    });
    const unlisten = listen<string>("file-opened", (event) => {
      loadExternalFile(event.payload);
    });
    return () => {
      unlisten.then((dispose) => dispose());
    };
  }, [loadExternalFile]);

  useEffect(() => {
    if (!rejectionMessage) return;
    const timer = setTimeout(clearRejection, 3200);
    return () => clearTimeout(timer);
  }, [rejectionMessage, clearRejection]);

  const openFileBrowser = useCallback(async () => {
    const selected = await open({
      multiple: true,
      title:
        operation === "convert"
          ? "Choose files to convert"
          : operation === "resize"
            ? "Choose images to resize"
            : "Choose images to optimize",
      filters: getOperationDialogFilter(operation),
    });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];

    try {
      addFiles(await Promise.all(paths.map(getFileInfo)));
    } catch (error) {
      console.error("Failed to get file info:", error);
      setRejection("One or more files could not be opened");
    }
  }, [addFiles, operation, setRejection]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const command = event.metaKey || event.ctrlKey;

      if (command && event.key === "o") {
        event.preventDefault();
        openFileBrowser();
        return;
      }
      if (command && event.key === "r" && state === "done" && result?.output_path) {
        event.preventDefault();
        revealInFinder(result.output_path);
        return;
      }
      if (event.key === "Enter" && state === "loaded") {
        event.preventDefault();
        startOperation();
        return;
      }
      if (event.key === "Escape") {
        event.preventDefault();
        if (state === "converting") cancelBatch();
        else if (state !== "empty") reset();
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state, result, startOperation, cancelBatch, reset, openFileBrowser]);

  useEffect(() => {
    const supportedMimes = ["image/png", "image/jpeg", "image/webp", "image/gif", "image/bmp"];

    const handlePaste = async (event: ClipboardEvent) => {
      const items = event.clipboardData?.items;
      if (!items) return;

      for (const item of items) {
        if (!supportedMimes.includes(item.type)) continue;

        event.preventDefault();
        const blob = item.getAsFile();
        if (!blob) continue;

        const buffer = await blob.arrayBuffer();
        const base64 = btoa(
          new Uint8Array(buffer).reduce((value, byte) => value + String.fromCharCode(byte), ""),
        );

        try {
          const path = await saveClipboardImage(base64, item.type);
          setFile(await getFileInfo(path));
        } catch (error) {
          console.error("Failed to paste image:", error);
          setRejection("Could not paste that image");
        }
        return;
      }

      setRejection("The clipboard does not contain a supported image");
    };

    window.addEventListener("paste", handlePaste);
    return () => window.removeEventListener("paste", handlePaste);
  }, [setFile, setRejection]);

  if (!ready) {
    return (
      <div className="grid h-screen w-screen select-none place-items-center bg-surface">
        <OnboardingCheck onReady={() => setReady(true)} />
      </div>
    );
  }

  return (
    <div className="app-background flex h-screen w-screen select-none overflow-hidden bg-surface text-white">
      <ToolNav
        operation={operation}
        disabled={state === "converting"}
        onChange={(value) => {
          if (value !== operation) setOperation(value);
        }}
      />

      <section className="relative flex min-w-0 flex-1 flex-col">
        <header className="h-10 shrink-0 border-b border-[#3b3d46]" data-tauri-drag-region />

        <main className="min-h-0 flex-1 overflow-hidden p-6">
          <div className="flex h-full min-h-0 flex-col">
            <header className="shrink-0">
              <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">
                {isResize
                  ? "Resize images"
                  : isOptimize
                    ? "Optimize images"
                    : "Universal file converter"}
              </h1>
              <p className="mt-1 text-sm text-[#a8a8b1]">
                {isResize
                  ? "Set exact dimensions or scale an image by percentage."
                  : isOptimize
                    ? "Reduce file size while keeping images looking sharp."
                    : "Convert images, video, audio, documents, and vectors."}
              </p>
            </header>

            <div className="mt-5 grid min-h-0 flex-1 grid-cols-[minmax(0,7fr)_minmax(280px,5fr)] gap-4">
              <DropZone isDragging={isDragging} disabled={state === "converting"} />
              <WorkspaceQueue
                onStart={startOperation}
                onCancel={cancelBatch}
                onCancelItem={cancelOperation}
              />
            </div>
          </div>
        </main>

        <AnimatePresence>
          {rejectionMessage && (
            <motion.div
              initial={{ opacity: 0, y: 8 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: 8 }}
              className="absolute bottom-5 left-1/2 -translate-x-1/2 rounded-lg border border-red-400/20 bg-[#2a171a] px-3 py-2 text-xs text-red-200 shadow-lg"
            >
              {rejectionMessage}
            </motion.div>
          )}
        </AnimatePresence>
      </section>
    </div>
  );
}
