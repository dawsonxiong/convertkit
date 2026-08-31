import assert from "node:assert/strict";
import test from "node:test";

import {
  arrayBufferToBase64,
  operationAcceptsPastedImage,
  PASTED_IMAGE_OPERATIONS,
  selectClipboardImage,
} from "../src/lib/clipboard.ts";

test("ignores ordinary clipboard text instead of treating it as a failed image paste", () => {
  assert.deepEqual(selectClipboardImage([{ kind: "string", type: "text/plain" }]), {
    kind: "none",
  });
});

test("selects a supported image and reports genuinely unsupported image formats", () => {
  const png = { kind: "file", type: "image/png", id: "png" };
  assert.deepEqual(selectClipboardImage([{ kind: "file", type: "image/tiff", id: "tiff" }, png]), {
    kind: "supported",
    item: png,
  });
  assert.deepEqual(selectClipboardImage([{ kind: "file", type: "image/tiff" }]), {
    kind: "unsupported",
    mime: "image/tiff",
  });
});

test("encodes binary clipboard data without a byte-by-byte string reduction", () => {
  const bytes = Uint8Array.from({ length: 100_000 }, (_, index) => index % 251);
  const encoded = arrayBufferToBase64(bytes.buffer);
  assert.deepEqual(Buffer.from(encoded, "base64"), Buffer.from(bytes));
});

test("keeps image-paste eligibility explicit and operation typed", () => {
  assert.equal(new Set(PASTED_IMAGE_OPERATIONS).size, PASTED_IMAGE_OPERATIONS.length);
  for (const operation of ["convert", "resize", "recognizeText", "createArchive", "inspect"]) {
    assert.equal(operationAcceptsPastedImage(operation), true, operation);
  }
  for (const operation of ["encodeVideo", "mergePdf", "rename", "transcribe"]) {
    assert.equal(operationAcceptsPastedImage(operation), false, operation);
  }
});
