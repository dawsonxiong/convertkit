import { create } from "zustand";
import {
  COLLAPSED_SIDEBAR_SECTIONS_KEY,
  parseCollapsedSidebarSections,
  serializeCollapsedSidebarSections,
  toggleCollapsedSidebarSection,
  type SidebarSectionId,
} from "../lib/collapsedSidebarSections";

function loadCollapsedSections(): SidebarSectionId[] {
  try {
    return parseCollapsedSidebarSections(
      globalThis.localStorage?.getItem(COLLAPSED_SIDEBAR_SECTIONS_KEY),
    );
  } catch {
    return [];
  }
}

function saveCollapsedSections(sections: SidebarSectionId[]) {
  try {
    globalThis.localStorage?.setItem(
      COLLAPSED_SIDEBAR_SECTIONS_KEY,
      serializeCollapsedSidebarSections(sections),
    );
  } catch {
    // Collapse state is optional and must not block file processing.
  }
}

interface CollapsedSidebarSectionsStore {
  sections: SidebarSectionId[];
  toggle: (sectionId: SidebarSectionId) => void;
}

const loaded = loadCollapsedSections();

export const useCollapsedSidebarSections = create<CollapsedSidebarSectionsStore>((set, get) => ({
  sections: loaded,
  toggle: (sectionId) => {
    const sections = toggleCollapsedSidebarSection(get().sections, sectionId);
    saveCollapsedSections(sections);
    set({ sections });
  },
}));
