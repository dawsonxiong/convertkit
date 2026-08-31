import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";
import type { OcrOutputFormat } from "../types";

const OUTPUT_FORMATS: Array<{ value: OcrOutputFormat; label: string }> = [
  { value: "text", label: "Plain text" },
  { value: "searchablePdf", label: "Searchable PDF" },
];

export function OcrOutputPanel() {
  const outputFormat = useAppStore((state) => state.ocrOutputFormat);
  const setOutputFormat = useAppStore((state) => state.setOcrOutputFormat);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Output</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="OCR output"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {OUTPUT_FORMATS.map((option) => {
          const selected = option.value === outputFormat;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={selected}
              tabIndex={selected ? 0 : -1}
              onClick={() => setOutputFormat(option.value)}
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
