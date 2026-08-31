import { useEffect } from "react";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import {
  IMAGE_BYTES_PER_KIBIBYTE,
  IMAGE_TARGET_SIZE_MAX_KB,
  IMAGE_TARGET_SIZE_MIN_KB,
  imageTargetSizeBytesFromKb,
  imageTargetSizeKbFromBytes,
  supportsImageTargetSize,
} from "../lib/imageOptimization";
import { useAppStore } from "../store/useAppStore";

const GOALS = [
  { value: "quality", label: "Quality" },
  { value: "fileSize", label: "File size" },
] as const;

export function OptimizePanel() {
  const keepMetadata = useAppStore((store) => store.keepMetadata);
  const setKeepMetadata = useAppStore((store) => store.setKeepMetadata);
  const files = useAppStore((store) => store.files);
  const compressionGoal = useAppStore((store) => store.imageOptimizationGoal);
  const targetSizeBytes = useAppStore((store) => store.imageTargetSizeBytes);
  const setCompressionGoal = useAppStore((store) => store.setImageOptimizationGoal);
  const setTargetSizeBytes = useAppStore((store) => store.setImageTargetSizeBytes);
  const smallestSourceSize = files.reduce(
    (smallest, file) => Math.min(smallest, file.size),
    Number.POSITIVE_INFINITY,
  );
  const sourceMaximumKb = Number.isFinite(smallestSourceSize)
    ? Math.floor((smallestSourceSize - 1) / IMAGE_BYTES_PER_KIBIBYTE)
    : IMAGE_TARGET_SIZE_MAX_KB;
  const maximumTargetKb = Math.min(IMAGE_TARGET_SIZE_MAX_KB, sourceMaximumKb);
  const supportsFileSize =
    maximumTargetKb >= IMAGE_TARGET_SIZE_MIN_KB &&
    files.length > 0 &&
    files.every((file) => supportsImageTargetSize(file.format));
  const targetSizeKb = imageTargetSizeKbFromBytes(targetSizeBytes);
  const targetIsBelowEverySource =
    targetSizeBytes !== null && files.every((file) => targetSizeBytes < file.size);

  useEffect(() => {
    if (!supportsFileSize && compressionGoal === "fileSize") setCompressionGoal("quality");
  }, [compressionGoal, setCompressionGoal, supportsFileSize]);

  const updateTargetSize = (value: string) => {
    if (value === "") {
      setTargetSizeBytes(null);
      return;
    }
    setTargetSizeBytes(imageTargetSizeBytesFromKb(Number(value)));
  };

  const chooseGoal = (goal: (typeof GOALS)[number]["value"]) => {
    if (goal === "fileSize" && !targetIsBelowEverySource) {
      setTargetSizeBytes(imageTargetSizeBytesFromKb(Math.min(500, maximumTargetKb)));
    }
    setCompressionGoal(goal);
  };

  return (
    <div className="border-t border-[#3b3d46] pt-4">
      <p className="mb-3 text-[13px] font-semibold text-white/85">Optimization goal</p>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Image optimization goal"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {GOALS.map((goal) => {
          const unavailable = goal.value === "fileSize" && !supportsFileSize;
          return (
            <button
              key={goal.value}
              type="button"
              role="radio"
              onClick={() => chooseGoal(goal.value)}
              aria-checked={compressionGoal === goal.value}
              tabIndex={compressionGoal === goal.value ? 0 : -1}
              disabled={unavailable}
              title={
                unavailable
                  ? "File-size targeting supports JPEG, WebP, AVIF, and HEIC sources over 16 KB"
                  : undefined
              }
              className="choice-button"
            >
              {goal.label}
            </button>
          );
        })}
      </div>

      {compressionGoal === "fileSize" && (
        <label className="mt-4 flex flex-col gap-1.5 text-[10px] font-medium text-white/50">
          Maximum size
          <div className="relative">
            <input
              type="number"
              min={IMAGE_TARGET_SIZE_MIN_KB}
              max={maximumTargetKb}
              step="1"
              value={targetSizeKb ?? ""}
              onChange={(event) => updateTargetSize(event.target.value)}
              placeholder="500"
              aria-label="Target image size in kibibytes"
              aria-invalid={targetSizeKb === null || !targetIsBelowEverySource}
              title="Target must be a whole kibibyte smaller than every source image"
              className="dimension-input"
            />
            <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-white/30">
              KB
            </span>
          </div>
        </label>
      )}

      <div className="mt-4 flex items-center justify-between gap-4 border-t border-[#2c2d33] pt-4">
        <p className="text-[13px] font-semibold text-white/85">Metadata</p>
        <div
          className="grid grid-cols-2 divide-x divide-[#44464f] border border-[#44464f] bg-[#0e0e10]"
          role="radiogroup"
          aria-label="Metadata handling"
          onKeyDown={handleRadioGroupKeyDown}
        >
          <button
            type="button"
            role="radio"
            onClick={() => setKeepMetadata(false)}
            aria-checked={!keepMetadata}
            tabIndex={!keepMetadata ? 0 : -1}
            className="segmented-button"
          >
            Remove
          </button>
          <button
            type="button"
            role="radio"
            onClick={() => setKeepMetadata(true)}
            aria-checked={keepMetadata}
            tabIndex={keepMetadata ? 0 : -1}
            className="segmented-button"
          >
            Keep
          </button>
        </div>
      </div>
    </div>
  );
}
