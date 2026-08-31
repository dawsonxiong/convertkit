import type { ArchiveFormat } from "../types";
import { ARCHIVE_FORMATS, archiveFormatLabel } from "../lib/archive";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

export function ArchiveFormatPanel() {
  const format = useAppStore((state) => state.archiveFormat);
  const setFormat = useAppStore((state) => state.setArchiveFormat);
  const password = useAppStore((state) => state.archivePassword);
  const confirmation = useAppStore((state) => state.archivePasswordConfirmation);
  const setPassword = useAppStore((state) => state.setArchivePassword);
  const setConfirmation = useAppStore((state) => state.setArchivePasswordConfirmation);
  const passwordsMismatch = Boolean(password || confirmation) && password !== confirmation;

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Archive format</h3>
      <div
        className="grid grid-cols-5 gap-2"
        role="radiogroup"
        aria-label="Archive format"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {ARCHIVE_FORMATS.map((value: ArchiveFormat) => {
          const selected = value === format;
          return (
            <button
              key={value}
              type="button"
              role="radio"
              aria-checked={selected}
              tabIndex={selected ? 0 : -1}
              onClick={() => setFormat(value)}
              className="choice-button"
            >
              {archiveFormatLabel(value)}
            </button>
          );
        })}
      </div>
      {format === "sevenZ" && (
        <div className="mt-4 grid grid-cols-2 gap-3">
          <label className="min-w-0">
            <span className="mb-1.5 block text-[11px] font-medium text-white/55">Password</span>
            <input
              type="password"
              value={password}
              maxLength={256}
              autoComplete="new-password"
              spellCheck={false}
              onChange={(event) => setPassword(event.target.value)}
              placeholder="Optional"
              className="archive-password-input"
            />
          </label>
          <label className="min-w-0">
            <span className="mb-1.5 block text-[11px] font-medium text-white/55">Confirm</span>
            <input
              type="password"
              value={confirmation}
              maxLength={256}
              autoComplete="new-password"
              spellCheck={false}
              onChange={(event) => setConfirmation(event.target.value)}
              placeholder="Repeat password"
              aria-invalid={passwordsMismatch}
              className="archive-password-input"
            />
          </label>
          {passwordsMismatch && (
            <p className="col-span-2 -mt-1 text-[10px] text-red-300/85">Passwords do not match.</p>
          )}
        </div>
      )}
    </section>
  );
}
