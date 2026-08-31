import type { ThumbnailMode, ThumbnailOutputFormat } from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

const MODES: Array<{ value: ThumbnailMode; label: string; detail: string }> = [
  { value: "frame", label: "Thumbnail", detail: "Midpoint frame" },
  { value: "contactSheet", label: "Contact sheet", detail: "12-frame grid" },
];

const FORMATS: Array<{ value: ThumbnailOutputFormat; label: string }> = [
  { value: "jpeg", label: "JPEG" },
  { value: "png", label: "PNG" },
];

export function ThumbnailPanel() {
  const mode = useAppStore((state) => state.thumbnailMode);
  const format = useAppStore((state) => state.thumbnailOutputFormat);
  const setMode = useAppStore((state) => state.setThumbnailMode);
  const setFormat = useAppStore((state) => state.setThumbnailOutputFormat);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Layout</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Thumbnail layout"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {MODES.map((option) => {
          const active = option.value === mode;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={active}
              tabIndex={active ? 0 : -1}
              onClick={() => setMode(option.value)}
              className="choice-button choice-button-detail min-w-0 flex-col items-start gap-0.5 text-left"
            >
              <span className="text-[11px] font-medium leading-none">{option.label}</span>
              <span className="text-[9px] leading-none text-current opacity-60">
                {option.detail}
              </span>
            </button>
          );
        })}
      </div>

      <h3 className="mb-3 mt-4 text-[13px] font-semibold text-white/85">Image format</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Thumbnail format"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {FORMATS.map((option) => {
          const active = option.value === format;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={active}
              tabIndex={active ? 0 : -1}
              onClick={() => setFormat(option.value)}
              className="choice-button"
            >
              {option.label}
            </button>
          );
        })}
      </div>
    </section>
  );
}
