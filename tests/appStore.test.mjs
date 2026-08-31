import assert from "node:assert/strict";
import test from "node:test";

import { RETIRED_OPERATIONS } from "../src/lib/operations.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

function video(path, name) {
  return {
    path,
    name,
    extension: "mkv",
    size: 1024,
    category: "video",
    format: "mkv",
    width: 1920,
    height: 1080,
    mimeType: "video/x-matroska",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function image(path, name) {
  return {
    path,
    name,
    extension: "png",
    size: 2048,
    category: "image",
    format: "png",
    width: 2400,
    height: 1200,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

const english = {
  streamIndex: 1,
  codec: "subrip",
  language: "eng",
  title: "English CC",
  isDefault: true,
  isForced: false,
  supported: true,
};

const french = {
  streamIndex: 2,
  codec: "subrip",
  language: "fra",
  title: "French",
  isDefault: false,
  isForced: false,
  supported: true,
};

const mainAudio = {
  streamIndex: 1,
  codec: "aac",
  language: "eng",
  title: "Main mix",
  channels: 2,
  channelLayout: "stereo",
  sampleRate: 48000,
  isDefault: true,
};

const commentaryAudio = {
  streamIndex: 3,
  codec: "ac3",
  language: "fra",
  title: "Commentary",
  channels: 6,
  channelLayout: "5.1(side)",
  sampleRate: 48000,
  isDefault: false,
};

test("retains subtitle queues and per-file settings across utility switches", () => {
  const first = video("/tmp/subtitle-a.mkv", "subtitle-a.mkv");
  const second = video("/tmp/subtitle-b.mkv", "subtitle-b.mkv");
  const store = useAppStore.getState();

  store.setOperation("extractSubtitles");
  useAppStore.getState().addFiles([first, second]);
  useAppStore.getState().setSubtitleTracks(first.path, [english, french]);
  useAppStore.getState().setSubtitleTracks(second.path, [french]);
  useAppStore.getState().setSubtitleTrackIndex(first.path, french.streamIndex);
  useAppStore.getState().setSubtitleOutputFormat("vtt");

  useAppStore.getState().setOperation("convert");
  assert.equal(useAppStore.getState().files.length, 0);

  useAppStore.getState().setOperation("extractSubtitles");
  const restored = useAppStore.getState();
  assert.deepEqual(
    restored.files.map((file) => file.path),
    [first.path, second.path],
  );
  assert.equal(restored.subtitleOutputFormat, "vtt");
  assert.equal(restored.subtitleTrackIndexes[first.path], french.streamIndex);
  assert.equal(restored.subtitleTrackIndexes[second.path], french.streamIndex);

  restored.removeFile(first.path);
  const afterRemoval = useAppStore.getState();
  assert.equal(afterRemoval.subtitleTracks[first.path], undefined);
  assert.equal(afterRemoval.subtitleTrackIndexes[first.path], undefined);
  assert.deepEqual(
    afterRemoval.files.map((file) => file.path),
    [second.path],
  );

  afterRemoval.reset();
  useAppStore.getState().setOperation("convert");
});

test("retains thumbnail layout and format across utility switches", () => {
  const file = video("/tmp/thumbnail-source.mkv", "thumbnail-source.mkv");

  useAppStore.getState().setOperation("generateThumbnails");
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setThumbnailMode("contactSheet");
  useAppStore.getState().setThumbnailOutputFormat("png");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("generateThumbnails");

  const restored = useAppStore.getState();
  assert.deepEqual(
    restored.files.map((item) => item.path),
    [file.path],
  );
  assert.equal(restored.thumbnailMode, "contactSheet");
  assert.equal(restored.thumbnailOutputFormat, "png");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("retains video delivery settings across utility switches", () => {
  const file = video("/tmp/video-delivery.mkv", "video-delivery.mkv");

  useAppStore.getState().setOperation("encodeVideo");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setVideoEncodingPreset("smaller");
  useAppStore.getState().setVideoResolution("hd");
  useAppStore.getState().setVideoQuality("smallest");
  useAppStore.getState().setVideoCompressionGoal("fileSize");
  useAppStore.getState().setVideoTargetSizeBytes(30 * 1024 * 1024);
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("encodeVideo");

  const restored = useAppStore.getState();
  assert.equal(restored.videoEncodingPreset, "smaller");
  assert.equal(restored.videoResolution, "hd");
  assert.equal(restored.videoQuality, "smallest");
  assert.equal(restored.videoCompressionGoal, "fileSize");
  assert.equal(restored.videoTargetSizeBytes, 30 * 1024 * 1024);
  assert.deepEqual(
    restored.files.map((item) => item.path),
    [file.path],
  );

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("retains image file-size optimization across utility switches", () => {
  const file = {
    ...image("/tmp/photo.jpeg", "photo.jpeg"),
    extension: "jpeg",
    format: "jpeg",
    size: 4 * 1024 * 1024,
    mimeType: "image/jpeg",
  };

  useAppStore.getState().setOperation("optimize");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setImageOptimizationGoal("fileSize");
  useAppStore.getState().setImageTargetSizeBytes(500 * 1024);
  useAppStore.getState().setKeepMetadata(true);
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("optimize");

  const restored = useAppStore.getState();
  assert.equal(restored.imageOptimizationGoal, "fileSize");
  assert.equal(restored.imageTargetSizeBytes, 500 * 1024);
  assert.equal(restored.keepMetadata, true);
  assert.deepEqual(restored.files.map((item) => item.path), [file.path]);

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("keeps archive passwords only in memory and scoped to their archive", () => {
  const archive = {
    ...image("/tmp/private.7z", "private.7z"),
    extension: "7z",
    format: "7z",
    category: "archive",
    mimeType: "application/x-7z-compressed",
  };

  useAppStore.getState().setOperation("extractArchive");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([archive]);
  useAppStore.getState().setExtractArchivePassword(archive.path, "correct horse");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("extractArchive");

  assert.equal(useAppStore.getState().extractArchivePasswords[archive.path], "correct horse");
  useAppStore.getState().removeFile(archive.path);
  assert.equal(useAppStore.getState().extractArchivePasswords[archive.path], undefined);

  useAppStore.getState().setOperation("createArchive");
  useAppStore.getState().reset();
  useAppStore.getState().setArchiveFormat("sevenZ");
  useAppStore.getState().setArchivePassword("ephemeral");
  useAppStore.getState().setArchivePasswordConfirmation("ephemeral");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("createArchive");
  assert.equal(useAppStore.getState().archivePassword, "ephemeral");
  assert.equal(useAppStore.getState().archivePasswordConfirmation, "ephemeral");

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("keeps lossless video on the quality path and rejects unsafe target sizes", () => {
  useAppStore.getState().setOperation("encodeVideo");
  useAppStore.getState().reset();
  useAppStore.getState().setVideoCompressionGoal("fileSize");
  useAppStore.getState().setVideoTargetSizeBytes(40 * 1024 * 1024);
  useAppStore.getState().setVideoEncodingPreset("archive");

  const lossless = useAppStore.getState();
  assert.equal(lossless.videoCompressionGoal, "quality");
  lossless.setVideoCompressionGoal("fileSize");
  assert.equal(useAppStore.getState().videoCompressionGoal, "quality");

  lossless.setVideoTargetSizeBytes(512);
  assert.equal(useAppStore.getState().videoTargetSizeBytes, 40 * 1024 * 1024);

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("retains per-file audio tracks and output format across utility switches", () => {
  const first = video("/tmp/audio-a.mkv", "audio-a.mkv");
  const second = video("/tmp/audio-b.mkv", "audio-b.mkv");

  useAppStore.getState().setOperation("extractAudio");
  useAppStore.getState().addFiles([first, second]);
  useAppStore.getState().setAudioTracks(first.path, [mainAudio, commentaryAudio]);
  useAppStore.getState().setAudioTracks(second.path, [mainAudio]);
  useAppStore.getState().setAudioTrackIndex(first.path, commentaryAudio.streamIndex);
  useAppStore.getState().setAudioOutputFormat("flac");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("extractAudio");

  const restored = useAppStore.getState();
  assert.deepEqual(
    restored.files.map((file) => file.path),
    [first.path, second.path],
  );
  assert.equal(restored.audioOutputFormat, "flac");
  assert.equal(restored.audioTrackIndexes[first.path], commentaryAudio.streamIndex);
  assert.equal(restored.audioTrackIndexes[second.path], mainAudio.streamIndex);

  restored.removeFile(first.path);
  const afterRemoval = useAppStore.getState();
  assert.equal(afterRemoval.audioTracks[first.path], undefined);
  assert.equal(afterRemoval.audioTrackIndexes[first.path], undefined);
  assert.deepEqual(
    afterRemoval.files.map((file) => file.path),
    [second.path],
  );

  afterRemoval.reset();
  useAppStore.getState().setOperation("convert");
});

test("retains selected image export variants across utility switches", () => {
  const first = image("/tmp/export-a.png", "export-a.png");
  const second = image("/tmp/export-b.png", "export-b.png");

  useAppStore.getState().setOperation("exportImages");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([first, second]);
  useAppStore.getState().setImageExportPresets(["preview", "web", "social"]);
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("exportImages");

  const restored = useAppStore.getState();
  assert.deepEqual(
    restored.files.map((file) => file.path),
    [first.path, second.path],
  );
  assert.deepEqual(restored.imageExportPresets, ["web", "social", "preview"]);

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("ignores attempts to activate retired operations", () => {
  const file = image("/tmp/active.png", "active.png");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);

  for (const retiredOperation of RETIRED_OPERATIONS) {
    useAppStore.getState().setOperation(retiredOperation);
    const state = useAppStore.getState();
    assert.equal(state.operation, "convert", retiredOperation);
    assert.deepEqual(
      state.files.map((item) => item.path),
      [file.path],
      retiredOperation,
    );
  }

  useAppStore.getState().reset();
});

test("retains transcription settings and queue across utility switches", () => {
  const file = video("/tmp/interview.mkv", "interview.mkv");

  useAppStore.getState().setOperation("transcribe");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setTranscriptionModel("small");
  useAppStore.getState().setTranscriptionLanguage("fr");
  useAppStore.getState().setTranscriptionOutputFormat("vtt");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("transcribe");

  const restored = useAppStore.getState();
  assert.deepEqual(
    restored.files.map((item) => item.path),
    [file.path],
  );
  assert.equal(restored.transcriptionModel, "small");
  assert.equal(restored.transcriptionLanguage, "fr");
  assert.equal(restored.transcriptionOutputFormat, "vtt");
  assert.equal(restored.queueItems[file.path].status, "pending");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("adds asynchronously collected files to the operation that requested them", () => {
  const inspected = {
    ...image("/tmp/inspect-only.bin", "inspect-only.bin"),
    extension: "bin",
    category: "other",
    format: "unknown",
    mimeType: null,
    width: null,
    height: null,
  };

  useAppStore.getState().setOperation("inspect");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().addFilesForOperation("inspect", [inspected]);

  assert.equal(useAppStore.getState().operation, "resize");
  assert.deepEqual(useAppStore.getState().files, []);

  useAppStore.getState().setOperation("inspect");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [inspected.path],
  );

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("pasted images append to the initiating utility after the user switches utilities", () => {
  const existing = image("/tmp/existing-convert.png", "existing-convert.png");
  const pasted = image("/tmp/pasted-convert.png", "pasted-convert.png");
  const resizeFile = image("/tmp/resize-only.png", "resize-only.png");

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([existing]);
  const targetOperation = useAppStore.getState().operation;

  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([resizeFile]);
  useAppStore.getState().addFilesForOperation(targetOperation, [pasted]);

  assert.equal(useAppStore.getState().operation, "resize");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [resizeFile.path],
  );

  useAppStore.getState().setOperation("convert");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [existing.path, pasted.path],
  );
  assert.equal(useAppStore.getState().queueItems[existing.path]?.status, "pending");
  assert.equal(useAppStore.getState().queueItems[pasted.path]?.status, "pending");

  useAppStore.getState().reset();
});

test("operation-scoped intake matches active intake and rejects duplicates", () => {
  const first = image("/tmp/scoped-a.png", "scoped-a.png");
  const duplicate = { ...first, name: "duplicate-name.png" };
  const second = image("/tmp/scoped-b.png", "scoped-b.png");

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFilesForOperation("convert", [first, duplicate, second]);

  const state = useAppStore.getState();
  assert.deepEqual(
    state.files.map((file) => file.path),
    [first.path, second.path],
  );
  assert.equal(Object.keys(state.queueItems).length, 2);
  assert.equal(state.file?.path, first.path);
  state.reset();
});

test("the store rejects active-session intake races without blocking idle sessions", () => {
  const running = image("/tmp/store-running.png", "store-running.png");
  const activeAddition = image("/tmp/store-active-addition.png", "store-active-addition.png");
  const independent = image("/tmp/store-independent.png", "store-independent.png");
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([running]);
  useAppStore.getState().startBatch([running.path]);

  useAppStore.getState().addFiles([activeAddition]);
  useAppStore.getState().addFilesForOperation("convert", [activeAddition]);
  useAppStore.getState().addFilesForOperation("resize", [independent]);

  const state = useAppStore.getState();
  assert.deepEqual(
    state.files.map((file) => file.path),
    [running.path],
  );
  assert.equal(state.queueItems[activeAddition.path], undefined);
  assert.deepEqual(
    state.sessions.resize.files.map((file) => file.path),
    [independent.path],
  );
  assert.match(state.rejectionMessage, /current job to finish/i);

  state.reset();
  state.setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("operation-scoped intake enforces the queue bound atomically", () => {
  const files = Array.from({ length: 105 }, (_, index) =>
    image(`/tmp/scoped-${index}.png`, `scoped-${index}.png`),
  );

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFilesForOperation("convert", files);

  const state = useAppStore.getState();
  assert.equal(state.files.length, 100);
  assert.equal(state.rejectionMessage, "Queue limit reached. 5 file(s) skipped.");
  state.reset();
});

test("appending to an independent completed queue preserves its finished outputs", () => {
  const completed = image("/tmp/completed-optimize.png", "completed-optimize.png");
  const handedOff = image("/tmp/handed-off-optimize.png", "handed-off-optimize.png");
  const result = {
    output_path: "/tmp/completed-optimize-output.png",
    output_paths: ["/tmp/completed-optimize-output.png"],
    output_size: 1024,
    duration_ms: 10,
  };

  useAppStore.getState().setOperation("optimize");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([completed]);
  useAppStore.getState().completeQueueItem(completed.path, result);
  useAppStore.getState().finishBatch();
  useAppStore.getState().addFilesForOperation("optimize", [handedOff]);

  const state = useAppStore.getState();
  assert.equal(state.state, "loaded");
  assert.equal(state.queueItems[completed.path]?.status, "completed");
  assert.deepEqual(state.queueItems[completed.path]?.result, result);
  assert.equal(state.queueItems[handedOff.path]?.status, "pending");
  assert.deepEqual(state.results, [result]);

  state.reset();
  state.setOperation("convert");
});

test("an unchanged Optimize result remains visibly honest", () => {
  const source = image("/tmp/already-optimized.png", "already-optimized.png");
  const result = {
    output_path: source.path,
    output_paths: [source.path],
    output_size: source.size,
    duration_ms: 10,
  };

  useAppStore.getState().setOperation("optimize");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([source]);
  useAppStore.getState().completeQueueItem(source.path, result);

  const item = useAppStore.getState().queueItems[source.path];
  assert.equal(item?.status, "completed");
  assert.equal(item?.stage, "Already optimized");
  assert.deepEqual(item?.result, result);

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("adding to a whole-queue operation invalidates the previous group result", () => {
  const first = image("/tmp/merge-first.png", "merge-first.png");
  const second = image("/tmp/merge-second.png", "merge-second.png");
  const addition = image("/tmp/merge-addition.png", "merge-addition.png");
  const result = {
    output_path: "/tmp/merged.pdf",
    output_paths: ["/tmp/merged.pdf"],
    output_size: 4096,
    duration_ms: 12,
  };

  useAppStore.getState().setOperation("mergePdf");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([first, second]);
  useAppStore.getState().completeGroupJob([first.path, second.path], result);
  useAppStore.getState().finishBatch();
  useAppStore.getState().addFiles([addition]);

  const state = useAppStore.getState();
  assert.deepEqual(
    state.files.map((file) => file.path),
    [first.path, second.path, addition.path],
  );
  assert.deepEqual(
    state.files.map((file) => state.queueItems[file.path]?.status),
    ["pending", "pending", "pending"],
  );
  assert.equal(state.result, null);
  assert.deepEqual(state.results, []);

  state.reset();
  state.setOperation("convert");
});

test("preserves and reorders mixed Combine to PDF inputs as one ordered queue", () => {
  const document = {
    ...image("/tmp/combined-body.pdf", "combined-body.pdf"),
    extension: "pdf",
    category: "document",
    format: "pdf",
    width: null,
    height: null,
    mimeType: "application/pdf",
  };
  const cover = image("/tmp/combined-cover.png", "combined-cover.png");
  const back = {
    ...image("/tmp/combined-back.jpg", "combined-back.jpg"),
    extension: "jpg",
    format: "jpg",
    mimeType: "image/jpeg",
  };

  useAppStore.getState().setOperation("mergePdf");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([document, cover, back]);
  useAppStore.getState().moveFile(back.path, -1);
  useAppStore.getState().moveFile(back.path, -1);

  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [back.path, document.path, cover.path],
  );

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("mergePdf");
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [back.path, document.path, cover.path],
  );

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("bulk output selection updates compatible rows without erasing other overrides", () => {
  const first = image("/tmp/bulk-a.png", "bulk-a.png");
  const second = image("/tmp/bulk-b.png", "bulk-b.png");
  const movie = video("/tmp/bulk-video.mkv", "bulk-video.mkv");

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([first, second, movie]);
  useAppStore.getState().setOutputFormat("webp", second.path);
  const movieFormat = useAppStore.getState().outputFormats[movie.path];

  useAppStore.getState().setCompatibleOutputFormats("jpg");
  const state = useAppStore.getState();
  assert.equal(state.outputFormats[first.path], "jpg");
  assert.equal(state.outputFormats[second.path], "jpg");
  assert.equal(state.outputFormats[movie.path], movieFormat);
  assert.equal(state.outputFormat, "jpg");
  state.reset();
});

test("bulk PDF output updates raster rows without changing incompatible media", () => {
  const picture = image("/tmp/bulk-pdf.png", "bulk-pdf.png");
  const movie = video("/tmp/bulk-pdf-video.mkv", "bulk-pdf-video.mkv");

  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([picture, movie]);
  const movieFormat = useAppStore.getState().outputFormats[movie.path];

  useAppStore.getState().setCompatibleOutputFormats("pdf");
  const state = useAppStore.getState();
  assert.equal(state.outputFormats[picture.path], "pdf");
  assert.equal(state.outputFormats[movie.path], movieFormat);
  state.reset();
});

test("retains searchable PDF OCR output across utility switches", () => {
  const file = image("/tmp/searchable-scan.png", "searchable-scan.png");

  useAppStore.getState().setOperation("recognizeText");
  useAppStore.getState().reset();
  assert.equal(useAppStore.getState().ocrOutputFormat, "text");
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setOcrOutputFormat("searchablePdf");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("recognizeText");

  const restored = useAppStore.getState();
  assert.equal(restored.ocrOutputFormat, "searchablePdf");
  assert.deepEqual(
    restored.files.map((item) => item.path),
    [file.path],
  );

  restored.reset();
  assert.equal(useAppStore.getState().ocrOutputFormat, "text");
  useAppStore.getState().setOperation("convert");
});
