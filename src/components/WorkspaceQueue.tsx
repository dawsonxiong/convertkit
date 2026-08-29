import { ConvertButton } from "./ConvertButton";
import { FilePreview } from "./FilePreview";
import { OptimizePanel } from "./OptimizePanel";
import { OutputSettings } from "./OutputSettings";
import { ResizePanel } from "./ResizePanel";
import { useAppStore } from "../store/useAppStore";

interface WorkspaceQueueProps {
  onStart: (paths?: string[]) => void;
  onCancel: () => void;
  onCancelItem: () => void;
}

export function WorkspaceQueue({ onStart, onCancel, onCancelItem }: WorkspaceQueueProps) {
  const state = useAppStore((store) => store.state);
  const operation = useAppStore((store) => store.operation);
  const files = useAppStore((store) => store.files);
  const file = useAppStore((store) => store.file);
  const queueItems = useAppStore((store) => store.queueItems);
  const skipQueueItem = useAppStore((store) => store.skipQueueItem);
  const reset = useAppStore((store) => store.reset);

  const canClear = Boolean(file) && state !== "converting";
  const completedCount = files.filter(
    (item) => queueItems[item.path]?.status === "completed",
  ).length;
  const failedCount = files.filter((item) => queueItems[item.path]?.status === "failed").length;
  const skippedCount = files.filter((item) =>
    ["skipped", "cancelled"].includes(queueItems[item.path]?.status ?? ""),
  ).length;
  const processedCount = completedCount + failedCount + skippedCount;
  const retryablePaths = files
    .filter((item) =>
      ["failed", "skipped", "cancelled"].includes(queueItems[item.path]?.status ?? ""),
    )
    .map((item) => item.path);

  return (
    <section className="flex min-h-0 flex-col border border-[#3b3d46] bg-[#131315]">
      <header className="flex h-11 shrink-0 items-center justify-between border-b border-[#3b3d46] bg-[#1b1b1d] px-3">
        <div className="flex min-w-0 items-center gap-2">
          <h2 className="shrink-0 text-[13px] font-semibold text-white/85">
            Queue ({files.length})
          </h2>
          {state === "done" && processedCount > 0 && (
            <div className="flex min-w-0 items-center gap-1.5 text-[11px] font-medium">
              <span aria-hidden="true" className="text-white/20">
                ·
              </span>
              {completedCount > 0 && (
                <span className="whitespace-nowrap text-emerald-300">
                  {completedCount} complete
                </span>
              )}
              {failedCount > 0 && (
                <span className="whitespace-nowrap text-red-300">{failedCount} failed</span>
              )}
              {skippedCount > 0 && (
                <span className="whitespace-nowrap text-white/40">{skippedCount} skipped</span>
              )}
            </div>
          )}
        </div>
        {canClear && (
          <button
            type="button"
            onClick={reset}
            className="text-[11px] text-white/45 hover:text-white/80"
          >
            Clear
          </button>
        )}
      </header>

      <div className="queue-grid queue-scroll min-h-0 flex-1 overflow-y-auto p-3">
        {state === "empty" ? (
          <div className="grid h-full min-h-40 place-items-center text-sm text-white/40">
            No files added
          </div>
        ) : (
          <div className="flex flex-col gap-4">
            <div className="flex flex-col gap-2">
              {files.map((item) => (
                <FilePreview
                  key={item.path}
                  file={item}
                  canDismiss={state === "loaded"}
                  onCancel={state === "converting" ? onCancelItem : undefined}
                  onSkip={state === "converting" ? () => skipQueueItem(item.path) : undefined}
                  onRetry={state === "done" ? () => onStart([item.path]) : undefined}
                />
              ))}
            </div>

            {state === "loaded" && operation === "resize" && <ResizePanel />}
            {state === "loaded" && operation === "optimize" && <OptimizePanel />}
            {state === "loaded" && <OutputSettings />}

            {state === "converting" && (
              <p className="text-[10px] text-white/40">
                {processedCount} of {files.length} processed
              </p>
            )}
          </div>
        )}
      </div>

      {(state === "loaded" || state === "converting" || state === "done") && (
        <footer className="shrink-0 border-t border-[#3b3d46] bg-[#1b1b1d] p-3">
          <div className="flex gap-2">
            {state === "done" && retryablePaths.length > 0 && (
              <button
                type="button"
                onClick={() => onStart(retryablePaths)}
                className="h-8 flex-1 border border-[#44464f] bg-[#201f22] text-[10px] font-medium text-white/65 hover:bg-[#2a2a2c] hover:text-white"
              >
                Retry {retryablePaths.length}
              </button>
            )}
            <div className="flex-1">
              <ConvertButton
                onClick={state === "converting" ? onCancel : () => onStart()}
                mode={state === "converting" ? "cancel" : "convert"}
                disabled={state === "done" && completedCount === files.length}
              />
            </div>
          </div>
        </footer>
      )}
    </section>
  );
}
