import type { Operation } from "../types";

const SUPPORTED_CLIPBOARD_IMAGE_MIMES = [
  "image/png",
  "image/jpeg",
  "image/webp",
  "image/gif",
  "image/bmp",
] as const;

export const PASTED_IMAGE_OPERATIONS = [
  "convert",
  "resize",
  "optimize",
  "exportImages",
  "removeMetadata",
  "recognizeText",
  "createArchive",
  "inspect",
] as const satisfies readonly Operation[];

const PASTED_IMAGE_OPERATION_SET = new Set<Operation>(PASTED_IMAGE_OPERATIONS);

export function operationAcceptsPastedImage(operation: Operation) {
  return PASTED_IMAGE_OPERATION_SET.has(operation);
}

interface ClipboardItemLike {
  kind: string;
  type: string;
}

type ClipboardImageSelection<T extends ClipboardItemLike> =
  | { kind: "none" }
  | { kind: "unsupported"; mime: string }
  | { kind: "supported"; item: T };

export function selectClipboardImage<T extends ClipboardItemLike>(
  items: ArrayLike<T>,
): ClipboardImageSelection<T> {
  const imageItems = Array.from(items).filter(
    (item) => item.kind === "file" && item.type.startsWith("image/"),
  );
  if (imageItems.length === 0) return { kind: "none" };

  const supported = imageItems.find((item) =>
    SUPPORTED_CLIPBOARD_IMAGE_MIMES.includes(
      item.type as (typeof SUPPORTED_CLIPBOARD_IMAGE_MIMES)[number],
    ),
  );
  if (supported) return { kind: "supported", item: supported };
  return { kind: "unsupported", mime: imageItems[0].type };
}

export function arrayBufferToBase64(buffer: ArrayBuffer) {
  const bytes = new Uint8Array(buffer);
  const chunkSize = 0x8000;
  let binary = "";

  for (let offset = 0; offset < bytes.length; offset += chunkSize) {
    binary += String.fromCharCode(...bytes.subarray(offset, offset + chunkSize));
  }

  return btoa(binary);
}
