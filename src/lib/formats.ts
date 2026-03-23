import type { FileCategory } from "../types";

export interface FormatMeta {
  extension: string;
  label: string;
  category: FileCategory;
  icon: string;
}

export const FORMAT_INFO: Record<string, FormatMeta> = {
  // ── Image ───────────────────────────────────────────────
  jpg: { extension: "jpg", label: "JPEG", category: "image", icon: "🖼" },
  png: { extension: "png", label: "PNG", category: "image", icon: "🖼" },
  webp: { extension: "webp", label: "WebP", category: "image", icon: "🖼" },
  tiff: { extension: "tiff", label: "TIFF", category: "image", icon: "🖼" },
  bmp: { extension: "bmp", label: "BMP", category: "image", icon: "🖼" },
  gif: { extension: "gif", label: "GIF", category: "image", icon: "🖼" },
  ico: { extension: "ico", label: "ICO", category: "image", icon: "🖼" },
  avif: { extension: "avif", label: "AVIF", category: "image", icon: "🖼" },
  heic: { extension: "heic", label: "HEIC", category: "image", icon: "🖼" },

  // ── Video ───────────────────────────────────────────────
  mp4: { extension: "mp4", label: "MP4", category: "video", icon: "🎬" },
  mov: { extension: "mov", label: "MOV", category: "video", icon: "🎬" },
  webm: { extension: "webm", label: "WebM", category: "video", icon: "🎬" },
  mkv: { extension: "mkv", label: "MKV", category: "video", icon: "🎬" },
  avi: { extension: "avi", label: "AVI", category: "video", icon: "🎬" },

  // ── Audio ───────────────────────────────────────────────
  mp3: { extension: "mp3", label: "MP3", category: "audio", icon: "🎵" },
  wav: { extension: "wav", label: "WAV", category: "audio", icon: "🎵" },
  aac: { extension: "aac", label: "AAC", category: "audio", icon: "🎵" },
  flac: { extension: "flac", label: "FLAC", category: "audio", icon: "🎵" },
  ogg: { extension: "ogg", label: "OGG", category: "audio", icon: "🎵" },
  m4a: { extension: "m4a", label: "M4A", category: "audio", icon: "🎵" },

  // ── Document ────────────────────────────────────────────
  pdf: { extension: "pdf", label: "PDF", category: "document", icon: "📄" },
  docx: { extension: "docx", label: "DOCX", category: "document", icon: "📄" },
  html: { extension: "html", label: "HTML", category: "document", icon: "📄" },
  md: { extension: "md", label: "Markdown", category: "document", icon: "📄" },
  epub: { extension: "epub", label: "EPUB", category: "document", icon: "📄" },
  txt: { extension: "txt", label: "TXT", category: "document", icon: "📄" },

  // ── Vector ──────────────────────────────────────────────
  svg: { extension: "svg", label: "SVG", category: "vector", icon: "✏️" },
};

/* ── Conversion compatibility matrix ──────────────────────── */

const IMAGE_FORMATS = ["jpg", "png", "webp", "tiff", "bmp", "gif", "ico", "avif", "heic"];
const VIDEO_FORMATS = ["mp4", "mov", "webm", "mkv", "avi"];
const AUDIO_FORMATS = ["mp3", "wav", "aac", "flac", "ogg", "m4a"];

const without = (arr: string[], item: string) => arr.filter((f) => f !== item);

export const COMPATIBLE_TARGETS: Record<string, string[]> = {
  // Image -> all other image formats + SVG (raster-to-vector)
  jpg: [...without(IMAGE_FORMATS, "jpg"), "svg"],
  png: [...without(IMAGE_FORMATS, "png"), "svg"],
  webp: [...without(IMAGE_FORMATS, "webp"), "svg"],
  tiff: [...without(IMAGE_FORMATS, "tiff"), "svg"],
  bmp: [...without(IMAGE_FORMATS, "bmp"), "svg"],
  gif: [...without(IMAGE_FORMATS, "gif"), "svg"],
  ico: [...without(IMAGE_FORMATS, "ico"), "svg"],
  avif: [...without(IMAGE_FORMATS, "avif"), "svg"],
  heic: [...without(IMAGE_FORMATS, "heic"), "svg"],

  // Video -> all other video formats + GIF + audio extraction
  mp4: [...without(VIDEO_FORMATS, "mp4"), "gif", ...AUDIO_FORMATS],
  mov: [...without(VIDEO_FORMATS, "mov"), "gif", ...AUDIO_FORMATS],
  webm: [...without(VIDEO_FORMATS, "webm"), "gif", ...AUDIO_FORMATS],
  mkv: [...without(VIDEO_FORMATS, "mkv"), "gif", ...AUDIO_FORMATS],
  avi: [...without(VIDEO_FORMATS, "avi"), "gif", ...AUDIO_FORMATS],

  // Audio -> all other audio formats
  mp3: without(AUDIO_FORMATS, "mp3"),
  wav: without(AUDIO_FORMATS, "wav"),
  aac: without(AUDIO_FORMATS, "aac"),
  flac: without(AUDIO_FORMATS, "flac"),
  ogg: without(AUDIO_FORMATS, "ogg"),
  m4a: without(AUDIO_FORMATS, "m4a"),

  // Document -> pandoc / libreoffice matrix
  pdf: ["docx"],
  docx: ["pdf", "html", "md", "txt", "epub"],
  html: ["pdf", "docx", "md", "txt", "epub"],
  md: ["pdf", "docx", "html", "txt", "epub"],
  epub: ["pdf", "docx", "html", "md", "txt"],
  txt: ["docx", "html", "md", "epub"],

  // SVG -> raster (resvg only supports SVG->PNG)
  svg: ["png"],
};

/** Set of all supported file extensions (lowercase, no dot). */
export const SUPPORTED_EXTENSIONS = new Set(Object.keys(FORMAT_INFO));

/** All supported extensions as a list (for file dialog filters). */
export const SUPPORTED_EXTENSIONS_LIST = Object.keys(FORMAT_INFO);

/** File dialog filter that only shows supported formats. */
export const FILE_DIALOG_FILTERS = [
  { name: "Supported files", extensions: SUPPORTED_EXTENSIONS_LIST },
];

/** Check whether a file path has a supported extension. */
export function isSupportedFile(path: string): boolean {
  const ext = path.split(".").pop()?.toLowerCase() ?? "";
  return SUPPORTED_EXTENSIONS.has(ext);
}

/** Returns the list of compatible output format keys for a given input format. */
export function getCompatibleFormats(inputFormat: string): string[] {
  return COMPATIBLE_TARGETS[inputFormat.toLowerCase()] ?? [];
}

interface CategoryGroup {
  category: string;
  formats: string[];
}

const CATEGORY_LABELS: Record<string, string> = {
  image: "Image",
  video: "Video",
  audio: "Audio",
  document: "Document",
  vector: "Vector",
};

const CATEGORY_ORDER: FileCategory[] = ["image", "video", "audio", "document", "vector"];

/** Groups an array of format keys by their category, in a stable order. */
export function getCategoryFormats(formats: string[]): CategoryGroup[] {
  const grouped: Record<string, string[]> = {};

  for (const fmt of formats) {
    const meta = FORMAT_INFO[fmt];
    if (!meta) continue;
    const cat = meta.category;
    if (!grouped[cat]) grouped[cat] = [];
    grouped[cat].push(fmt);
  }

  return CATEGORY_ORDER.filter((cat) => grouped[cat] && grouped[cat].length > 0).map((cat) => ({
    category: CATEGORY_LABELS[cat] ?? cat,
    formats: grouped[cat],
  }));
}
