import { OPERATIONS, type ActiveOperation } from "./operations.ts";

export type SidebarOperation = ActiveOperation;

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
] as const;

export type ToolSectionId = (typeof TOOL_SECTIONS)[number]["id"];

export interface ToolSection {
  id: ToolSectionId;
  label: string;
  operations: readonly SidebarOperation[];
}

export const SIDEBAR_OPERATIONS: readonly SidebarOperation[] = TOOL_SECTIONS.flatMap(
  (section) => section.operations,
);

export function filterToolSections(query: string): ToolSection[] {
  const needle = query.trim().toLocaleLowerCase();
  if (!needle) {
    return TOOL_SECTIONS.map((section) => ({
      id: section.id,
      label: section.label,
      operations: [...section.operations],
    }));
  }
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

export function pinnedOperationsForQuery(pinned: readonly SidebarOperation[], query: string) {
  const allowed = new Set(filterToolSections(query).flatMap((section) => section.operations));
  return pinned.filter((operation) => allowed.has(operation));
}

export function excludePinnedOperations(
  sections: readonly ToolSection[],
  pinned: readonly SidebarOperation[],
): ToolSection[] {
  const pinnedSet = new Set(pinned);
  return sections
    .map((section) => ({
      ...section,
      operations: section.operations.filter((operation) => !pinnedSet.has(operation)),
    }))
    .filter((section) => section.operations.length > 0);
}
