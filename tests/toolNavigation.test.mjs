import assert from "node:assert/strict";
import test from "node:test";

import {
  ACTIVE_OPERATION_IDS,
  getOperationDialogFilter,
  isPathSupportedForOperation,
  JOB_OPERATIONS,
  NON_JOB_OPERATIONS,
  OPERATION_IDS,
  OPERATIONS,
  RETIRED_OPERATIONS,
} from "../src/lib/operations.ts";
import {
  filterToolSections,
  SIDEBAR_OPERATIONS,
  TOOL_SECTIONS,
} from "../src/lib/toolNavigation.ts";

test("groups every visible sidebar operation exactly once", () => {
  assert.equal(new Set(SIDEBAR_OPERATIONS).size, SIDEBAR_OPERATIONS.length);
  assert.equal(new Set(TOOL_SECTIONS.map((section) => section.id)).size, TOOL_SECTIONS.length);
  assert.equal(new Set(TOOL_SECTIONS.map((section) => section.label)).size, TOOL_SECTIONS.length);
});

test("shows every registered operation exactly once", () => {
  assert.deepEqual(new Set(SIDEBAR_OPERATIONS), new Set(Object.keys(OPERATIONS)));
  assert.deepEqual(new Set(SIDEBAR_OPERATIONS), new Set(ACTIVE_OPERATION_IDS));
});

test("keeps retired identifiers only for legacy-state migration", () => {
  assert.deepEqual(RETIRED_OPERATIONS, ["packageAppIcon", "duplicates", "normalizeAudio"]);

  for (const operation of RETIRED_OPERATIONS) {
    assert.equal(SIDEBAR_OPERATIONS.includes(operation), false, operation);
    assert.equal(ACTIVE_OPERATION_IDS.includes(operation), false, operation);
    assert.equal(OPERATION_IDS.includes(operation), false, operation);
    assert.equal(Object.hasOwn(OPERATIONS, operation), false, operation);
  }
});

test("classifies every registered operation as exactly one job or read-only workspace", () => {
  const jobs = new Set(JOB_OPERATIONS);
  const nonJobs = new Set(NON_JOB_OPERATIONS);

  for (const operation of jobs) assert.equal(nonJobs.has(operation), false);
  assert.deepEqual(new Set([...jobs, ...nonJobs]), new Set(OPERATION_IDS));
  assert.deepEqual(new Set(OPERATION_IDS), new Set(Object.keys(OPERATIONS)));
});

test("advertises ICO wherever the image utility picker accepts it", () => {
  for (const operationId of ["resize", "optimize", "exportImages"]) {
    const operation = OPERATIONS[operationId];
    assert.equal(operation.extensions.includes("ico"), true, operationId);
    assert.match(operation.dropDescription, /\bICO\b/, operationId);
    assert.equal(isPathSupportedForOperation("/tmp/app-icon.ICO", operationId), true, operationId);
  }
});

test("treats metadata removal as a general file utility with PDF support", () => {
  const metadata = OPERATIONS.removeMetadata;
  assert.deepEqual(metadata.categories, ["image", "document", "video", "audio"]);
  for (const extension of ["jpg", "png", "webp", "pdf", "mp4", "mov", "mp3", "flac"]) {
    assert.equal(metadata.extensions.includes(extension), true, extension);
  }
  assert.equal(isPathSupportedForOperation("/tmp/private.PDF", "removeMetadata"), true);
  assert.deepEqual(getOperationDialogFilter("removeMetadata"), [
    {
      name: "Images, PDFs, audio, and video",
      extensions: metadata.extensions,
    },
  ]);
  assert.equal(
    TOOL_SECTIONS.find((section) => section.id === "general")?.operations.includes(
      "removeMetadata",
    ),
    true,
  );
});

test("keeps PDF page export with the document utilities", () => {
  const operation = OPERATIONS.exportPdfPages;
  assert.deepEqual(operation.extensions, ["pdf"]);
  assert.equal(operation.actionLabel, "Export pages");
  assert.equal(
    TOOL_SECTIONS.find((section) => section.id === "pdf-documents")?.operations.includes(
      "exportPdfPages",
    ),
    true,
  );
});

test("keeps 7Z and standalone GZIP extraction in the existing archive utility", () => {
  const operation = OPERATIONS.extractArchive;
  assert.deepEqual(operation.extensions, ["zip", "7z", "tar", "tgz", "gz"]);
  assert.match(operation.description, /7Z/);
  assert.match(operation.description, /GZIP/);
  assert.match(operation.dropDescription, /standalone GZIP files decompress to one file/);
  assert.deepEqual(getOperationDialogFilter("extractArchive"), [
    { name: "Archive files", extensions: operation.extensions },
  ]);
});

test("keeps mixed PDF and image combining on the existing mergePdf operation", () => {
  const operation = OPERATIONS.mergePdf;
  assert.equal(operation.label, "Combine to PDF");
  assert.equal(operation.actionLabel, "Combine");
  assert.deepEqual(operation.categories, ["document", "image"]);
  assert.deepEqual(operation.extensions, ["pdf", "png", "jpg", "jpeg"]);
  for (const path of ["/tmp/report.pdf", "/tmp/page.png", "/tmp/cover.jpg", "/tmp/back.JPEG"]) {
    assert.equal(isPathSupportedForOperation(path, "mergePdf"), true, path);
  }
  assert.equal(isPathSupportedForOperation("/tmp/animation.webp", "mergePdf"), false);
  assert.deepEqual(getOperationDialogFilter("mergePdf"), [
    { name: "PDF and image files", extensions: ["pdf", "png", "jpg", "jpeg"] },
  ]);
  assert.equal(
    TOOL_SECTIONS.find((section) => section.id === "pdf-documents")?.operations.includes(
      "mergePdf",
    ),
    true,
  );
});

test("keeps on-device OCR with images and document utilities", () => {
  const operation = OPERATIONS.recognizeText;
  assert.deepEqual(operation.categories, ["image", "document"]);
  for (const extension of ["pdf", "png", "jpeg", "tiff", "heic"]) {
    assert.equal(operation.extensions.includes(extension), true, extension);
  }
  assert.equal(operation.extensions.includes("docx"), false);
  assert.equal(
    TOOL_SECTIONS.find((section) => section.id === "pdf-documents")?.operations.includes(
      "recognizeText",
    ),
    true,
  );
});

test("filters grouped tools by intent, format, and section without changing their order", () => {
  assert.deepEqual(
    filterToolSections("compress").flatMap((section) => section.operations),
    ["encodeVideo", "compressAudio", "compressPdf", "createArchive"],
  );
  assert.deepEqual(
    filterToolSections("epub").flatMap((section) => section.operations),
    ["convert", "extractText"],
  );
  assert.deepEqual(
    filterToolSections("organize").flatMap((section) => section.operations),
    ["createArchive", "extractArchive", "rename", "inspect"],
  );
  assert.equal(filterToolSections("not-a-real-tool").length, 0);
  assert.equal(filterToolSections("  "), TOOL_SECTIONS);
});
