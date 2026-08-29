import { useCallback } from "react";
import { open } from "@tauri-apps/plugin-dialog";
import { getFileInfo } from "../lib/tauri";
import {
  getOperationDialogFilter,
  isPathSupportedForOperation,
  OPERATIONS,
} from "../lib/operations";
import { useAppStore } from "../store/useAppStore";

interface DropZoneProps {
  isDragging: boolean;
  disabled?: boolean;
}

export function DropZone({ isDragging, disabled = false }: DropZoneProps) {
  const operation = useAppStore((store) => store.operation);
  const addFiles = useAppStore((store) => store.addFiles);
  const setRejection = useAppStore((store) => store.setRejection);
  const meta = OPERATIONS[operation];

  const handleClick = useCallback(async () => {
    const selected = await open({
      multiple: true,
      title:
        operation === "convert"
          ? "Choose files to convert"
          : operation === "resize"
            ? "Choose images to resize"
            : "Choose images to optimize",
      filters: getOperationDialogFilter(operation),
    });

    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    const supported = paths.filter((path) => isPathSupportedForOperation(path, operation));
    if (supported.length === 0) {
      setRejection(
        operation !== "convert"
          ? `${operation === "resize" ? "Resize" : "Optimize"} works with raster images`
          : "Those file types are not supported",
      );
      return;
    }

    try {
      addFiles(await Promise.all(supported.map(getFileInfo)));
      if (supported.length < paths.length) {
        setRejection(`${paths.length - supported.length} unsupported file(s) skipped`);
      }
    } catch (error) {
      console.error("Failed to get file info:", error);
      setRejection("One or more files could not be opened");
    }
  }, [addFiles, operation, setRejection]);

  return (
    <button
      type="button"
      onClick={handleClick}
      disabled={disabled}
      className={`drop-grid group relative flex min-h-[360px] flex-col items-center justify-center overflow-hidden border border-dashed p-6 text-center transition-colors disabled:cursor-not-allowed disabled:opacity-60 ${
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
        {isDragging ? "Release to add files" : meta.dropLabel}
      </span>
      <span className="mt-2 max-w-64 text-sm leading-relaxed text-[#92939d]">
        {operation !== "convert"
          ? "PNG, JPEG, WebP, GIF, HEIC, TIFF, BMP, and AVIF."
          : "Images, video, audio, and documents."}
      </span>
      <span className="mt-5 flex h-8 items-center border border-[#44464f] bg-[#201f22] px-4 text-[11px] font-medium text-[#e5e1e4] group-hover:bg-[#2a2a2c]">
        Browse files
      </span>
    </button>
  );
}
