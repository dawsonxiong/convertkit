import assert from "node:assert/strict";
import test from "node:test";

import { operationForOpenedPaths } from "../src/lib/openWith.ts";

test("keeps Finder batches in the active tool when every path is supported", () => {
  assert.equal(operationForOpenedPaths(["/tmp/one.png", "/tmp/two.jpeg"], "resize"), "resize");
  assert.equal(
    operationForOpenedPaths(
      ["/tmp/cover.PNG", "/tmp/body.pdf", "/tmp/back-cover.jpeg"],
      "mergePdf",
    ),
    "mergePdf",
  );
  assert.equal(operationForOpenedPaths(["/tmp/anything.bin"], "inspect"), "inspect");
});

test("routes Finder batches to one sensible fallback without splitting them", () => {
  assert.equal(operationForOpenedPaths(["/tmp/photo.png", "/tmp/song.mp3"], "resize"), "convert");
  assert.equal(
    operationForOpenedPaths(["/tmp/first.zip", "/tmp/second.tar"], "convert"),
    "extractArchive",
  );
  assert.equal(operationForOpenedPaths(["/tmp/backup.7Z"], "convert"), "extractArchive");
  assert.equal(operationForOpenedPaths(["/tmp/data.csv.gz"], "convert"), "extractArchive");
  assert.equal(operationForOpenedPaths(["/tmp/document.pdf"], "convert"), "splitPdf");
  assert.equal(
    operationForOpenedPaths(["/tmp/photo.png", "/tmp/document.pdf"], "convert"),
    "mergePdf",
  );
  assert.equal(operationForOpenedPaths(["/tmp/archive.rar"], "convert"), "inspect");
});

test("an empty Finder payload does not change tools", () => {
  assert.equal(operationForOpenedPaths([], "transcribe"), "transcribe");
});
