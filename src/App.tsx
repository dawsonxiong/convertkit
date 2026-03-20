import { useEffect } from "react";
import { AnimatePresence, motion } from "framer-motion";
import { useAppStore } from "./store/useAppStore";
import { useFileDrop } from "./hooks/useFileDrop";
import { useProgress } from "./hooks/useProgress";
import { useConvert } from "./hooks/useConvert";
import { TitleBar } from "./components/TitleBar";
import { DropZone } from "./components/DropZone";
import { FilePreview } from "./components/FilePreview";
import { FormatPicker } from "./components/FormatPicker";
import { ConvertButton } from "./components/ConvertButton";
import { ProgressBar } from "./components/ProgressBar";
import { StatusMessage } from "./components/StatusMessage";

const fade = {
  initial: { opacity: 0, y: 8 },
  animate: { opacity: 1, y: 0 },
  exit: { opacity: 0, y: -8 },
  transition: { duration: 0.2, ease: "easeOut" },
};

export default function App() {
  const state = useAppStore((s) => s.state);
  const { isDragging } = useFileDrop();
  const { convert, cancel } = useConvert();

  // Listen for progress events from the backend
  useProgress();

  // Keyboard shortcut: Enter to convert
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Enter" && state === "loaded") {
        convert();
      }
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [state, convert]);

  return (
    <div className="flex flex-col h-screen w-screen select-none bg-surface light:bg-surface-light">
      <TitleBar />

      <main className="flex-1 flex flex-col items-center justify-center px-6 pb-8">
        <AnimatePresence mode="wait">
          {state === "empty" && (
            <motion.div key="empty" {...fade} className="w-full max-w-md">
              <DropZone isDragging={isDragging} />
            </motion.div>
          )}

          {state === "loaded" && (
            <motion.div key="loaded" {...fade} className="w-full max-w-md flex flex-col gap-5">
              <FilePreview />
              <FormatPicker />
              <ConvertButton onClick={convert} mode="convert" />
            </motion.div>
          )}

          {state === "converting" && (
            <motion.div key="converting" {...fade} className="w-full max-w-md flex flex-col gap-5">
              <FilePreview />
              <ProgressBar />
              <ConvertButton onClick={cancel} mode="cancel" />
            </motion.div>
          )}

          {state === "done" && (
            <motion.div key="done" {...fade} className="w-full max-w-md flex flex-col gap-5">
              <StatusMessage variant="success" />
            </motion.div>
          )}

          {state === "error" && (
            <motion.div key="error" {...fade} className="w-full max-w-md flex flex-col gap-5">
              <StatusMessage variant="error" />
            </motion.div>
          )}
        </AnimatePresence>
      </main>
    </div>
  );
}
