import { useEffect } from "react";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO, getCompatibleFormats } from "../lib/formats";

export function FormatPicker() {
  const file = useAppStore((s) => s.file);
  const outputFormat = useAppStore((s) => s.outputFormat);
  const setOutputFormat = useAppStore((s) => s.setOutputFormat);

  const compatible = file ? getCompatibleFormats(file.format) : [];

  // Auto-select the first compatible format when the file changes
  useEffect(() => {
    if (compatible.length > 0 && !outputFormat) {
      setOutputFormat(compatible[0]);
    }
  }, [file?.format]); // eslint-disable-line react-hooks/exhaustive-deps

  if (compatible.length === 0) return null;

  return (
    <label className="flex flex-col gap-2 text-sm font-medium text-[#c5c6d0]">
      Output format
      <div className="relative">
        <select
          value={outputFormat ?? ""}
          onChange={(event) => setOutputFormat(event.target.value)}
          className="h-9 w-full appearance-none border border-[#44464f] bg-[#0e0e10] px-3 pr-8 text-sm font-medium text-[#e5e1e4] outline-none focus:border-[#b0c6ff]"
        >
          {compatible.map((format) => (
            <option key={format} value={format}>
              {FORMAT_INFO[format]?.label ?? format.toUpperCase()}
            </option>
          ))}
        </select>
        <svg
          className="pointer-events-none absolute right-2.5 top-1/2 size-3 -translate-y-1/2 text-[#92939d]"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="2"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="m7 9 5 5 5-5" />
        </svg>
      </div>
    </label>
  );
}
