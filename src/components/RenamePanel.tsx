import type { RenameNumbering } from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

const NUMBERING: Array<{ value: RenameNumbering; label: string }> = [
  { value: "none", label: "None" },
  { value: "prefix", label: "Before" },
  { value: "suffix", label: "After" },
];

const inputClass =
  "h-8 w-full min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]";

export function RenamePanel() {
  const settings = useAppStore((state) => state.renameSettings);
  const setSettings = useAppStore((state) => state.setRenameSettings);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[13px] font-semibold text-white/85">Naming</h3>
        <button
          type="button"
          onClick={() =>
            setSettings({
              find: "",
              replace: "",
              prefix: "",
              suffix: "",
              numbering: "none",
              start: 1,
              padding: 2,
            })
          }
          className="text-button"
        >
          Reset
        </button>
      </div>

      <div className="grid grid-cols-2 gap-2">
        <label className="min-w-0">
          <span className="mb-1.5 block text-[10px] font-medium text-white/45">Find</span>
          <input
            type="text"
            value={settings.find}
            onChange={(event) => setSettings({ find: event.target.value })}
            placeholder="Text to replace"
            spellCheck={false}
            className={inputClass}
          />
        </label>
        <label className="min-w-0">
          <span className="mb-1.5 block text-[10px] font-medium text-white/45">Replace</span>
          <input
            type="text"
            value={settings.replace}
            onChange={(event) => setSettings({ replace: event.target.value })}
            placeholder="Replacement"
            spellCheck={false}
            className={inputClass}
          />
        </label>
        <label className="min-w-0">
          <span className="mb-1.5 block text-[10px] font-medium text-white/45">Prefix</span>
          <input
            type="text"
            value={settings.prefix}
            onChange={(event) => setSettings({ prefix: event.target.value })}
            placeholder="Before the name"
            spellCheck={false}
            className={inputClass}
          />
        </label>
        <label className="min-w-0">
          <span className="mb-1.5 block text-[10px] font-medium text-white/45">Suffix</span>
          <input
            type="text"
            value={settings.suffix}
            onChange={(event) => setSettings({ suffix: event.target.value })}
            placeholder="After the name"
            spellCheck={false}
            className={inputClass}
          />
        </label>
      </div>

      <div className="mt-3 flex items-end gap-2">
        <div className="min-w-0 flex-1">
          <span className="mb-1.5 block text-[10px] font-medium text-white/45">Numbering</span>
          <div
            className="grid h-8 grid-cols-3 divide-x divide-[#44464f] border border-[#44464f] bg-[#101012]"
            role="radiogroup"
            aria-label="Numbering placement"
            onKeyDown={handleRadioGroupKeyDown}
          >
            {NUMBERING.map((option) => {
              const selected = settings.numbering === option.value;
              return (
                <button
                  key={option.value}
                  type="button"
                  role="radio"
                  onClick={() => setSettings({ numbering: option.value })}
                  aria-checked={selected}
                  tabIndex={selected ? 0 : -1}
                  className="segmented-button"
                >
                  {option.label}
                </button>
              );
            })}
          </div>
        </div>

        {settings.numbering !== "none" && (
          <>
            <label className="w-16 shrink-0">
              <span className="mb-1.5 block text-[10px] font-medium text-white/45">Start</span>
              <input
                type="number"
                min={0}
                max={999999}
                value={settings.start}
                onChange={(event) =>
                  setSettings({ start: Math.max(0, Number(event.target.value) || 0) })
                }
                className={inputClass}
              />
            </label>
            <label className="w-16 shrink-0">
              <span className="mb-1.5 block text-[10px] font-medium text-white/45">Digits</span>
              <input
                type="number"
                min={1}
                max={6}
                value={settings.padding}
                onChange={(event) =>
                  setSettings({
                    padding: Math.max(1, Math.min(6, Number(event.target.value) || 1)),
                  })
                }
                className={inputClass}
              />
            </label>
          </>
        )}
      </div>
    </section>
  );
}
