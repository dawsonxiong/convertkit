import assert from "node:assert/strict";
import test from "node:test";

import {
  COMPATIBLE_TARGETS,
  FORMAT_INFO,
  getCompatibleFormats,
  normalizeExtension,
} from "../src/lib/formats.ts";

test("defines one compatibility row for every known format", () => {
  assert.deepEqual(Object.keys(COMPATIBLE_TARGETS).sort(), Object.keys(FORMAT_INFO).sort());
});

test("conversion targets are known, unique, and never the input format", () => {
  for (const [input, targets] of Object.entries(COMPATIBLE_TARGETS)) {
    assert.equal(new Set(targets).size, targets.length, `${input} contains duplicate targets`);
    assert.equal(targets.includes(input), false, `${input} advertises a no-op conversion`);
    for (const target of targets) {
      assert.ok(FORMAT_INFO[target], `${input} advertises unknown target ${target}`);
    }
  }
});

test("keeps unreliable PDF input conversion out of the picker", () => {
  assert.deepEqual(getCompatibleFormats("pdf"), []);
});

test("offers one-page PDF output for PNG and JPEG without changing defaults", () => {
  for (const format of ["jpg", "png"]) {
    assert.equal(getCompatibleFormats(format).at(-1), "pdf", `${format} keeps PDF last`);
  }
  for (const format of ["webp", "tiff", "bmp", "gif", "ico", "avif", "heic"]) {
    assert.equal(getCompatibleFormats(format).includes("pdf"), false);
  }
  assert.equal(getCompatibleFormats("svg").includes("pdf"), false);
  assert.equal(getCompatibleFormats("png")[0], "jpg");
  assert.equal(getCompatibleFormats("jpg")[0], "png");
});

test("normalizes common aliases before matrix lookup", () => {
  assert.equal(normalizeExtension("JPEG"), "jpg");
  assert.equal(normalizeExtension("HEIF"), "heic");
  assert.ok(getCompatibleFormats(normalizeExtension("markdown")).includes("docx"));
});
