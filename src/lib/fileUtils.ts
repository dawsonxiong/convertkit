import { FORMAT_INFO } from "./formats";

/** Convert raw bytes to a human-readable string (B, KB, MB, GB). */
export function formatFileSize(bytes: number): string {
  if (bytes < 0) return "0 B";
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  if (bytes < 1024 * 1024 * 1024) return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
  return `${(bytes / (1024 * 1024 * 1024)).toFixed(2)} GB`;
}

/** Extract the lowercase extension from a filename (no leading dot). */
export function getExtension(filename: string): string {
  const dot = filename.lastIndexOf(".");
  if (dot === -1 || dot === filename.length - 1) return "";
  return filename.slice(dot + 1).toLowerCase();
}

/** Map a file extension to a known format key, or null if unrecognised. */
export function getFormatFromExtension(ext: string): string | null {
  const lower = ext.toLowerCase().replace(/^\./, "");

  // Direct match
  if (FORMAT_INFO[lower]) return lower;

  // Common aliases
  const aliases: Record<string, string> = {
    jpeg: "jpg",
    tif: "tiff",
    htm: "html",
    markdown: "md",
    wave: "wav",
    opus: "ogg",
    m4v: "mp4",
  };

  return aliases[lower] ?? null;
}
