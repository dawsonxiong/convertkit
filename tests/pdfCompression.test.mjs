import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  DEFAULT_PDF_TARGET_SIZE_BYTES,
  isPdfTargetSizeBytes,
  PDF_BYTES_PER_MEBIBYTE,
  PDF_TARGET_SIZE_MAX_MB,
  PDF_TARGET_SIZE_MIN_MB,
  pdfCompressionSettingsAreReady,
  pdfTargetSizeBytesFromMb,
  pdfTargetSizeForRequest,
  pdfTargetSizeMbFromBytes,
} from "../src/lib/pdfCompression.ts";

test("converts bounded whole-megabyte PDF targets without decimal drift", () => {
  assert.equal(pdfTargetSizeBytesFromMb(PDF_TARGET_SIZE_MIN_MB), PDF_BYTES_PER_MEBIBYTE);
  assert.equal(
    pdfTargetSizeBytesFromMb(PDF_TARGET_SIZE_MAX_MB),
    PDF_TARGET_SIZE_MAX_MB * PDF_BYTES_PER_MEBIBYTE,
  );
  assert.equal(pdfTargetSizeBytesFromMb(0), null);
  assert.equal(pdfTargetSizeBytesFromMb(PDF_TARGET_SIZE_MAX_MB + 1), null);
  assert.equal(pdfTargetSizeBytesFromMb(2.5), null);
  assert.equal(pdfTargetSizeMbFromBytes(DEFAULT_PDF_TARGET_SIZE_BYTES), 10);
  assert.equal(isPdfTargetSizeBytes(DEFAULT_PDF_TARGET_SIZE_BYTES), true);
  assert.equal(isPdfTargetSizeBytes(PDF_BYTES_PER_MEBIBYTE + 1), false);
});

test("requires a valid target below every queued PDF", () => {
  const target = 2 * PDF_BYTES_PER_MEBIBYTE;
  const files = [{ size: 5 * PDF_BYTES_PER_MEBIBYTE }, { size: target + 1 }];
  assert.equal(pdfCompressionSettingsAreReady("quality", null, files), true);
  assert.equal(pdfCompressionSettingsAreReady("fileSize", target, files), true);
  assert.equal(pdfCompressionSettingsAreReady("fileSize", null, files), false);
  assert.equal(pdfCompressionSettingsAreReady("fileSize", target, [{ size: target }]), false);
  assert.equal(pdfTargetSizeForRequest("quality", target), null);
  assert.equal(pdfTargetSizeForRequest("fileSize", target), target);
});

test("compress-PDF controls expose quality and file-size goals through shared styling", async () => {
  const source = await readFile(
    new URL("../src/components/PdfCompressionPanel.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, />Compression goal</);
  assert.match(source, /label: "Quality"/);
  assert.match(source, /label: "File size"/);
  assert.match(source, />Quality level</);
  assert.match(source, /aria-label="Target PDF size in megabytes"/);
  assert.match(source, /className="choice-button"/);
});
