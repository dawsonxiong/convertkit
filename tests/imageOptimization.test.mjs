import assert from "node:assert/strict";
import test from "node:test";
import {
  DEFAULT_IMAGE_TARGET_SIZE_BYTES,
  IMAGE_BYTES_PER_KIBIBYTE,
  IMAGE_TARGET_SIZE_MAX_KB,
  IMAGE_TARGET_SIZE_MIN_KB,
  imageOptimizationSettingsAreReady,
  imageTargetSizeBytesFromKb,
  imageTargetSizeForRequest,
  imageTargetSizeKbFromBytes,
  isImageTargetSizeBytes,
  supportsImageTargetSize,
} from "../src/lib/imageOptimization.ts";

function image(format, size) {
  return {
    path: `/tmp/source.${format}`,
    name: `source.${format}`,
    extension: format,
    format,
    category: "image",
    size,
    width: 100,
    height: 100,
    mimeType: null,
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

test("round-trips bounded whole-kibibyte image targets", () => {
  assert.equal(imageTargetSizeBytesFromKb(500), DEFAULT_IMAGE_TARGET_SIZE_BYTES);
  assert.equal(imageTargetSizeKbFromBytes(DEFAULT_IMAGE_TARGET_SIZE_BYTES), 500);
  assert.equal(isImageTargetSizeBytes(IMAGE_TARGET_SIZE_MIN_KB * 1024), true);
  assert.equal(isImageTargetSizeBytes(IMAGE_TARGET_SIZE_MAX_KB * 1024), true);
  assert.equal(isImageTargetSizeBytes(IMAGE_TARGET_SIZE_MIN_KB * 1024 - 1), false);
  assert.equal(isImageTargetSizeBytes(500 * 1024 + 1), false);
  assert.equal(imageTargetSizeBytesFromKb(1.5), null);
});

test("limits file-size optimization to supported lossy image formats", () => {
  for (const format of ["jpg", "jpeg", "webp", "avif", "heic", "heif"]) {
    assert.equal(supportsImageTargetSize(format), true);
  }
  for (const format of ["png", "gif", "tiff", "bmp", "ico"]) {
    assert.equal(supportsImageTargetSize(format), false);
  }
});

test("requires one valid target below every queued source", () => {
  const target = 500 * IMAGE_BYTES_PER_KIBIBYTE;
  const files = [image("jpeg", target + 1), image("webp", target * 2)];
  assert.equal(imageOptimizationSettingsAreReady("quality", null, files), true);
  assert.equal(imageOptimizationSettingsAreReady("fileSize", target, files), true);
  assert.equal(imageOptimizationSettingsAreReady("fileSize", target + 1, files), false);
  assert.equal(
    imageOptimizationSettingsAreReady("fileSize", target, [...files, image("png", target * 2)]),
    false,
  );
  assert.equal(imageTargetSizeForRequest("quality", target), null);
  assert.equal(imageTargetSizeForRequest("fileSize", target), target);
});
