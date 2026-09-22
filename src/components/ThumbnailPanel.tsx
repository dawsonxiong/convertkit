import type { ThumbnailOutputFormat } from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

const FORMATS: Array<{ value: ThumbnailOutputFormat; label: string }> = [
  { value: "jpeg", label: "JPEG" },
  { value: "png", label: "PNG" },
];

export function ThumbnailPanel() {
  const format = useAppStore((state) => state.thumbnailOutputFormat);
  const setFormat = useAppStore((state) => state.setThumbnailOutputFormat);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Image format</h3>
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
