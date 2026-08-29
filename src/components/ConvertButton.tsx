import { useAppStore } from "../store/useAppStore";

interface ConvertButtonProps {
  onClick: () => void;
  mode: "convert" | "cancel";
  disabled?: boolean;
}

export function ConvertButton({ onClick, mode, disabled = false }: ConvertButtonProps) {
  const files = useAppStore((s) => s.files);
  const outputFormats = useAppStore((s) => s.outputFormats);
  const operation = useAppStore((s) => s.operation);
  const resizeWidth = useAppStore((s) => s.resizeWidth);
  const resizeHeight = useAppStore((s) => s.resizeHeight);
  const actionLabel =
    operation === "resize" ? "Resize" : operation === "optimize" ? "Optimize" : "Convert";
  const isDisabled =
    mode === "convert" &&
    (disabled ||
      files.length === 0 ||
      (operation === "resize"
        ? !resizeWidth || !resizeHeight
        : operation === "convert"
          ? files.some((file) => !outputFormats[file.path])
          : false));

  return (
    <button
      type="button"
      onClick={onClick}
      disabled={isDisabled}
      className={`flex h-8 w-full items-center justify-center gap-2 border text-[11px] font-medium ${
        mode === "cancel"
          ? "border-[#44464f] bg-[#201f22] text-white/70 hover:bg-[#2a2a2c] hover:text-white"
          : isDisabled
            ? "cursor-not-allowed border-[#353740] bg-[#24252a] text-white/30"
            : "border-[#b0c6ff] bg-[#b0c6ff] text-[#001944] hover:border-[#d9e2ff] hover:bg-[#d9e2ff]"
      }`}
    >
      <span className="flex items-center gap-2">
        {mode === "cancel" && (
          <svg
            className="size-3.5"
            fill="none"
            viewBox="0 0 24 24"
            stroke="currentColor"
            strokeWidth={2}
          >
            <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
          </svg>
        )}
        {mode === "cancel"
          ? "Cancel"
          : files.length > 1
            ? `${actionLabel} ${files.length} files`
            : actionLabel}
      </span>
    </button>
  );
}
