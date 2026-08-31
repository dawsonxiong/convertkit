import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getOperationDialogFilter, OPERATIONS } from "../lib/operations";
import { useAppStore } from "../store/useAppStore";
import { useAddPaths } from "../hooks/useAddPaths";

interface DropZoneProps {
  isDragging: boolean;
  disabled?: boolean;
}

export function DropZone({ isDragging, disabled = false }: DropZoneProps) {
  const operation = useAppStore((store) => store.operation);
  const addPaths = useAddPaths();
  const meta = OPERATIONS[operation];

  const browseFiles = useCallback(async () => {
    if (disabled) return;
    const selected = await open({
      multiple: true,
      title: meta.fileDialogTitle,
      filters: getOperationDialogFilter(operation),
    });

    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    await addPaths(paths);
  }, [addPaths, disabled, meta.fileDialogTitle, operation]);

  const browseFolder = useCallback(async () => {
    if (disabled) return;
    const selected = await open({
      directory: true,
      multiple: false,
      title: meta.folderDialogTitle,
    });
    if (typeof selected === "string") await addPaths([selected]);
  }, [addPaths, disabled, meta.folderDialogTitle]);

  return (
    <div
      aria-disabled={disabled}
      className={`drop-grid relative flex min-h-[360px] flex-col items-center justify-center overflow-hidden border border-dashed p-6 text-center transition-colors ${
        disabled ? "pointer-events-none opacity-60" : ""
      } ${
        isDragging
          ? "border-[#b0c6ff] bg-[#b0c6ff]/[0.05]"
          : "border-[#44464f] hover:border-[#696b75]"
      }`}
    >
      <svg
        className="size-10 text-white"
        fill="none"
        viewBox="0 0 24 24"
        stroke="currentColor"
        strokeWidth={1.35}
        strokeLinecap="round"
        strokeLinejoin="round"
        aria-hidden="true"
      >
        <path d="M7.5 3.5h6.75L18.5 7.75V20.5h-11z" />
        <path d="M14 3.75V8h4.25M13 16V9.5m0 0-2.5 2.5m2.5-2.5 2.5 2.5" />
      </svg>

      <span className="mt-5 text-xl font-semibold text-[#e5e1e4]">
        {isDragging ? "Release to add files or folders" : meta.dropLabel}
      </span>
      <span className="mt-2 max-w-64 text-sm leading-relaxed text-[#92939d]">
        {meta.dropDescription}
      </span>
      <div className="mt-5 flex items-center gap-2">
        <button
          type="button"
          onClick={browseFiles}
          disabled={disabled}
          className="secondary-button"
        >
          Browse files
        </button>
        <button
          type="button"
          onClick={browseFolder}
          disabled={disabled}
          className="secondary-button"
        >
          Choose folder
        </button>
      </div>
    </div>
  );
}
