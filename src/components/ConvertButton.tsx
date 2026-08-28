import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";

interface ConvertButtonProps {
  onClick: () => void;
  mode: "convert" | "cancel";
}

export function ConvertButton({ onClick, mode }: ConvertButtonProps) {
  const files = useAppStore((s) => s.files);
  const outputFormats = useAppStore((s) => s.outputFormats);
  const operation = useAppStore((s) => s.operation);
  const resizeWidth = useAppStore((s) => s.resizeWidth);
  const resizeHeight = useAppStore((s) => s.resizeHeight);
  const isDisabled =
    mode === "convert" &&
    (files.length === 0 ||
      (operation === "resize"
        ? !resizeWidth || !resizeHeight
        : files.some((file) => !outputFormats[file.path])));

  return (
    <motion.button
      type="button"
      onClick={onClick}
      disabled={isDisabled}
      className={`flex h-8 w-full items-center justify-center gap-2 border text-[11px] font-medium transition-colors ${
        mode === "cancel"
          ? "border-[#44464f] bg-[#201f22] text-white/70 hover:bg-[#2a2a2c] hover:text-white"
          : isDisabled
            ? "cursor-not-allowed border-[#353740] bg-[#24252a] text-white/30"
            : "border-[#b0c6ff] bg-[#b0c6ff] text-[#001944] hover:border-[#d9e2ff] hover:bg-[#d9e2ff]"
      }`}
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
      ) : operation === "resize" ? (
        files.length > 1 ? (
          `Resize ${files.length} files`
        ) : (
          "Resize"
        )
      ) : files.length > 1 ? (
        `Convert ${files.length} files`
      ) : (
        "Convert"
      )}
    </motion.button>
  );
}
