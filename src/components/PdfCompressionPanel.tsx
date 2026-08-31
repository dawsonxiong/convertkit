import type { PdfCompressionGoal, PdfCompressionPreset } from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import {
  PDF_BYTES_PER_MEBIBYTE,
  PDF_TARGET_SIZE_MAX_MB,
  PDF_TARGET_SIZE_MIN_MB,
  pdfTargetSizeBytesFromMb,
  pdfTargetSizeMbFromBytes,
} from "../lib/pdfCompression";
import { useAppStore } from "../store/useAppStore";

const PRESETS: Array<{ value: PdfCompressionPreset; label: string }> = [
  { value: "high", label: "High quality" },
  { value: "balanced", label: "Balanced" },
  { value: "smallest", label: "Smallest" },
];

const GOALS: Array<{ value: PdfCompressionGoal; label: string }> = [
  { value: "quality", label: "Quality" },
  { value: "fileSize", label: "File size" },
];

export function PdfCompressionPanel() {
  const preset = useAppStore((state) => state.pdfCompressionPreset);
  const setPreset = useAppStore((state) => state.setPdfCompressionPreset);
  const goal = useAppStore((state) => state.pdfCompressionGoal);
  const targetSizeBytes = useAppStore((state) => state.pdfTargetSizeBytes);
  const files = useAppStore((state) => state.files);
  const setGoal = useAppStore((state) => state.setPdfCompressionGoal);
  const setTargetSizeBytes = useAppStore((state) => state.setPdfTargetSizeBytes);
  const targetSizeMb = pdfTargetSizeMbFromBytes(targetSizeBytes);
  const smallestSourceSize = files.reduce(
    (smallest, file) => Math.min(smallest, file.size),
    Number.POSITIVE_INFINITY,
  );
  const sourceMaximumMb = Number.isFinite(smallestSourceSize)
    ? Math.floor((smallestSourceSize - 1) / PDF_BYTES_PER_MEBIBYTE)
    : PDF_TARGET_SIZE_MAX_MB;
  const maximumTargetMb = Math.min(PDF_TARGET_SIZE_MAX_MB, sourceMaximumMb);
  const targetIsBelowEverySource =
    targetSizeBytes !== null && files.every((file) => targetSizeBytes < file.size);

  const updateTargetSize = (value: string) => {
    if (value === "") {
      setTargetSizeBytes(null);
      return;
    }
    setTargetSizeBytes(pdfTargetSizeBytesFromMb(Number(value)));
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Compression goal</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="PDF compression goal"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {GOALS.map((item) => {
          const selected = item.value === goal;
          return (
            <button
              key={item.value}
              type="button"
              role="radio"
              onClick={() => setGoal(item.value)}
              aria-checked={selected}
              tabIndex={selected ? 0 : -1}
              className="choice-button"
            >
              {item.label}
            </button>
          );
        })}
      </div>

      {goal === "quality" ? (
        <>
          <h3 className="mb-3 mt-4 text-[13px] font-semibold text-white/85">Quality level</h3>
          <div
            className="grid grid-cols-3 gap-2"
            role="radiogroup"
            aria-label="PDF quality level"
            onKeyDown={handleRadioGroupKeyDown}
          >
            {PRESETS.map((item) => {
              const selected = item.value === preset;
              return (
                <button
                  key={item.value}
                  type="button"
                  role="radio"
                  onClick={() => setPreset(item.value)}
                  aria-checked={selected}
                  tabIndex={selected ? 0 : -1}
                  className="choice-button"
                >
                  {item.label}
                </button>
              );
            })}
          </div>
        </>
      ) : (
        <label className="mt-4 flex flex-col gap-1.5 text-[10px] font-medium text-white/50">
          Target size
          <div className="relative">
            <input
              type="number"
              min={PDF_TARGET_SIZE_MIN_MB}
              max={maximumTargetMb}
              step="1"
              value={targetSizeMb ?? ""}
              onChange={(event) => updateTargetSize(event.target.value)}
              placeholder="10"
              aria-label="Target PDF size in megabytes"
              aria-invalid={targetSizeMb === null || !targetIsBelowEverySource}
              title="Target must be a whole megabyte smaller than every source PDF"
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
