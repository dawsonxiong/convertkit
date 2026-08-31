import type { FileCategory, Operation } from "../types";
import {
  FORMAT_INFO,
  getCompatibleFormats,
  normalizeExtension,
  SUPPORTED_INPUT_EXTENSIONS_LIST,
} from "./formats.ts";
import { isSupportedArchivePath } from "./archive.ts";

interface OperationMeta {
  label: string;
  pageTitle: string;
  description: string;
  dropLabel: string;
  dropDescription: string;
  actionLabel: string;
  fileDialogTitle: string;
  folderDialogTitle: string;
  categories: FileCategory[];
  extensions: string[];
}

export const JOB_OPERATIONS = [
  "convert",
  "resize",
  "optimize",
  "exportImages",
  "encodeVideo",
  "compressAudio",
  "removeAudio",
  "extractSubtitles",
  "generateThumbnails",
  "removeMetadata",
  "extractAudio",
  "transcribe",
  "extractText",
  "recognizeText",
  "mergePdf",
  "splitPdf",
  "exportPdfPages",
  "compressPdf",
  "createArchive",
  "extractArchive",
  "rename",
] as const satisfies readonly Operation[];

export const NON_JOB_OPERATIONS = ["inspect"] as const satisfies readonly Operation[];

const CONVERT_EXTENSIONS = SUPPORTED_INPUT_EXTENSIONS_LIST.filter(
  (extension) => getCompatibleFormats(normalizeExtension(extension)).length > 0,
);
const RESIZE_EXTENSIONS = CONVERT_EXTENSIONS.filter(
  (extension) => FORMAT_INFO[normalizeExtension(extension)]?.category === "image",
);
const OPTIMIZE_EXTENSIONS = RESIZE_EXTENSIONS;
const IMAGE_EXPORT_EXTENSIONS = RESIZE_EXTENSIONS;
const VIDEO_EXTENSIONS = ["mp4", "m4v", "mov", "webm", "mkv", "avi"];
const AUDIO_EXTENSIONS = ["mp3", "wav", "wave", "aac", "flac", "ogg", "oga", "m4a"];
const TRANSCRIPTION_EXTENSIONS = [...VIDEO_EXTENSIONS, ...AUDIO_EXTENSIONS];
const METADATA_EXTENSIONS = [
  "jpg",
  "jpeg",
  "png",
  "webp",
  "pdf",
  ...VIDEO_EXTENSIONS,
  ...AUDIO_EXTENSIONS,
];
const TEXT_EXTRACTION_EXTENSIONS = ["pdf", "docx", "html", "htm", "md", "markdown", "epub"];
const OCR_EXTENSIONS = ["pdf", "jpg", "jpeg", "png", "gif", "tif", "tiff", "bmp", "heic", "heif"];
const PDF_EXTENSIONS = ["pdf"];
const COMBINE_PDF_EXTENSIONS = ["pdf", "png", "jpg", "jpeg"];
const ARCHIVE_EXTENSIONS = ["zip", "7z", "tar", "tgz", "gz"];

