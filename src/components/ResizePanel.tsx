import { useEffect } from "react";
import { useAppStore } from "../store/useAppStore";

const SCALE_PRESETS = [25, 50, 75, 100];
const MAX_DIMENSION = 32_768;

export function ResizePanel() {
  const file = useAppStore((store) => store.file);
  const width = useAppStore((store) => store.resizeWidth);
  const height = useAppStore((store) => store.resizeHeight);
  const preserveAspect = useAppStore((store) => store.preserveAspect);
  const setDimensions = useAppStore((store) => store.setResizeDimensions);
  const setPreserveAspect = useAppStore((store) => store.setPreserveAspect);

  const originalWidth = file?.width ?? null;
  const originalHeight = file?.height ?? null;
  const originalRatio = originalWidth && originalHeight ? originalWidth / originalHeight : null;

  useEffect(() => {
    if (width === null && height === null && originalWidth && originalHeight) {
      setDimensions(originalWidth, originalHeight);
    }
  }, [height, originalHeight, originalWidth, setDimensions, width]);

  const updateDimension = (dimension: "width" | "height", value: string) => {
    const next = value === "" ? null : Number(value);
    if (next !== null && (!Number.isInteger(next) || next < 1 || next > MAX_DIMENSION)) return;

    if (preserveAspect && originalRatio && next !== null) {
      if (dimension === "width") {
        setDimensions(next, Math.max(1, Math.round(next / originalRatio)));
      } else {
        setDimensions(Math.max(1, Math.round(next * originalRatio)), next);
      }
      return;
    }

    setDimensions(dimension === "width" ? next : width, dimension === "height" ? next : height);
  };

  const toggleAspect = () => {
    const next = !preserveAspect;
    setPreserveAspect(next);
    if (next && originalRatio && width) {
      setDimensions(width, Math.max(1, Math.round(width / originalRatio)));
    }
  };

  const applyScale = (percent: number) => {
    if (!originalWidth || !originalHeight) return;
    setDimensions(
      Math.max(1, Math.round((originalWidth * percent) / 100)),
      Math.max(1, Math.round((originalHeight * percent) / 100)),
    );
  };

  const activeScale =
    originalWidth && originalHeight && width && height
      ? SCALE_PRESETS.find(
          (percent) =>
            Math.round((originalWidth * percent) / 100) === width &&
            Math.round((originalHeight * percent) / 100) === height,
        )
      : undefined;

  return (
    <div className="flex flex-col gap-4">
      <div className="grid grid-cols-[minmax(0,1fr)_auto] items-start gap-2">
        <div className="min-w-0">
          <p className="whitespace-nowrap text-[11px] font-medium text-white/80">Dimensions</p>
          <p className="mt-1 text-[10px] leading-4 text-white/40">
            {originalWidth && originalHeight
              ? `Original ${originalWidth.toLocaleString()} × ${originalHeight.toLocaleString()} px`
              : "Enter a width and height in pixels"}
          </p>
        </div>
        <button
          type="button"
          onClick={toggleAspect}
          aria-pressed={preserveAspect}
          className={`flex h-7 items-center gap-1.5 whitespace-nowrap border px-2 text-[10px] font-medium transition-colors ${
            preserveAspect
              ? "border-[#6f7fa7] bg-[#20283a] text-[#b0c6ff]"
              : "border-[#44464f] bg-[#0e0e10] text-[#92939d] hover:text-[#e5e1e4]"
          }`}
        >
          <svg
            className="size-3"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth={1.8}
            aria-hidden="true"
          >
            <path
              d={
                preserveAspect
                  ? "M8.5 11V8a3.5 3.5 0 017 0v3M6.5 11h11v9h-11z"
                  : "M9 11V8a3.5 3.5 0 016.8-1.15M6.5 11h11v9h-11z"
              }
              strokeLinecap="round"
              strokeLinejoin="round"
            />
          </svg>
          {preserveAspect ? "Ratio locked" : "Ratio unlocked"}
        </button>
      </div>

      {originalWidth && originalHeight && (
        <div className="grid grid-cols-4 gap-2">
          {SCALE_PRESETS.map((percent) => (
            <button
              key={percent}
              type="button"
              onClick={() => applyScale(percent)}
              className={`h-7 border text-[10px] font-medium transition-colors ${
                activeScale === percent
                  ? "border-[#b0c6ff] bg-[#20283a] text-[#b0c6ff]"
                  : "border-[#44464f] bg-[#0e0e10] text-[#92939d] hover:border-[#696b75] hover:text-[#e5e1e4]"
              }`}
            >
              {percent}%
            </button>
          ))}
        </div>
      )}

      <div className="grid grid-cols-[1fr_auto_1fr] items-end gap-2">
        <label className="flex flex-col gap-1.5 text-[10px] font-medium text-white/50">
          Width
          <div className="relative">
            <input
              type="number"
              min="1"
              max={MAX_DIMENSION}
              value={width ?? ""}
              onChange={(event) => updateDimension("width", event.target.value)}
              placeholder="0"
              className="dimension-input"
            />
            <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-white/30">
              px
            </span>
          </div>
        </label>

        <span className="pb-2.5 text-[10px] text-white/30">×</span>

        <label className="flex flex-col gap-1.5 text-[10px] font-medium text-white/50">
          Height
          <div className="relative">
            <input
              type="number"
              min="1"
              max={MAX_DIMENSION}
              value={height ?? ""}
              onChange={(event) => updateDimension("height", event.target.value)}
              placeholder="0"
              className="dimension-input"
            />
            <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-white/30">
              px
            </span>
          </div>
        </label>
      </div>
    </div>
  );
}
