import { TOOL_SECTIONS } from "./toolNavigation.ts";

export const COLLAPSED_SIDEBAR_SECTIONS_KEY = "convertkit.collapsedSidebarSections.v1";

export const SIDEBAR_SECTION_IDS = [
  "pinned",
  ...TOOL_SECTIONS.map((section) => section.id),
] as const;

export type SidebarSectionId = (typeof SIDEBAR_SECTION_IDS)[number];

const SIDEBAR_SECTION_ID_SET = new Set<string>(SIDEBAR_SECTION_IDS);

export function isSidebarSectionId(value: string): value is SidebarSectionId {
  return SIDEBAR_SECTION_ID_SET.has(value);
}

export function normalizeCollapsedSidebarSections(value: unknown): SidebarSectionId[] {
  if (!Array.isArray(value)) return [];

  const seen = new Set<SidebarSectionId>();
  const sections: SidebarSectionId[] = [];

  for (const item of value) {
    if (typeof item !== "string" || !isSidebarSectionId(item) || seen.has(item)) continue;
    seen.add(item);
    sections.push(item);
  }

  return sections;
}

export function serializeCollapsedSidebarSections(sections: readonly string[]): string {
  return JSON.stringify({
    version: 1,
    sections: normalizeCollapsedSidebarSections(sections),
  });
}

export function parseCollapsedSidebarSections(raw: string | null | undefined): SidebarSectionId[] {
  if (!raw) return [];

  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) return normalizeCollapsedSidebarSections(parsed);
    if (!parsed || typeof parsed !== "object") return [];

    const payload = parsed as { version?: unknown; sections?: unknown };
    if (payload.version !== 1) return [];
    return normalizeCollapsedSidebarSections(payload.sections);
  } catch {
    return [];
  }
}

export function toggleCollapsedSidebarSection(
  collapsed: readonly string[],
  sectionId: SidebarSectionId,
): SidebarSectionId[] {
  const current = normalizeCollapsedSidebarSections(collapsed);
  if (!isSidebarSectionId(sectionId)) return current;
  return current.includes(sectionId)
    ? current.filter((item) => item !== sectionId)
    : [...current, sectionId];
}
