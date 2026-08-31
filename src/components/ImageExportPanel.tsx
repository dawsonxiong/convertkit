import { useAppStore } from "../store/useAppStore";
import type { ImageExportPreset } from "../types";

const VARIANTS: Array<{
  value: ImageExportPreset;
  label: string;
  detail: string;
}> = [
  { value: "web", label: "Web", detail: "WebP · max 1,920 px" },
  { value: "email", label: "Email", detail: "JPEG · max 1,600 px" },
  { value: "social", label: "Social", detail: "JPEG · max 2,048 px" },
  { value: "preview", label: "Preview", detail: "WebP · max 640 px" },
];

export function ImageExportPanel() {
  const selected = useAppStore((state) => state.imageExportPresets);
  const setSelected = useAppStore((state) => state.setImageExportPresets);

  const toggle = (preset: ImageExportPreset) => {
    setSelected(
      selected.includes(preset)
        ? selected.filter((candidate) => candidate !== preset)
        : [...selected, preset],
    );
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <div className="mb-3 flex items-baseline justify-between gap-3">
        <h3 className="text-[13px] font-semibold text-white/85">Variants</h3>
        <span className="text-[10px] text-white/35">
          {selected.length} {selected.length === 1 ? "output" : "outputs"} per image
        </span>
      </div>

      <div className="divide-y divide-white/[0.06] bg-[#101012]">
        {VARIANTS.map((variant) => {
          const active = selected.includes(variant.value);
          return (
            <button
              key={variant.value}
              type="button"
              onClick={() => toggle(variant.value)}
              aria-pressed={active}
              className="choice-row flex h-10 w-full items-center gap-2.5 px-2.5 text-left"
            >
              <span
                className={`grid size-4 shrink-0 place-items-center border ${
                  active
                    ? "border-[#9bb6ff] bg-[#9bb6ff] text-[#0b1d43]"
                    : "border-[#555761] text-transparent"
                }`}
                aria-hidden="true"
              >
                <svg
                  className="size-2.5"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="2.5"
                >
                  <path d="m5 12 4 4 10-10" />
                </svg>
              </span>
              <span className="text-[11px] font-medium text-current">{variant.label}</span>
              <span className="ml-auto text-[10px] text-white/35">{variant.detail}</span>
            </button>
          );
        })}
      </div>
    </section>
  );
}
