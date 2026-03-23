import { useEffect } from "react";
import { motion } from "framer-motion";
import { useAppStore } from "../store/useAppStore";
import { FORMAT_INFO, getCompatibleFormats, getCategoryFormats } from "../lib/formats";

export function FormatPicker() {
  const file = useAppStore((s) => s.file);
  const outputFormat = useAppStore((s) => s.outputFormat);
  const setOutputFormat = useAppStore((s) => s.setOutputFormat);

  const compatible = file ? getCompatibleFormats(file.format) : [];
  const groups = getCategoryFormats(compatible);

  // Auto-select the first compatible format when the file changes
  useEffect(() => {
    if (compatible.length > 0 && !outputFormat) {
      setOutputFormat(compatible[0]);
    }
  }, [file?.format]); // eslint-disable-line react-hooks/exhaustive-deps

  if (groups.length === 0) return null;

  return (
    <div className="flex flex-col gap-3">
      <span className="text-[13px] font-medium text-white/30 light:text-black/30">
        Convert to
      </span>

      {groups.map((group) => (
        <div key={group.category} className="flex flex-col gap-1.5">
          {groups.length > 1 && (
            <span className="text-[11px] text-white/20 light:text-black/20 font-medium">
              {group.category}
            </span>
          )}

          <div className="flex flex-wrap gap-2">
            {group.formats.map((fmt) => {
              const meta = FORMAT_INFO[fmt];
              if (!meta) return null;

              const isSelected = outputFormat === fmt;

              return (
                <motion.button
                  key={fmt}
                  type="button"
                  whileHover={{ scale: 1.04 }}
                  whileTap={{ scale: 0.96 }}
                  onClick={() => setOutputFormat(fmt)}
                  className={`
                    px-3 py-1.5 text-xs font-medium rounded-[var(--radius-pill)]
                    border transition-all duration-200
                    ${
                      isSelected
                        ? "bg-accent text-white border-accent"
                        : "bg-white/[0.04] light:bg-black/[0.03] text-white/50 light:text-black/50 border-white/[0.05] light:border-black/[0.05] hover:border-white/[0.10] light:hover:border-black/[0.10] hover:text-white/70 light:hover:text-black/70"
                    }
                  `}
                >
                  {meta.label}
                </motion.button>
              );
            })}
          </div>
        </div>
      ))}
    </div>
  );
}
