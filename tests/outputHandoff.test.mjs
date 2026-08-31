import assert from "node:assert/strict";
import test from "node:test";
import {
  outputHandoffOperations,
  outputPathsForResult,
  outputPathsForResults,
  uniqueOutputPaths,
} from "../src/lib/outputHandoff.ts";
import { RETIRED_OPERATIONS } from "../src/lib/operations.ts";
import { SIDEBAR_OPERATIONS } from "../src/lib/toolNavigation.ts";

test("output paths stay ordered and deduplicate group and multi-output results", () => {
  assert.deepEqual(uniqueOutputPaths(["/out/a.png", "", "/out/a.png", null, "/out/b.png"]), [
    "/out/a.png",
    "/out/b.png",
  ]);
  assert.deepEqual(
    outputPathsForResult({
      output_path: "/out/a.png",
      output_paths: ["/out/a.png", "/out/b.png", "/out/a.png"],
      output_size: 20,
      duration_ms: 10,
    }),
    ["/out/a.png", "/out/b.png"],
  );
});

test("aggregate output paths preserve result order and deduplicate every batch output", () => {
  assert.deepEqual(
    outputPathsForResults([
      {
        output_path: "/out/first.png",
        output_paths: ["/out/first.png", "/out/first-preview.png"],
        output_size: 20,
        duration_ms: 10,
      },
      {
        output_path: "/out/second.png",
        output_paths: ["/out/second.png", "/out/first-preview.png", "/out/second-thumb.png"],
        output_size: 24,
        duration_ms: 12,
      },
    ]),
    ["/out/first.png", "/out/first-preview.png", "/out/second.png", "/out/second-thumb.png"],
  );
  assert.deepEqual(outputPathsForResults([]), []);
});

test("handoff destinations are visible, ordered, compatible with every output, and not current", () => {
  const destinations = outputHandoffOperations(["/out/photo.png"], "convert");

  assert.deepEqual(destinations, [
    "removeMetadata",
    "resize",
    "optimize",
    "exportImages",
    "recognizeText",
    "mergePdf",
    "createArchive",
    "rename",
    "inspect",
  ]);
  assert.deepEqual(
    destinations,
    SIDEBAR_OPERATIONS.filter((operation) => destinations.includes(operation)),
  );
  assert.equal(destinations.includes("convert"), false);
  for (const retired of RETIRED_OPERATIONS) assert.equal(destinations.includes(retired), false);
});

test("mixed batches use a compatibility intersection instead of dropping outputs", () => {
  assert.deepEqual(outputHandoffOperations(["/out/photo.png", "/out/video.mp4"], "convert"), [
    "removeMetadata",
    "createArchive",
    "rename",
    "inspect",
  ]);
});

test("PDF and archive outputs expose only compatible utility destinations", () => {
  assert.deepEqual(outputHandoffOperations(["/out/report.pdf"], "compressPdf"), [
    "removeMetadata",
    "extractText",
    "recognizeText",
    "mergePdf",
    "splitPdf",
    "exportPdfPages",
    "createArchive",
    "rename",
    "inspect",
  ]);
  assert.deepEqual(outputHandoffOperations(["/out/files.zip"], "createArchive"), [
    "extractArchive",
    "rename",
    "inspect",
  ]);
  assert.deepEqual(outputHandoffOperations(["/out/backup.7z"], "inspect"), [
    "createArchive",
    "extractArchive",
    "rename",
  ]);
  assert.deepEqual(outputHandoffOperations(["/out/data.csv.gz"], "inspect"), [
    "createArchive",
    "extractArchive",
    "rename",
  ]);
  assert.deepEqual(outputHandoffOperations([], "convert"), []);
});

test("converted image PDFs can continue through every compatible PDF utility", () => {
  assert.deepEqual(outputHandoffOperations(["/out/photo.pdf"], "convert"), [
    "removeMetadata",
    "extractText",
    "recognizeText",
    "mergePdf",
    "splitPdf",
    "exportPdfPages",
    "compressPdf",
    "createArchive",
    "rename",
    "inspect",
  ]);
});