export const OPERATIONS: Record<Operation, OperationMeta> = {
  convert: {
    label: "Convert",
    pageTitle: "Universal file converter",
    description: "Convert images, video, audio, and documents.",
    dropLabel: "Drop files here",
    dropDescription: "Images, video, audio, and documents.",
    actionLabel: "Convert",
    fileDialogTitle: "Choose files to convert",
    folderDialogTitle: "Choose a folder to convert",
    categories: ["image", "video", "audio", "document", "vector"],
    extensions: CONVERT_EXTENSIONS,
  },
  resize: {
    label: "Resize image",
    pageTitle: "Resize images",
    description: "Set exact dimensions or scale an image by percentage.",
    dropLabel: "Drop images here",
    dropDescription: "PNG, JPEG, WebP, GIF, HEIC, TIFF, BMP, AVIF, and ICO.",
    actionLabel: "Resize",
    fileDialogTitle: "Choose images to resize",
    folderDialogTitle: "Choose a folder of images to resize",
    categories: ["image"],
    extensions: RESIZE_EXTENSIONS,
  },
  optimize: {
    label: "Optimize image",
    pageTitle: "Optimize images",
    description: "Reduce file size without changing dimensions or format.",
    dropLabel: "Drop images here",
    dropDescription: "PNG, JPEG, WebP, GIF, HEIC, TIFF, BMP, AVIF, and ICO.",
    actionLabel: "Optimize",
    fileDialogTitle: "Choose images to optimize",
    folderDialogTitle: "Choose a folder of images to optimize",
    categories: ["image"],
    extensions: OPTIMIZE_EXTENSIONS,
  },
  exportImages: {
    label: "Export images",
    pageTitle: "Export image variants",
    description: "Create delivery-ready sizes and formats from static images.",
    dropLabel: "Drop images here",
    dropDescription: "Static PNG, JPEG, WebP, GIF, HEIC, TIFF, BMP, AVIF, and ICO.",
    actionLabel: "Export",
    fileDialogTitle: "Choose images to export",
    folderDialogTitle: "Choose a folder of images to export",
    categories: ["image"],
    extensions: IMAGE_EXPORT_EXTENSIONS,
  },
  encodeVideo: {
    label: "Compress video",
    pageTitle: "Compress videos",
    description: "Reduce or re-encode videos with practical presets.",
    dropLabel: "Drop videos here",
    dropDescription: "MP4, MOV, WebM, MKV, AVI, and M4V.",
    actionLabel: "Compress",
    fileDialogTitle: "Choose videos to compress",
    folderDialogTitle: "Choose a folder of videos",
    categories: ["video"],
    extensions: VIDEO_EXTENSIONS,
  },
  compressAudio: {
    label: "Compress audio",
    pageTitle: "Compress audio",
    description: "Reduce audio file size with practical M4A presets.",
    dropLabel: "Drop audio files here",
    dropDescription: "MP3, WAV, AAC, FLAC, OGG, and M4A.",
    actionLabel: "Compress",
    fileDialogTitle: "Choose audio files to compress",
    folderDialogTitle: "Choose a folder of audio files",
    categories: ["audio"],
    extensions: AUDIO_EXTENSIONS,
  },
  removeAudio: {
    label: "Remove audio",
    pageTitle: "Remove audio",
    description: "Create silent video copies without re-encoding the picture.",
    dropLabel: "Drop videos here",
    dropDescription: "MP4, MOV, WebM, MKV, AVI, and M4V.",
    actionLabel: "Remove audio",
    fileDialogTitle: "Choose videos",
    folderDialogTitle: "Choose a folder of videos",
    categories: ["video"],
    extensions: VIDEO_EXTENSIONS,
  },
  extractSubtitles: {
    label: "Extract subtitles",
    pageTitle: "Extract subtitles",
    description: "Export embedded text subtitle tracks from video files.",
    dropLabel: "Drop videos here",
    dropDescription: "MP4, MOV, WebM, MKV, AVI, and M4V.",
    actionLabel: "Extract subtitles",
    fileDialogTitle: "Choose videos",
    folderDialogTitle: "Choose a folder of videos",
    categories: ["video"],
    extensions: VIDEO_EXTENSIONS,
  },
  generateThumbnails: {
    label: "Video thumbnails",
    pageTitle: "Video thumbnails",
    description: "Create one midpoint still or contact sheet from each video.",
    dropLabel: "Drop videos here",
    dropDescription: "MP4, MOV, WebM, MKV, AVI, and M4V.",
    actionLabel: "Generate",
    fileDialogTitle: "Choose videos",
    folderDialogTitle: "Choose a folder of videos",
    categories: ["video"],
    extensions: VIDEO_EXTENSIONS,
  },
  removeMetadata: {
    label: "Remove metadata",
    pageTitle: "Remove metadata",
    description: "Remove private tags from images, PDFs, audio, and video without recompressing.",
    dropLabel: "Drop files here",
    dropDescription: "Images, PDFs, audio, and video files.",
    actionLabel: "Remove metadata",
    fileDialogTitle: "Choose files to clean",
    folderDialogTitle: "Choose a folder to clean",
    categories: ["image", "document", "video", "audio"],
    extensions: METADATA_EXTENSIONS,
  },
  extractAudio: {
    label: "Extract audio",
    pageTitle: "Extract audio",
    description: "Save the audio track from one or more video files.",
    dropLabel: "Drop videos here",
    dropDescription: "MP4, MOV, WebM, MKV, AVI, and M4V.",
    actionLabel: "Extract audio",
    fileDialogTitle: "Choose videos",
    folderDialogTitle: "Choose a folder of videos",
    categories: ["video"],
    extensions: VIDEO_EXTENSIONS,
  },
  transcribe: {
    label: "Transcribe",
    pageTitle: "Transcribe audio and video",
    description: "Create local transcripts with Whisper.",
    dropLabel: "Drop audio or video here",
    dropDescription: "Audio and video files.",
    actionLabel: "Transcribe",
    fileDialogTitle: "Choose audio or video files",
    folderDialogTitle: "Choose a folder of media files",
    categories: ["audio", "video"],
    extensions: TRANSCRIPTION_EXTENSIONS,
  },
  extractText: {
    label: "Extract text",
    pageTitle: "Extract text",
    description: "Save readable text from PDFs and documents.",
    dropLabel: "Drop documents here",
    dropDescription: "PDF, DOCX, HTML, Markdown, and EPUB.",
    actionLabel: "Extract text",
    fileDialogTitle: "Choose documents",
    folderDialogTitle: "Choose a folder of documents",
    categories: ["document"],
    extensions: TEXT_EXTRACTION_EXTENSIONS,
  },
  recognizeText: {
    label: "Recognize text",
    pageTitle: "Recognize text",
    description: "Create text files or searchable PDFs from scans.",
    dropLabel: "Drop images or PDFs here",
    dropDescription: "JPG, PNG, TIFF, HEIC, BMP, GIF, and PDF.",
    actionLabel: "Recognize text",
    fileDialogTitle: "Choose images or scanned PDFs",
    folderDialogTitle: "Choose a folder of images or scanned PDFs",
    categories: ["image", "document"],
    extensions: OCR_EXTENSIONS,
  },
  mergePdf: {
    label: "Combine to PDF",
    pageTitle: "Combine to PDF",
    description: "Combine PDFs and images in the order shown.",
    dropLabel: "Drop files here",
    dropDescription: "Add two or more PDF, PNG, or JPEG files.",
    actionLabel: "Combine",
    fileDialogTitle: "Choose files to combine",
    folderDialogTitle: "Choose a folder of PDFs and images",
    categories: ["document", "image"],
    extensions: COMBINE_PDF_EXTENSIONS,
  },
  splitPdf: {
    label: "Split PDF",
    pageTitle: "Split PDFs",
    description: "Extract selected pages or save every page separately.",
    dropLabel: "Drop PDFs here",
    dropDescription: "Add one or more PDF files.",
    actionLabel: "Split",
    fileDialogTitle: "Choose PDFs to split",
    folderDialogTitle: "Choose a folder of PDFs",
    categories: ["document"],
    extensions: PDF_EXTENSIONS,
  },
  exportPdfPages: {
    label: "Export PDF pages",
    pageTitle: "Export PDF pages",
    description: "Save PDF pages as normal image files.",
    dropLabel: "Drop PDFs here",
    dropDescription: "Export every page or choose specific pages.",
    actionLabel: "Export pages",
    fileDialogTitle: "Choose PDFs to export",
    folderDialogTitle: "Choose a folder of PDFs",
    categories: ["document"],
    extensions: PDF_EXTENSIONS,
  },
  compressPdf: {
    label: "Compress PDF",
    pageTitle: "Compress PDFs",
    description: "Reduce PDF file size in batches.",
    dropLabel: "Drop PDFs here",
    dropDescription: "Add one or more PDF files.",
    actionLabel: "Compress",
    fileDialogTitle: "Choose PDFs to compress",
    folderDialogTitle: "Choose a folder of PDFs",
    categories: ["document"],
    extensions: PDF_EXTENSIONS,
  },
  createArchive: {
    label: "Create archive",
    pageTitle: "Create archive",
    description:
      "Package files and folders as ZIP, 7Z, TAR, or TAR.GZ, or compress one file with GZIP.",
    dropLabel: "Drop files or folders here",
    dropDescription: "Use ZIP, 7Z, TAR, or TAR.GZ for folders; GZIP compresses one file.",
    actionLabel: "Create archive",
    fileDialogTitle: "Choose files to archive",
    folderDialogTitle: "Choose a folder to archive",
    categories: ["image", "video", "audio", "document", "vector", "other"],
    extensions: [],
  },
  extractArchive: {
    label: "Extract archive",
    pageTitle: "Extract archive",
    description: "Safely unpack ZIP, 7Z, TAR, TAR.GZ, TGZ, and GZIP files.",
    dropLabel: "Drop archives here",
    dropDescription: "Archives unpack into folders; standalone GZIP files decompress to one file.",
    actionLabel: "Extract",
    fileDialogTitle: "Choose archives to extract",
    folderDialogTitle: "Choose a folder of archives",
    categories: ["other"],
    extensions: ARCHIVE_EXTENSIONS,
  },
  rename: {
    label: "Batch rename",
    pageTitle: "Batch rename",
    description: "Preview every filename before anything changes.",
    dropLabel: "Drop files here",
    dropDescription: "Names change in place and can be undone.",
    actionLabel: "Rename",
    fileDialogTitle: "Choose files to rename",
    folderDialogTitle: "Choose a folder of files to rename",
    categories: ["image", "video", "audio", "document", "vector", "other"],
    extensions: [],
  },
  inspect: {
    label: "Inspect files",
    pageTitle: "Inspect files",
    description: "View file details and verify checksums.",
    dropLabel: "Drop files here",
    dropDescription: "Inspect any local file without changing it.",
    actionLabel: "Inspect",
    fileDialogTitle: "Choose files to inspect",
    folderDialogTitle: "Choose a folder to inspect",
    categories: ["image", "video", "audio", "document", "vector", "other"],
    extensions: [],
  },
};

