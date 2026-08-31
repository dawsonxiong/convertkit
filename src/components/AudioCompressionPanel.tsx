import type { AudioCompressionPreset } from "../types";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";

const PRESETS: Array<{
  value: AudioCompressionPreset;
  label: string;
  bitrate: string;
}> = [
  { value: "high", label: "High", bitrate: "256 kbps" },
  { value: "balanced", label: "Balanced", bitrate: "160 kbps" },
  { value: "smallest", label: "Smallest", bitrate: "96 kbps" },
];

export function AudioCompressionPanel() {
  const preset = useAppStore((state) => state.audioCompressionPreset);
  const setPreset = useAppStore((state) => state.setAudioCompressionPreset);

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Quality level</h3>
      <div
        className="grid grid-cols-3 gap-2"
        role="radiogroup"
        aria-label="Audio quality level"
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
              className="choice-button choice-button-detail min-w-0 flex-col items-start gap-0.5 text-left"
            >
              <span className="text-[11px] font-medium">{option.label}</span>
              <span className="text-[9px] text-current opacity-60">{option.bitrate}</span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
