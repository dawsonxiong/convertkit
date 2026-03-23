import { useEffect, useCallback, useState } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { open } from "@tauri-apps/plugin-dialog";
import { useAppStore } from "./store/useAppStore";
import { useFileDrop } from "./hooks/useFileDrop";
import { useProgress } from "./hooks/useProgress";
import { useConvert } from "./hooks/useConvert";
import { getFileInfo, revealInFinder } from "./lib/tauri";
import { DropZone } from "./components/DropZone";
import { FilePreview } from "./components/FilePreview";
import { FormatPicker } from "./components/FormatPicker";
import { ConvertButton } from "./components/ConvertButton";
import { ProgressBar } from "./components/ProgressBar";
import { StatusMessage } from "./components/StatusMessage";
import { OnboardingCheck } from "./components/OnboardingCheck";

const fade = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -8 },
  transition: { duration: 0.2, ease: "easeOut" },
};

export default function App() {
  const [ready, setReady] = useState(false);
  const state = useAppStore((s) => s.state);
  const result = useAppStore((s) => s.result);
  const setFile = useAppStore((s) => s.setFile);
  const reset = useAppStore((s) => s.reset);
  const rejectionMessage = useAppStore((s) => s.rejectionMessage);
  const clearRejection = useAppStore((s) => s.clearRejection);
  const { isDragging } = useFileDrop();
  const { convert, cancel } = useConvert();

  useProgress();

  // Auto-clear rejection message after 3 seconds
  useEffect(() => {
    if (!rejectionMessage) return;
    const timer = setTimeout(clearRejection, 3000);
    return () => clearTimeout(timer);
  }, [rejectionMessage, clearRejection]);

  const openFileBrowser = useCallback(async () => {
    const selected = await open({ multiple: false, title: "Choose a file to convert" });
    if (selected) {
      try {
        const info = await getFileInfo(selected);
        setFile(info);
      } catch (err) {
        console.error("Failed to get file info:", err);
      }
    }
  }, [setFile]);

  // Keyboard shortcuts
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      const meta = e.metaKey || e.ctrlKey;

      // ⌘O — open file browser
      if (meta && e.key === "o") {
        e.preventDefault();
        openFileBrowser();
        return;
      }
      // ⌘R — reveal in Finder (done state)
      if (meta && e.key === "r" && state === "done" && result?.output_path) {
        e.preventDefault();
        revealInFinder(result.output_path);
        return;
      }
      // Enter / ⌘Enter — start conversion
      if (e.key === "Enter" && state === "loaded") {
        e.preventDefault();
        convert();
        return;
      }
      // Escape — cancel or reset
      if (e.key === "Escape") {
        e.preventDefault();
        if (state === "converting") cancel();
        else if (state !== "empty") reset();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state, result, convert, cancel, reset, openFileBrowser]);

  if (!ready) {
    return (
      <div className="flex flex-col h-screen w-screen select-none bg-surface light:bg-surface-light">
          <main className="flex-1 flex flex-col items-center justify-center px-6 pb-8">
          <OnboardingCheck onReady={() => setReady(true)} />
        </main>
      </div>
    );
  }

  return (
    <div className="flex flex-col h-screen w-screen select-none bg-surface light:bg-surface-light">

      <main className="flex-1 flex flex-col items-center justify-center px-6 pb-8">
        <AnimatePresence mode="wait">
          {state === "empty" && (
            <motion.div key="empty" {...fade} className="w-full max-w-md flex flex-col gap-3">
              <DropZone isDragging={isDragging} />
              <AnimatePresence>
                {rejectionMessage && (
                  <motion.p
                    initial={{ opacity: 0, y: -4 }}
                    animate={{ opacity: 1, y: 0 }}
                    exit={{ opacity: 0, y: -4 }}
                    className="text-xs text-error text-center"
                  >
                    {rejectionMessage}
                  </motion.p>
                )}
              </AnimatePresence>
            </motion.div>
          )}

          {state === "loaded" && (
            <motion.div key="loaded" {...fade} className="w-full max-w-md flex flex-col gap-6">
              <FilePreview />
              <FormatPicker />
              <ConvertButton onClick={convert} mode="convert" />
            </motion.div>
          )}

          {state === "converting" && (
            <motion.div key="converting" {...fade} className="w-full max-w-md flex flex-col gap-6">
              <FilePreview />
              <ProgressBar />
              <ConvertButton onClick={cancel} mode="cancel" />
            </motion.div>
          )}

          {state === "done" && (
            <motion.div key="done" {...fade} className="w-full max-w-md flex flex-col gap-6">
              <StatusMessage variant="success" />
            </motion.div>
          )}

          {state === "error" && (
            <motion.div key="error" {...fade} className="w-full max-w-md flex flex-col gap-6">
              <StatusMessage variant="error" />
            </motion.div>
          )}
        </AnimatePresence>
      </main>
    </div>
  );
}
