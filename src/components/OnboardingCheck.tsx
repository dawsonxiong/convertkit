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
        <p className="text-sm text-white/40">Checking local tools…</p>
      </div>
    );
  }

  return (
    <motion.div
      initial={{ opacity: 0, y: 8 }}
      animate={{ opacity: 1, y: 0 }}
      className="workspace-card flex w-full max-w-md flex-col items-center gap-5 px-8 py-9"
    >
      <div className="flex size-12 items-center justify-center rounded-xl border border-blue-500/30 bg-blue-500/10">
        <svg
          className="size-6 text-blue-400"
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
        <p className="text-sm font-medium text-white/90">Missing required tools</p>
        <p className="max-w-xs text-xs text-white/40">
          ConvertKit needs a few CLI tools installed. Run this in Terminal:
        </p>
      </div>

      {/* Brew command */}
      <button
        type="button"
        onClick={handleCopy}
        className="
          w-full max-w-xs px-3 py-2 text-xs font-mono text-left rounded-[var(--radius-button)]
          bg-white/[0.04] text-white/60 border border-white/[0.06]
          hover:border-white/[0.14]
          transition-all duration-200
        "
        title="Click to copy"
      >
        $ {brewCommand}
        <span className="float-right text-white/20">copy</span>
      </button>

      {/* Dep status list */}
      <div className="w-full max-w-xs flex flex-col gap-1.5">
        {deps.map((dep) => (
          <div key={dep.name} className="flex items-center justify-between text-xs px-1">
            <span className="text-white/50">{dep.name}</span>
            <span
              className={
                dep.installed ? "text-success" : dep.required ? "text-error" : "text-white/25"
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
            bg-white/[0.06] text-white/60 hover:bg-white/[0.10]
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
            bg-blue-600 hover:bg-blue-500 text-white
            transition-all duration-200
          "
        >
          Continue anyway
        </button>
      </div>
    </motion.div>
  );
}
