import { useAppStore } from "../store/useAppStore";

export function OptimizePanel() {
  const keepMetadata = useAppStore((store) => store.keepMetadata);
  const setKeepMetadata = useAppStore((store) => store.setKeepMetadata);

  return (
    <div className="border-t border-[#3b3d46] pt-4">
      <div className="flex items-center justify-between gap-4">
        <p className="text-[11px] font-medium text-white/80">Metadata</p>
        <div className="grid grid-cols-2 border border-[#44464f] bg-[#0e0e10]">
          <button
            type="button"
            onClick={() => setKeepMetadata(false)}
            aria-pressed={!keepMetadata}
            className={`h-7 px-3 text-[10px] font-medium transition-colors ${
              !keepMetadata ? "bg-[#20283a] text-[#b0c6ff]" : "text-white/45 hover:text-white/75"
            }`}
          >
            Remove
          </button>
          <button
            type="button"
            onClick={() => setKeepMetadata(true)}
            aria-pressed={keepMetadata}
            className={`h-7 border-l border-[#44464f] px-3 text-[10px] font-medium transition-colors ${
              keepMetadata ? "bg-[#20283a] text-[#b0c6ff]" : "text-white/45 hover:text-white/75"
            }`}
          >
            Keep
          </button>
        </div>
      </div>
    </div>
  );
}
