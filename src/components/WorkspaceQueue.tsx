import { AnimatePresence, motion } from "framer-motion";
import { ConvertButton } from "./ConvertButton";
import { FilePreview } from "./FilePreview";
import { ProgressBar } from "./ProgressBar";
import { ResizePanel } from "./ResizePanel";
import { StatusMessage } from "./StatusMessage";
import { useAppStore } from "../store/useAppStore";

interface WorkspaceQueueProps {
  onStart: () => void;
  onCancel: () => void;
}

export function WorkspaceQueue({ onStart, onCancel }: WorkspaceQueueProps) {
  const state = useAppStore((store) => store.state);
  const operation = useAppStore((store) => store.operation);
  const files = useAppStore((store) => store.files);
  const file = useAppStore((store) => store.file);
  const reset = useAppStore((store) => store.reset);

  const canClear = Boolean(file) && state !== "converting";

  return (
    <section className="flex min-h-0 flex-col border border-[#3b3d46] bg-[#131315]">
      <header className="flex h-11 shrink-0 items-center justify-between border-b border-[#3b3d46] bg-[#1b1b1d] px-3">
        <h2 className="text-[13px] font-semibold text-white/85">Queue ({files.length})</h2>
        {canClear && (
          <button
            type="button"
            onClick={reset}
            className="text-[11px] text-white/45 transition-colors hover:text-white/80"
          >
            Clear
          </button>
        )}
      </header>

      <div className="queue-scroll min-h-0 flex-1 overflow-y-auto p-3">
        <AnimatePresence mode="wait">
          {state === "empty" && (
            <motion.div
              key="queue-empty"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="grid h-full min-h-40 place-items-center text-xs text-white/30"
            >
              No files added
            </motion.div>
          )}

          {state === "loaded" && (
            <motion.div
              key={`queue-loaded-${operation}`}
              initial={{ opacity: 0, y: 4 }}
              animate={{ opacity: 1, y: 0 }}
              exit={{ opacity: 0, y: -4 }}
              className="flex flex-col gap-4"
            >
              <div className="flex flex-col gap-2">
                {files.map((item) => (
                  <FilePreview key={item.path} file={item} canDismiss />
                ))}
              </div>
              {operation === "resize" && <ResizePanel />}
            </motion.div>
          )}

          {state === "converting" && (
            <motion.div
              key="queue-converting"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              exit={{ opacity: 0 }}
              className="flex flex-col gap-5"
            >
              <div className="flex flex-col gap-2">
                {files.map((item) => (
                  <FilePreview key={item.path} file={item} />
                ))}
              </div>
              <ProgressBar />
            </motion.div>
          )}

          {state === "done" && (
            <motion.div key="queue-done" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
              <StatusMessage variant="success" />
            </motion.div>
          )}

          {state === "error" && (
            <motion.div key="queue-error" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
              <StatusMessage variant="error" />
            </motion.div>
          )}
        </AnimatePresence>
      </div>

      {(state === "loaded" || state === "converting") && (
        <footer className="shrink-0 border-t border-[#3b3d46] bg-[#1b1b1d] p-3">
          <ConvertButton
            onClick={state === "converting" ? onCancel : onStart}
            mode={state === "converting" ? "cancel" : "convert"}
          />
        </footer>
      )}
    </section>
  );
}
