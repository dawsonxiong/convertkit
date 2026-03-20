import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";

export function ProgressBar() {
  const progress = useAppStore((s) => s.progress);
  const stage = useAppStore((s) => s.progressStage);

  const isIndeterminate = progress < 0;
  const percent = isIndeterminate ? 0 : Math.min(100, Math.max(0, progress));

  return (
    <div className="flex flex-col gap-2">
      {/* Track */}
      <div className="relative h-1.5 w-full rounded-full bg-white/[0.06] light:bg-black/[0.06] overflow-hidden">
        {isIndeterminate ? (
          <div className="absolute inset-0">
            <div className="h-full w-1/4 rounded-full bg-accent animate-indeterminate" />
          </div>
        ) : (
          <motion.div
            className="h-full rounded-full bg-accent"
            initial={{ width: 0 }}
            animate={{ width: `${percent}%` }}
            transition={{ duration: 0.3, ease: "easeOut" }}
          />
        )}
      </div>

      {/* Label row */}
      <div className="flex items-center justify-between">
        <span className="text-xs text-white/40 light:text-black/40">
          {stage || "Converting..."}
        </span>
        {!isIndeterminate && (
          <span className="text-xs tabular-nums text-white/30 light:text-black/30">
            {Math.round(percent)}%
          </span>
        )}
      </div>
    </div>
  );
}
