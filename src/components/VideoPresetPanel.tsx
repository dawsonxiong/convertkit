import type {
  VideoCompressionGoal,
  VideoEncodingPreset,
  VideoQuality,
  VideoResolution,
} from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";
import {
  VIDEO_TARGET_SIZE_MAX_MB,
  VIDEO_TARGET_SIZE_MIN_MB,
  videoTargetSizeBytesFromMb,
  videoTargetSizeMbFromBytes,
} from "../lib/videoCompression";

const PRESETS: Array<{
  value: VideoEncodingPreset;
  label: string;
  detail: string;
}> = [
  { value: "compatible", label: "Compatible", detail: "MP4 · H.264" },
  { value: "smaller", label: "Smaller", detail: "MP4 · H.265 · 1080p" },
  { value: "web", label: "Web", detail: "WebM · VP9 · 1080p" },
  { value: "archive", label: "Lossless", detail: "MKV · FFV1" },
];

const RESOLUTIONS: Array<{ value: VideoResolution; label: string }> = [
  { value: "automatic", label: "Auto" },
  { value: "original", label: "Original" },
  { value: "fullHd", label: "1080p" },
  { value: "hd", label: "720p" },
];

const QUALITIES: Array<{ value: VideoQuality; label: string }> = [
  { value: "high", label: "High" },
  { value: "balanced", label: "Balanced" },
  { value: "smallest", label: "Smallest" },
];

const COMPRESSION_GOALS: Array<{ value: VideoCompressionGoal; label: string }> = [
  { value: "quality", label: "Quality" },
  { value: "fileSize", label: "File size" },
];

export function VideoPresetPanel() {
  const preset = useAppStore((state) => state.videoEncodingPreset);
  const resolution = useAppStore((state) => state.videoResolution);
  const quality = useAppStore((state) => state.videoQuality);
  const compressionGoal = useAppStore((state) => state.videoCompressionGoal);
  const targetSizeBytes = useAppStore((state) => state.videoTargetSizeBytes);
  const setPreset = useAppStore((state) => state.setVideoEncodingPreset);
  const setResolution = useAppStore((state) => state.setVideoResolution);
  const setQuality = useAppStore((state) => state.setVideoQuality);
  const setCompressionGoal = useAppStore((state) => state.setVideoCompressionGoal);
  const setTargetSizeBytes = useAppStore((state) => state.setVideoTargetSizeBytes);
  const targetSizeMb = videoTargetSizeMbFromBytes(targetSizeBytes);

  const updateTargetSize = (value: string) => {
    if (value === "") {
      setTargetSizeBytes(null);
      return;
    }
    const bytes = videoTargetSizeBytesFromMb(Number(value));
    setTargetSizeBytes(bytes);
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Preset</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Video preset"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {PRESETS.map((option) => {
          const active = option.value === preset;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={active}
              tabIndex={active ? 0 : -1}
              onClick={() => setPreset(option.value)}
              className="choice-button choice-button-detail min-w-0 justify-between text-left"
            >
              <span className="text-[11px] font-medium">{option.label}</span>
              <span className="truncate text-[9px] text-current opacity-60">{option.detail}</span>
            </button>
          );
        })}
      </div>

      {preset !== "archive" && (
        <>
          <h3 className="mb-3 mt-4 text-[13px] font-semibold text-white/85">Maximum resolution</h3>
          <div
            className="grid grid-cols-4 gap-2"
            role="radiogroup"
            aria-label="Maximum resolution"
            onKeyDown={handleRadioGroupKeyDown}
          >
            {RESOLUTIONS.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={resolution === option.value}
                tabIndex={resolution === option.value ? 0 : -1}
                onClick={() => setResolution(option.value)}
                className="choice-button"
                title={
                  option.value === "automatic"
                    ? "Use the selected preset's default size"
                    : undefined
                }
              >
                {option.label}
              </button>
            ))}
          </div>
        </>
      )}

      <h3 className="mb-3 mt-4 text-[13px] font-semibold text-white/85">Compression goal</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Compression goal"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {COMPRESSION_GOALS.map((option) => {
          const unavailable = preset === "archive" && option.value === "fileSize";
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={compressionGoal === option.value}
              tabIndex={compressionGoal === option.value ? 0 : -1}
              disabled={unavailable}
              title={unavailable ? "File-size targeting is unavailable for Lossless" : undefined}
              onClick={() => setCompressionGoal(option.value)}
              className="choice-button"
            >
              {option.label}
            </button>
          );
        })}
      </div>

      {preset !== "archive" && compressionGoal === "quality" && (
        <>
          <h3 className="mb-3 mt-4 text-[13px] font-semibold text-white/85">Quality level</h3>
          <div
            className="grid grid-cols-3 gap-2"
            role="radiogroup"
            aria-label="Video quality"
            onKeyDown={handleRadioGroupKeyDown}
          >
            {QUALITIES.map((option) => (
              <button
                key={option.value}
                type="button"
                role="radio"
                aria-checked={quality === option.value}
                tabIndex={quality === option.value ? 0 : -1}
                onClick={() => setQuality(option.value)}
                className="choice-button"
              >
                {option.label}
              </button>
            ))}
          </div>
        </>
      )}

      {preset !== "archive" && compressionGoal === "fileSize" && (
        <label className="mt-4 flex flex-col gap-1.5 text-[10px] font-medium text-white/50">
          Target size
          <div className="relative">
            <input
              type="number"
              min={VIDEO_TARGET_SIZE_MIN_MB}
              max={VIDEO_TARGET_SIZE_MAX_MB}
              step="1"
              value={targetSizeMb ?? ""}
              onChange={(event) => updateTargetSize(event.target.value)}
              placeholder="100"
              aria-label="Target file size in megabytes"
              aria-invalid={targetSizeMb === null}
              className="dimension-input"
            />
            <span className="pointer-events-none absolute right-2.5 top-1/2 -translate-y-1/2 text-[10px] text-white/30">
              MB
            </span>
          </div>
        </label>
      )}
    </section>
  );
}
