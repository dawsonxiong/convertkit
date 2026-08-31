import assert from "node:assert/strict";
import test from "node:test";

import { normalizeConversionError } from "../src/lib/errors.ts";
import { CONVERSION_ERROR_MESSAGES } from "../src/lib/conversionErrorMessages.ts";

test("fills in details omitted from unit backend errors", () => {
  assert.deepEqual(normalizeConversionError({ kind: "Cancelled" }), {
    kind: "Cancelled",
    detail: {},
  });
  assert.deepEqual(normalizeConversionError({ kind: "ArchivePasswordRequired" }), {
    kind: "ArchivePasswordRequired",
    detail: {},
  });
  assert.deepEqual(normalizeConversionError({ kind: "IncorrectArchivePassword" }), {
    kind: "IncorrectArchivePassword",
    detail: {},
  });
});

test("keeps string details from structured backend errors", () => {
  assert.deepEqual(
    normalizeConversionError({
      kind: "ProcessFailed",
      detail: { message: "No measurable audio found", exit_code: 1 },
    }),
    { kind: "ProcessFailed", detail: { message: "No measurable audio found" } },
  );
});

test("wraps unstructured errors without trusting unknown kinds", () => {
  assert.deepEqual(normalizeConversionError(new Error("Bridge failed")), {
    kind: "ProcessFailed",
    detail: { message: "Bridge failed" },
  });
  assert.deepEqual(normalizeConversionError({ kind: "Unexpected" }), {
    kind: "ProcessFailed",
    detail: { message: "[object Object]" },
  });
});

test("retains numeric target-size details from typed backend errors", () => {
  assert.deepEqual(
    normalizeConversionError({
      kind: "TargetSizeUnreachable",
      detail: { targetBytes: 512000, smallestBytes: 700000 },
    }),
    {
      kind: "TargetSizeUnreachable",
      detail: { targetBytes: "512000", smallestBytes: "700000" },
    },
  );
  assert.deepEqual(
    normalizeConversionError({
      kind: "OutputNotSmaller",
      detail: { sourceBytes: 512000, outputBytes: 600000 },
    }),
    {
      kind: "OutputNotSmaller",
      detail: { sourceBytes: "512000", outputBytes: "600000" },
    },
  );
  assert.equal(
    CONVERSION_ERROR_MESSAGES.OutputNotSmaller,
    "The compressed file would not be smaller than the original.",
  );
});
