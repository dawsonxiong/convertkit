import { SIDEBAR_OPERATIONS, type SidebarOperation } from "./toolNavigation.ts";

export const MAX_PINNED_TOOLS = 12;
export const PINNED_TOOLS_STORAGE_KEY = "convertkit.pinnedTools.v1";

const SIDEBAR_OPERATION_SET = new Set<string>(SIDEBAR_OPERATIONS);

export function isSidebarOperation(value: string): value is SidebarOperation {
  return SIDEBAR_OPERATION_SET.has(value);
}

export function normalizePinnedTools(value: unknown): SidebarOperation[] {
  if (!Array.isArray(value)) return [];

  const seen = new Set<SidebarOperation>();
  const operations: SidebarOperation[] = [];

  for (const item of value) {
    if (typeof item !== "string" || !isSidebarOperation(item) || seen.has(item)) continue;
    seen.add(item);
    operations.push(item);
    if (operations.length >= MAX_PINNED_TOOLS) break;
  }

  return operations;
}

export function serializePinnedTools(operations: readonly SidebarOperation[]): string {
  return JSON.stringify({ version: 1, operations: normalizePinnedTools(operations) });
}

export function parsePinnedTools(raw: string | null | undefined): SidebarOperation[] {
  if (!raw) return [];

  try {
    const parsed: unknown = JSON.parse(raw);
    if (Array.isArray(parsed)) return normalizePinnedTools(parsed);
    if (!parsed || typeof parsed !== "object") return [];

    const payload = parsed as { version?: unknown; operations?: unknown };
    if (payload.version !== 1) return [];
    return normalizePinnedTools(payload.operations);
  } catch {
    return [];
  }
}

export function togglePinnedTool(
  pinned: readonly SidebarOperation[],
  operation: SidebarOperation,
): SidebarOperation[] {
  const current = normalizePinnedTools(pinned);
  if (!isSidebarOperation(operation)) return current;
  if (current.includes(operation)) return current.filter((item) => item !== operation);
  if (current.length >= MAX_PINNED_TOOLS) return current;
  return [...current, operation];
}
