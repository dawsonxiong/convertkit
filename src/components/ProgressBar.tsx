import { useAppStore } from "../store/useAppStore";

export function ProgressBar() {
  const progress = useAppStore((s) => s.progress);
  const stage = useAppStore((s) => s.progressStage);

  const isIndeterminate = progress < 0;
  const percent = isIndeterminate ? 0 : Math.min(100, Math.max(0, progress));

  return (
    <div className="flex flex-col gap-2">
      {/* Track */}
      <div className="relative h-1.5 w-full overflow-hidden rounded-full bg-white/[0.065]">
        {isIndeterminate ? (
          <div className="absolute inset-0">
            <div className="h-full w-1/4 animate-indeterminate rounded-full bg-blue-500" />
          </div>
        ) : (
          <div
            className="h-full rounded-full bg-blue-500 transition-[width] duration-150 ease-out"
            style={{ width: `${percent}%` }}
          />
        )}
      </div>

      {/* Label row */}
      <div className="flex items-center justify-between">
        <span className="text-xs text-white/50">{stage || "Working…"}</span>
        {!isIndeterminate && (
          <span className="text-xs tabular-nums text-white/40">{Math.round(percent)}%</span>
        )}
      </div>
    </div>
  );
}
