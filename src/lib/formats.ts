import type { FileCategory } from "../types";
import formatMatrix from "./formatMatrix.json" with { type: "json" };

interface FormatMeta {
  extension: string;
  label: string;
  category: FileCategory;
}

export const FORMAT_INFO: Record<string, FormatMeta> = {
  // ── Image ───────────────────────────────────────────────
  jpg: { extension: "jpg", label: "JPEG", category: "image" },
  png: { extension: "png", label: "PNG", category: "image" },
  webp: { extension: "webp", label: "WebP", category: "image" },
  tiff: { extension: "tiff", label: "TIFF", category: "image" },
  bmp: { extension: "bmp", label: "BMP", category: "image" },
  gif: { extension: "gif", label: "GIF", category: "image" },
  ico: { extension: "ico", label: "ICO", category: "image" },
  avif: { extension: "avif", label: "AVIF", category: "image" },
  heic: { extension: "heic", label: "HEIC", category: "image" },

  // ── Video ───────────────────────────────────────────────
  mp4: { extension: "mp4", label: "MP4", category: "video" },
  mov: { extension: "mov", label: "MOV", category: "video" },
  webm: { extension: "webm", label: "WebM", category: "video" },
  mkv: { extension: "mkv", label: "MKV", category: "video" },
  avi: { extension: "avi", label: "AVI", category: "video" },

  // ── Audio ───────────────────────────────────────────────
  mp3: { extension: "mp3", label: "MP3", category: "audio" },
  wav: { extension: "wav", label: "WAV", category: "audio" },
  aac: { extension: "aac", label: "AAC", category: "audio" },
  flac: { extension: "flac", label: "FLAC", category: "audio" },
  ogg: { extension: "ogg", label: "OGG", category: "audio" },
  m4a: { extension: "m4a", label: "M4A", category: "audio" },

  // ── Document ────────────────────────────────────────────
  pdf: { extension: "pdf", label: "PDF", category: "document" },
  docx: { extension: "docx", label: "DOCX", category: "document" },
  html: { extension: "html", label: "HTML", category: "document" },
  md: { extension: "md", label: "Markdown", category: "document" },
  epub: { extension: "epub", label: "EPUB", category: "document" },
  txt: { extension: "txt", label: "TXT", category: "document" },

  // ── Vector ──────────────────────────────────────────────
  svg: { extension: "svg", label: "SVG", category: "vector" },

  // ── Archive ─────────────────────────────────────────────
  zip: { extension: "zip", label: "ZIP", category: "other" },
  tar: { extension: "tar", label: "TAR", category: "other" },
};

/* ── Conversion compatibility matrix ──────────────────────── */

// Rust tests also validate this file against every engine route. Keep one
// frontend source so the picker and input filtering cannot drift apart.
export const COMPATIBLE_TARGETS: Readonly<Record<string, readonly string[]>> = formatMatrix;

const EXTENSION_ALIASES: Record<string, string> = {
  jpeg: "jpg",
  tif: "tiff",
  heif: "heic",
  m4v: "mp4",
  oga: "ogg",
  opus: "ogg",
  htm: "html",
  markdown: "md",
  text: "txt",
  wave: "wav",
};

export const SUPPORTED_INPUT_EXTENSIONS_LIST = [
  ...Object.keys(FORMAT_INFO),
  ...Object.keys(EXTENSION_ALIASES),
];

/** Convert a supported alias into the canonical format key. */
export function normalizeExtension(extension: string): string {
  const normalized = extension.toLowerCase();
  return EXTENSION_ALIASES[normalized] ?? normalized;
}

/** Returns the list of compatible output format keys for a given input format. */
export function getCompatibleFormats(inputFormat: string): readonly string[] {
  return COMPATIBLE_TARGETS[inputFormat.toLowerCase()] ?? [];
}
