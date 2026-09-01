import type { Operation } from "../types";

export interface BuiltInFinderAction {
  id: string;
  label: string;
  operation: Operation;
}

export const BUILT_IN_FINDER_ACTIONS: readonly BuiltInFinderAction[] = [
  { id: "builtin-convert", label: "Convert files", operation: "convert" },
  { id: "builtin-optimize", label: "Optimize images", operation: "optimize" },
  { id: "builtin-remove-metadata", label: "Remove metadata", operation: "removeMetadata" },
  { id: "builtin-inspect", label: "Inspect files", operation: "inspect" },
];

export function operationForBuiltInFinderAction(actionId: string): Operation | null {
  return BUILT_IN_FINDER_ACTIONS.find((action) => action.id === actionId)?.operation ?? null;
}
