import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";

interface ConvertButtonProps {
  onClick: () => void;
  mode: "convert" | "cancel";
}

export function ConvertButton({ onClick, mode }: ConvertButtonProps) {
  const outputFormat = useAppStore((s) => s.outputFormat);
  const isDisabled = mode === "convert" && !outputFormat;

  return (
    <motion.button
      type="button"
      onClick={onClick}
      disabled={isDisabled}
      whileHover={isDisabled ? {} : { scale: 1.01 }}
      whileTap={isDisabled ? {} : { scale: 0.98 }}
      className={`
        w-full h-11 rounded-[var(--radius-button)] text-sm font-semibold
        flex items-center justify-center gap-2
        transition-all duration-200
        ${
          mode === "cancel"
            ? "bg-white/[0.06] light:bg-black/[0.05] text-white/60 light:text-black/60 hover:bg-white/[0.10] light:hover:bg-black/[0.08]"
            : isDisabled
              ? "bg-accent/30 text-white/30 cursor-not-allowed"
              : "bg-accent hover:bg-accent-hover text-white shadow-[0_1px_2px_rgba(0,0,0,0.2)]"
        }
      `}
    >
      {mode === "cancel" ? (
        <>
          <svg
            className="w-3.5 h-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
          Cancel
        </>
      ) : (
        "Convert"
      )}
    </motion.button>
  );
}
