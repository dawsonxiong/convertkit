import { OPERATIONS, type ActiveOperation } from "./operations.ts";

export type SidebarOperation = ActiveOperation;

interface ToolSection {
  id: string;
  label: string;
  operations: readonly SidebarOperation[];
}

export const TOOL_SECTIONS = [
  {
    id: "general",
    label: "General",
    operations: ["convert", "removeMetadata"],
  },
  {
    id: "images",
    label: "Images",
    operations: ["resize", "optimize", "exportImages"],
  },
  {
    id: "video-audio",
    label: "Video & audio",
    operations: [
      "encodeVideo",
      "compressAudio",
      "extractAudio",
      "transcribe",
      "extractSubtitles",
      "generateThumbnails",
      "removeAudio",
    ],
  },
  {
    id: "pdf-documents",
    label: "PDF & documents",
    operations: [
      "extractText",
      "recognizeText",
      "mergePdf",
      "splitPdf",
      "exportPdfPages",
      "compressPdf",
    ],
  },
  {
    id: "organize",
    label: "Organize",
    operations: ["createArchive", "extractArchive", "rename", "inspect"],
  },
] as const satisfies readonly ToolSection[];

export const SIDEBAR_OPERATIONS: readonly SidebarOperation[] = TOOL_SECTIONS.flatMap(
  (section) => section.operations,
);

export function filterToolSections(query: string) {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) return TOOL_SECTIONS;
  const queryTokens = needle.split(/\s+/);

  return TOOL_SECTIONS.map((section) => {
    const sectionMatches = section.label.toLocaleLowerCase().includes(needle);
    const operations = section.operations.filter((operation) => {
      if (sectionMatches) return true;

      const meta = OPERATIONS[operation];
      const searchableTokens = [
        meta.label,
        meta.pageTitle,
        meta.description,
        ...meta.categories,
        ...meta.extensions,
      ]
        .join(" ")
        .toLocaleLowerCase()
        .split(/[^a-z0-9]+/)
        .filter(Boolean);

      return queryTokens.every((token) =>
        searchableTokens.some((candidate) => candidate.startsWith(token)),
      );
    });

    return { ...section, operations };
  }).filter((section) => section.operations.length > 0);
}
