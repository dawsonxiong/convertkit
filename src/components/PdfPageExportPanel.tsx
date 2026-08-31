import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

const FORMATS = [
  { value: "png", label: "PNG" },
  { value: "jpeg", label: "JPEG" },
] as const;

const RESOLUTIONS = [
  { value: "screen", label: "Screen", detail: "144 dpi" },
  { value: "print", label: "Print", detail: "300 dpi" },
] as const;

export function PdfPageExportPanel() {
  const mode = useAppStore((state) => state.pdfSplitMode);
  const pageSelection = useAppStore((state) => state.pdfPageSelection);
  const format = useAppStore((state) => state.pdfPageImageFormat);
  const resolution = useAppStore((state) => state.pdfPageImageResolution);
  const setMode = useAppStore((state) => state.setPdfSplitMode);
  const setPageSelection = useAppStore((state) => state.setPdfPageSelection);
  const setFormat = useAppStore((state) => state.setPdfPageImageFormat);
  const setResolution = useAppStore((state) => state.setPdfPageImageResolution);

  return (
    <section>
      <h3 className="mb-2 text-[11px] font-medium text-white/70">Pages</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="PDF page mode"
        onKeyDown={handleRadioGroupKeyDown}
      >
        <button
          type="button"
          role="radio"
          onClick={() => setMode("everyPage")}
          aria-checked={mode === "everyPage"}
          tabIndex={mode === "everyPage" ? 0 : -1}
          className="choice-button"
        >
          Every page
        </button>
        <button
          type="button"
          role="radio"
          onClick={() => setMode("extract")}
          aria-checked={mode === "extract"}
          tabIndex={mode === "extract" ? 0 : -1}
          className="choice-button"
        >
          Select pages
        </button>
      </div>

      {mode === "extract" && (
        <div className="mt-3 grid grid-cols-[78px_minmax(0,1fr)] items-center gap-3">
          <label
            htmlFor="pdf-image-page-selection"
            className="text-[11px] font-medium text-white/45"
          >
            Page range
          </label>
          <input
            id="pdf-image-page-selection"
            type="text"
            value={pageSelection}
            onChange={(event) => setPageSelection(event.target.value)}
            placeholder="1-3, 5, 8-10"
            spellCheck={false}
            className="h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]"
          />
        </div>
      )}

      <h3 className="mb-2 mt-4 text-[11px] font-medium text-white/70">Image format</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Image format"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {FORMATS.map((option) => (
          <button
            key={option.value}
            type="button"
            role="radio"
            aria-checked={format === option.value}
            tabIndex={format === option.value ? 0 : -1}
            onClick={() => setFormat(option.value)}
            className="choice-button"
          >
            {option.label}
          </button>
        ))}
      </div>

      <h3 className="mb-2 mt-4 text-[11px] font-medium text-white/70">Resolution</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Resolution"
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
            className="choice-button min-w-0 justify-between"
          >
            <span>{option.label}</span>
            <span className="text-[10px] font-normal text-white/35">{option.detail}</span>
          </button>
        ))}
      </div>
    </section>
  );
}
