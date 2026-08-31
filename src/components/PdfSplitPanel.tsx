import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

export function PdfSplitPanel() {
  const mode = useAppStore((state) => state.pdfSplitMode);
  const pageSelection = useAppStore((state) => state.pdfPageSelection);
  const setMode = useAppStore((state) => state.setPdfSplitMode);
  const setPageSelection = useAppStore((state) => state.setPdfPageSelection);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Pages</h3>

      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="PDF split mode"
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
          <label htmlFor="pdf-page-selection" className="text-[11px] font-medium text-white/45">
            Pages
          </label>
          <input
            id="pdf-page-selection"
            type="text"
            value={pageSelection}
            onChange={(event) => setPageSelection(event.target.value)}
            placeholder="1-3, 5, 8-10"
            spellCheck={false}
            className="h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]"
          />
        </div>
      )}
    </section>
  );
}