export const OPERATION_IDS = Object.freeze(Object.keys(OPERATIONS) as Operation[]);

export const RETIRED_OPERATIONS = ["packageAppIcon", "duplicates", "normalizeAudio"] as const;

export type ActiveOperation = Operation;
type RetiredOperation = (typeof RETIRED_OPERATIONS)[number];

const RETIRED_OPERATION_SET = new Set<string>(RETIRED_OPERATIONS);
const ACTIVE_OPERATION_SET = new Set<Operation>(OPERATION_IDS);

export const ACTIVE_OPERATION_IDS = OPERATION_IDS;

export function isActiveOperation(value: unknown): value is ActiveOperation {
  return typeof value === "string" && ACTIVE_OPERATION_SET.has(value as Operation);
}

export function isRetiredOperation(value: unknown): value is RetiredOperation {
  return typeof value === "string" && RETIRED_OPERATION_SET.has(value);
}

export function isPathSupportedForOperation(path: string, operation: Operation): boolean {
  if (operation === "createArchive" || operation === "rename" || operation === "inspect")
    return true;
  if (operation === "extractArchive") return isSupportedArchivePath(path);
  const extension = normalizeExtension(path.split(".").pop() ?? "");
  return OPERATIONS[operation].extensions.includes(extension);
}

export function getOperationDialogFilter(operation: Operation) {
  if (operation === "createArchive" || operation === "rename" || operation === "inspect") return [];
  return [
    {
      name:
        operation === "convert"
          ? "Supported files"
          : operation === "removeMetadata"
            ? "Images, PDFs, audio, and video"
            : operation === "extractAudio" ||
                operation === "transcribe" ||
                operation === "encodeVideo" ||
                operation === "removeAudio" ||
                operation === "extractSubtitles"
              ? operation === "transcribe"
                ? "Audio and video files"
                : "Video files"
              : operation === "compressAudio"
                ? "Audio files"
                : operation === "extractText"
                  ? "Documents"
                  : operation === "recognizeText"
                    ? "Images and PDFs"
                    : operation === "mergePdf"
                      ? "PDF and image files"
                      : operation === "splitPdf" ||
                          operation === "exportPdfPages" ||
                          operation === "compressPdf"
                        ? "PDF files"
                        : operation === "extractArchive"
                          ? "Archive files"
                          : "Images",
      extensions: OPERATIONS[operation].extensions,
    },
  ];
}
