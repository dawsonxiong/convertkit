import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  BYTES_PER_MEBIBYTE,
  DEFAULT_VIDEO_TARGET_SIZE_BYTES,
  effectiveVideoCompressionGoal,
  isVideoTargetSizeBytes,
  VIDEO_TARGET_SIZE_MAX_MB,
  VIDEO_TARGET_SIZE_MIN_MB,
  videoTargetSizeBytesFromMb,
  videoTargetSizeForRequest,
  videoTargetSizeMbFromBytes,
  videoCompressionSettingsAreReady,
} from "../src/lib/videoCompression.ts";

test("converts bounded whole-megabyte video targets without decimal drift", () => {
  assert.equal(videoTargetSizeBytesFromMb(VIDEO_TARGET_SIZE_MIN_MB), BYTES_PER_MEBIBYTE);
  assert.equal(
    videoTargetSizeBytesFromMb(VIDEO_TARGET_SIZE_MAX_MB),
    VIDEO_TARGET_SIZE_MAX_MB * BYTES_PER_MEBIBYTE,
  );
  assert.equal(videoTargetSizeBytesFromMb(0), null);
  assert.equal(videoTargetSizeBytesFromMb(VIDEO_TARGET_SIZE_MAX_MB + 1), null);
  assert.equal(videoTargetSizeBytesFromMb(2.5), null);
  assert.equal(videoTargetSizeMbFromBytes(DEFAULT_VIDEO_TARGET_SIZE_BYTES), 100);
  assert.equal(isVideoTargetSizeBytes(DEFAULT_VIDEO_TARGET_SIZE_BYTES), true);
  assert.equal(isVideoTargetSizeBytes(512), false);
});

test("sends a target only for supported file-size compression", () => {
  const target = 25 * BYTES_PER_MEBIBYTE;
  assert.equal(effectiveVideoCompressionGoal("smaller", "fileSize"), "fileSize");
  assert.equal(videoTargetSizeForRequest("smaller", "fileSize", target), target);
  assert.equal(videoTargetSizeForRequest("smaller", "quality", target), null);
  assert.equal(effectiveVideoCompressionGoal("archive", "fileSize"), "quality");
  assert.equal(videoTargetSizeForRequest("archive", "fileSize", target), null);
  assert.equal(videoCompressionSettingsAreReady("smaller", "fileSize", target), true);
  assert.equal(videoCompressionSettingsAreReady("smaller", "fileSize", null), false);
  assert.equal(videoCompressionSettingsAreReady("archive", "fileSize", null), true);
});

test("compress-video controls name resolution precisely and disable lossless size targets", async () => {
  const source = await readFile(
    new URL("../src/components/VideoPresetPanel.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, />\s*Maximum resolution\s*</);
  assert.doesNotMatch(source, />\s*Maximum size\s*</);
  assert.match(source, />Compression goal</);
  assert.match(source, /label: "Quality"/);
  assert.match(source, /label: "File size"/);
  assert.match(source, /preset === "archive" && option\.value === "fileSize"/);
  assert.match(source, /disabled=\{unavailable\}/);
  assert.match(source, /aria-label="Target file size in megabytes"/);
});
