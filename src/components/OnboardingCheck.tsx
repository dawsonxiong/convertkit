import { useCallback, useEffect, useState } from "react";
import { motion } from "framer-motion";
import { checkDependencies } from "../lib/tauri";
import type { DependencyStatus } from "../types";

interface OnboardingCheckProps {
  onReady: () => void;
}

export function OnboardingCheck({ onReady }: OnboardingCheckProps) {
  const [deps, setDeps] = useState<DependencyStatus[]>([]);
  const [checking, setChecking] = useState(true);

  const check = useCallback(async () => {
    setChecking(true);
    try {
      const result = await checkDependencies();
      setDeps(result);

      // If all required deps are installed, proceed automatically.
      const allRequired = result.filter((d) => d.required);
      if (allRequired.every((d) => d.installed)) {
        onReady();
      }
    } catch {
      // If check itself fails, let the user through anyway.
      onReady();
    } finally {
      setChecking(false);
    }
  }, [onReady]);

  useEffect(() => {
    check();
  }, [check]);

  const missingRequired = deps.filter((d) => d.required && !d.installed);
  const brewCommand = `brew install ${missingRequired.map((d) => d.name).join(" ")}`;

  const handleCopy = useCallback(() => {
    navigator.clipboard.writeText(brewCommand);
  }, [brewCommand]);

  if (checking) {
    return (
      <div className="flex items-center justify-center h-full">
        <p className="text-sm text-white/40 light:text-black/40">Checking dependencies…</p>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      className="flex flex-col items-center gap-5 px-6 py-10"
    >
      <div className="w-12 h-12 rounded-full bg-accent/10 flex items-center justify-center">
        <svg
          className="w-6 h-6 text-accent"
          fill="none"
          viewBox="0 0 24 24"
          stroke="currentColor"
          strokeWidth={1.5}
        >
          <path
            strokeLinecap="round"
            strokeLinejoin="round"
            d="M11.42 15.17l-5.657-5.657m0 0L11.42 3.856m-5.657 5.657h13.285"
          />
        </svg>
      </div>

      <div className="flex flex-col items-center gap-1 text-center">
        <p className="text-sm font-medium text-white/90 light:text-black/90">
          Missing required tools
        </p>
        <p className="text-xs text-white/40 light:text-black/40 max-w-xs">
          ConvertKit needs a few CLI tools installed. Run this in Terminal:
        </p>
      </div>

      {/* Brew command */}
      <button
        type="button"
        onClick={handleCopy}
        className="
          w-full max-w-xs px-3 py-2 text-xs font-mono text-left rounded-[var(--radius-button)]
          bg-white/[0.04] light:bg-black/[0.03]
          text-white/60 light:text-black/60
          border border-white/[0.06] light:border-black/[0.06]
          hover:border-white/[0.14] light:hover:border-black/[0.14]
          transition-all duration-200
        "
        title="Click to copy"
      >
        $ {brewCommand}
        <span className="float-right text-white/20 light:text-black/20">copy</span>
      </button>

      {/* Dep status list */}
      <div className="w-full max-w-xs flex flex-col gap-1.5">
        {deps.map((dep) => (
          <div
            key={dep.name}
            className="flex items-center justify-between text-xs px-1"
          >
            <span className="text-white/50 light:text-black/50">{dep.name}</span>
            <span
              className={
                dep.installed
                  ? "text-success"
                  : dep.required
                    ? "text-error"
                    : "text-white/25 light:text-black/25"
              }
            >
              {dep.installed ? (dep.version ?? "installed") : dep.required ? "missing" : "optional"}
            </span>
          </div>
        ))}
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2 w-full max-w-xs">
        <button
          type="button"
          onClick={check}
          className="
            flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
            bg-white/[0.06] light:bg-black/[0.05]
            text-white/60 light:text-black/60
            hover:bg-white/[0.10] light:hover:bg-black/[0.08]
            transition-all duration-200
          "
        >
          Check again
        </button>
        <button
          type="button"
          onClick={onReady}
          className="
            flex-1 h-9 rounded-[var(--radius-button)] text-xs font-medium
            bg-accent hover:bg-accent-hover text-white
            transition-all duration-200
          "
        >
          Continue anyway
        </button>
      </div>
    </motion.div>
  );
}
