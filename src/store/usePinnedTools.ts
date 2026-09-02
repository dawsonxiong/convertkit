import { create } from "zustand";
import {
  parsePinnedTools,
  PINNED_TOOLS_STORAGE_KEY,
  serializePinnedTools,
  togglePinnedTool,
} from "../lib/pinnedTools";
import type { SidebarOperation } from "../lib/toolNavigation";

function loadPinnedTools(): SidebarOperation[] {
  try {
    return parsePinnedTools(globalThis.localStorage?.getItem(PINNED_TOOLS_STORAGE_KEY));
  } catch {
    return [];
  }
}

function savePinnedTools(operations: SidebarOperation[]) {
  try {
    globalThis.localStorage?.setItem(PINNED_TOOLS_STORAGE_KEY, serializePinnedTools(operations));
  } catch {
    // Pins are optional and must not block file processing.
  }
}

interface PinnedToolsStore {
  operations: SidebarOperation[];
  toggle: (operation: SidebarOperation) => void;
}

const loaded = loadPinnedTools();

export const usePinnedTools = create<PinnedToolsStore>((set, get) => ({
  operations: loaded,
  toggle: (operation) => {
    const operations = togglePinnedTool(get().operations, operation);
    savePinnedTools(operations);
    set({ operations });
  },
}));
