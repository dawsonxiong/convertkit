import type { FileCategory, Operation } from "../types";
import {
  FORMAT_INFO,
  getCompatibleFormats,
  normalizeExtension,
  SUPPORTED_INPUT_EXTENSIONS_LIST,
} from "./formats";

export interface OperationMeta {
  id: Operation;
  label: string;
  dropLabel: string;
  actionLabel: string;
  categories: FileCategory[];
  extensions: string[];
}

const CONVERT_EXTENSIONS = SUPPORTED_INPUT_EXTENSIONS_LIST.filter(
  (extension) => getCompatibleFormats(normalizeExtension(extension)).length > 0,
);
const RESIZE_EXTENSIONS = CONVERT_EXTENSIONS.filter(
  (extension) => FORMAT_INFO[normalizeExtension(extension)]?.category === "image",
);
const OPTIMIZE_EXTENSIONS = RESIZE_EXTENSIONS;

export const OPERATIONS: Record<Operation, OperationMeta> = {
  convert: {
    id: "convert",
    label: "Convert",
    dropLabel: "Drop files here",
    actionLabel: "Convert",
    categories: ["image", "video", "audio", "document", "vector"],
    extensions: CONVERT_EXTENSIONS,
  },
  resize: {
    id: "resize",
    label: "Resize image",
    dropLabel: "Drop images here",
    actionLabel: "Resize",
    categories: ["image"],
    extensions: RESIZE_EXTENSIONS,
  },
  optimize: {
    id: "optimize",
    label: "Optimize image",
    dropLabel: "Drop images here",
    actionLabel: "Optimize",
    categories: ["image"],
    extensions: OPTIMIZE_EXTENSIONS,
  },
};

export function isPathSupportedForOperation(path: string, operation: Operation): boolean {
  const extension = normalizeExtension(path.split(".").pop() ?? "");
  return OPERATIONS[operation].extensions.includes(extension);
}

export function isCategorySupportedForOperation(
  category: FileCategory,
  operation: Operation,
): boolean {
  return OPERATIONS[operation].categories.includes(category);
}

export function getOperationDialogFilter(operation: Operation) {
  return [
    {
      name: operation === "convert" ? "Supported files" : "Images",
      extensions: OPERATIONS[operation].extensions,
    },
  ];
}
