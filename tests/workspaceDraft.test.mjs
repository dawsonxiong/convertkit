import assert from "node:assert/strict";
import test from "node:test";

import {
  currentWorkspaceDraft,
  restoreWorkspaceDraft,
  useAppStore,
} from "../src/store/useAppStore.ts";
import {
  ACTIVE_OPERATION_IDS,
  MAX_WORKSPACE_DRAFT_BYTES,
  parseWorkspaceDraft,
  WORKSPACE_DRAFT_VERSION,
} from "../src/lib/workspaceDraft.ts";
import { RETIRED_OPERATIONS } from "../src/lib/operations.ts";

function video(path, name) {
  return {
    path,
    name,
    extension: "mkv",
    size: 4096,
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

test("parses a bounded versioned workspace draft and drops unsafe values", () => {
  const path = "/tmp/movie.mkv";
  const draft = parseWorkspaceDraft(
    JSON.stringify({
      version: WORKSPACE_DRAFT_VERSION,
      activeOperation: "extractAudio",
      sessions: {
        extractAudio: {
          paths: [path, path, "", 42],
          outputFormats: { [path]: "wav", "/tmp/other.mkv": "mp3", bad: "../wav" },
          resizeWidth: -1,
          preserveAspect: false,
          audioOutputFormat: "flac",
          ocrOutputFormat: "searchablePdf",
          audioTrackIndexes: { [path]: 3, "/tmp/other.mkv": 2, bad: 9000 },
          imageExportPresets: ["preview", "bogus", "web", "preview"],
          videoEncodingPreset: "unknown",
          videoResolution: "fullHd",
          videoQuality: "high",
          videoCompressionGoal: "fileSize",
          videoTargetSizeBytes: 25 * 1024 * 1024,
          pdfCompressionGoal: "fileSize",
          pdfTargetSizeBytes: 5 * 1024 * 1024,
          pdfPageSelection: "1-3",
          pdfPageImageFormat: "jpeg",
          pdfPageImageResolution: "print",
          archiveFormat: "sevenZ",
          renameSettings: { prefix: "ready-", numbering: "suffix", start: 4, padding: 3 },
        },
      },
    }),
  );

  assert.ok(draft);
  assert.equal(draft.activeOperation, "extractAudio");
  assert.deepEqual(draft.sessions.extractAudio.paths, [path]);
  assert.deepEqual(draft.sessions.extractAudio.outputFormats, { [path]: "wav" });
  assert.equal(draft.sessions.extractAudio.resizeWidth, null);
  assert.equal(draft.sessions.extractAudio.preserveAspect, false);
  assert.equal(draft.sessions.extractAudio.audioOutputFormat, "flac");
  assert.equal(draft.sessions.extractAudio.ocrOutputFormat, "searchablePdf");
  assert.deepEqual(draft.sessions.extractAudio.audioTrackIndexes, { [path]: 3 });
  assert.deepEqual(draft.sessions.extractAudio.imageExportPresets, ["preview", "web"]);
  assert.equal(draft.sessions.extractAudio.videoEncodingPreset, "compatible");
  assert.equal(draft.sessions.extractAudio.videoResolution, "fullHd");
  assert.equal(draft.sessions.extractAudio.videoQuality, "high");
  assert.equal(draft.sessions.extractAudio.videoCompressionGoal, "fileSize");
  assert.equal(draft.sessions.extractAudio.videoTargetSizeBytes, 25 * 1024 * 1024);
  assert.equal(draft.sessions.extractAudio.pdfCompressionGoal, "fileSize");
  assert.equal(draft.sessions.extractAudio.pdfTargetSizeBytes, 5 * 1024 * 1024);
  assert.equal(draft.sessions.extractAudio.pdfPageImageFormat, "jpeg");
  assert.equal(draft.sessions.extractAudio.pdfPageImageResolution, "print");
  assert.equal(draft.sessions.extractAudio.archiveFormat, "sevenZ");
  assert.equal(draft.sessions.extractAudio.renameSettings.prefix, "ready-");
  assert.equal(draft.sessions.extractAudio.renameSettings.numbering, "suffix");
});

test("rejects malformed, unknown-version, and oversized workspace drafts", () => {
  assert.equal(parseWorkspaceDraft("not-json"), null);
  assert.equal(
    parseWorkspaceDraft(JSON.stringify({ version: 99, activeOperation: "convert", sessions: {} })),
    null,
  );
  assert.equal(parseWorkspaceDraft("x".repeat(MAX_WORKSPACE_DRAFT_BYTES + 1)), null);
});

test("never serializes archive passwords into workspace recovery", () => {
  useAppStore.getState().setOperation("createArchive");
  useAppStore.getState().reset();
  useAppStore.getState().setArchivePassword("do-not-persist");
  useAppStore.getState().setArchivePasswordConfirmation("do-not-persist");

  const serialized = JSON.stringify(currentWorkspaceDraft());
  assert.equal(serialized.includes("do-not-persist"), false);
  assert.equal(serialized.includes("archivePassword"), false);

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("migrates retired active operations to convert and drops retired sessions", () => {
  for (const retiredOperation of RETIRED_OPERATIONS) {
    const parsed = parseWorkspaceDraft(
      JSON.stringify({
        version: WORKSPACE_DRAFT_VERSION,
        activeOperation: retiredOperation,
        sessions: {
          [retiredOperation]: { paths: [`/tmp/${retiredOperation}`] },
          extractAudio: { paths: ["/tmp/active.mkv"] },
        },
      }),
    );

    assert.ok(parsed);
    assert.equal(parsed.activeOperation, "convert", retiredOperation);
    assert.equal(parsed.sessions[retiredOperation], undefined, retiredOperation);
    assert.deepEqual(parsed.sessions.extractAudio.paths, ["/tmp/active.mkv"]);
    assert.deepEqual(
      Object.keys(parsed.sessions).every((operation) => ACTIVE_OPERATION_IDS.includes(operation)),
      true,
    );
  }
});

test("restores revalidated files and turns interrupted work into a pending queue", () => {
  const file = video("/tmp/recovered.mkv", "recovered.mkv");
  useAppStore.getState().setOperation("extractAudio");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setAudioTracks(file.path, [
    {
      streamIndex: 2,
      codec: "aac",
      language: "eng",
      title: "Main mix",
      channels: 2,
      channelLayout: "stereo",
      sampleRate: 48000,
      isDefault: true,
    },
  ]);
  useAppStore.getState().setAudioOutputFormat("wav");
  useAppStore.getState().startBatch([file.path]);
  useAppStore.getState().startQueueItem(file.path, "interrupted-job");

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { extractAudio: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "extractAudio");
  assert.equal(restored.state, "loaded");
  assert.equal(restored.audioOutputFormat, "wav");
  assert.equal(restored.audioTrackIndexes[file.path], 2);
  assert.equal(restored.audioTracks[file.path], undefined);
  assert.equal(restored.queueItems[file.path].status, "pending");
  assert.equal(restored.jobId, null);
  assert.equal(restored.activePath, null);

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("restores image export recipes as pending work", () => {
  const file = {
    path: "/tmp/export.png",
    name: "export.png",
    extension: "png",
    size: 4096,
    category: "image",
    format: "png",
    width: 2400,
    height: 1200,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
  useAppStore.getState().setOperation("exportImages");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setImageExportPresets(["email", "preview"]);

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { exportImages: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "exportImages");
  assert.deepEqual(restored.imageExportPresets, ["email", "preview"]);
  assert.equal(restored.queueItems[file.path].status, "pending");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("restores image file-size optimization as pending work", () => {
  const file = {
    path: "/tmp/optimize.jpeg",
    name: "optimize.jpeg",
    extension: "jpeg",
    size: 4 * 1024 * 1024,
    category: "image",
    format: "jpeg",
    width: 2400,
    height: 1200,
    mimeType: "image/jpeg",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
  useAppStore.getState().setOperation("optimize");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setImageOptimizationGoal("fileSize");
  useAppStore.getState().setImageTargetSizeBytes(600 * 1024);
  useAppStore.getState().setKeepMetadata(true);

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { optimize: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "optimize");
  assert.equal(restored.imageOptimizationGoal, "fileSize");
  assert.equal(restored.imageTargetSizeBytes, 600 * 1024);
  assert.equal(restored.keepMetadata, true);
  assert.equal(restored.queueItems[file.path].status, "pending");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("restores video delivery controls as pending work", () => {
  const file = video("/tmp/delivery.mkv", "delivery.mkv");

  useAppStore.getState().setOperation("encodeVideo");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setVideoEncodingPreset("web");
  useAppStore.getState().setVideoResolution("hd");
  useAppStore.getState().setVideoQuality("high");
  useAppStore.getState().setVideoCompressionGoal("fileSize");
  useAppStore.getState().setVideoTargetSizeBytes(35 * 1024 * 1024);

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { encodeVideo: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "encodeVideo");
  assert.equal(restored.videoEncodingPreset, "web");
  assert.equal(restored.videoResolution, "hd");
  assert.equal(restored.videoQuality, "high");
  assert.equal(restored.videoCompressionGoal, "fileSize");
  assert.equal(restored.videoTargetSizeBytes, 35 * 1024 * 1024);
  assert.equal(restored.queueItems[file.path].status, "pending");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("migrates legacy and malformed video compression drafts to quality defaults", () => {
  const parsed = parseWorkspaceDraft(
    JSON.stringify({
      version: WORKSPACE_DRAFT_VERSION,
      activeOperation: "encodeVideo",
      sessions: {
        encodeVideo: {
          paths: [],
          videoEncodingPreset: "archive",
          videoCompressionGoal: "fileSize",
          videoTargetSizeBytes: 512,
        },
        convert: { paths: [] },
      },
    }),
  );

  assert.ok(parsed);
  assert.equal(parsed.sessions.encodeVideo.videoCompressionGoal, "quality");
  assert.equal(parsed.sessions.encodeVideo.videoTargetSizeBytes, 100 * 1024 * 1024);
  assert.equal(parsed.sessions.convert.videoCompressionGoal, "quality");
  assert.equal(parsed.sessions.convert.videoTargetSizeBytes, 100 * 1024 * 1024);
});

test("restores PDF file-size compression and defaults legacy drafts to quality", () => {
  const file = {
    path: "/tmp/report.pdf",
    name: "report.pdf",
    extension: "pdf",
    size: 20 * 1024 * 1024,
    category: "document",
    format: "pdf",
    width: null,
    height: null,
    mimeType: "application/pdf",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
  useAppStore.getState().setOperation("compressPdf");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setPdfCompressionPreset("smallest");
  useAppStore.getState().setPdfCompressionGoal("fileSize");
  useAppStore.getState().setPdfTargetSizeBytes(5 * 1024 * 1024);

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { compressPdf: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.pdfCompressionPreset, "smallest");
  assert.equal(restored.pdfCompressionGoal, "fileSize");
  assert.equal(restored.pdfTargetSizeBytes, 5 * 1024 * 1024);

  const legacy = parseWorkspaceDraft(
    JSON.stringify({
      version: WORKSPACE_DRAFT_VERSION,
      activeOperation: "compressPdf",
      sessions: { compressPdf: { paths: [], pdfCompressionPreset: "high" } },
    }),
  );
  assert.equal(legacy.sessions.compressPdf.pdfCompressionGoal, "quality");
  assert.equal(legacy.sessions.compressPdf.pdfTargetSizeBytes, 10 * 1024 * 1024);

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("does not persist or restore current retired workspace sessions", () => {
  const file = {
    path: "/tmp/app-icon.png",
    name: "app-icon.png",
    extension: "png",
    size: 4096,
    category: "image",
    format: "png",
    width: 1024,
    height: 1024,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.setState({
    operation: "packageAppIcon",
    state: "loaded",
    files: [file],
    file,
    outputFormats: { [file.path]: "png" },
    outputFormat: "png",
  });

  const draft = currentWorkspaceDraft();
  assert.equal(draft.activeOperation, "convert");
  assert.equal(draft.sessions.packageAppIcon, undefined);

  restoreWorkspaceDraft(draft, { packageAppIcon: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "convert");
  assert.equal(Object.hasOwn(restored.sessions, "packageAppIcon"), false);

  restored.reset();
});

test("restores transcription settings and rejects unsupported language values", () => {
  const file = video("/tmp/transcription.mkv", "transcription.mkv");
  useAppStore.getState().setOperation("transcribe");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setTranscriptionModel("tiny");
  useAppStore.getState().setTranscriptionLanguage("de");
  useAppStore.getState().setTranscriptionOutputFormat("srt");

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { transcribe: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "transcribe");
  assert.equal(restored.transcriptionModel, "tiny");
  assert.equal(restored.transcriptionLanguage, "de");
  assert.equal(restored.transcriptionOutputFormat, "srt");
  assert.equal(restored.queueItems[file.path].status, "pending");

  const parsed = parseWorkspaceDraft(
    JSON.stringify({
      ...draft,
      sessions: {
        transcribe: {
          ...draft.sessions.transcribe,
          transcriptionLanguage: "not-a-whisper-language",
        },
      },
    }),
  );
  assert.equal(parsed.sessions.transcribe.transcriptionLanguage, "auto");

  restored.reset();
  useAppStore.getState().setOperation("convert");
});

test("restores searchable PDF OCR output and migrates missing or malformed values to text", () => {
  const file = {
    path: "/tmp/searchable-scan.png",
    name: "searchable-scan.png",
    extension: "png",
    size: 4096,
    category: "image",
    format: "png",
    width: 1600,
    height: 2200,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };

  useAppStore.getState().setOperation("recognizeText");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().setOcrOutputFormat("searchablePdf");

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { recognizeText: [file] });
  const restored = useAppStore.getState();
  assert.equal(restored.operation, "recognizeText");
  assert.equal(restored.ocrOutputFormat, "searchablePdf");
  assert.equal(restored.queueItems[file.path].status, "pending");

  for (const storedValue of [undefined, "html"]) {
    const session = { paths: [] };
    if (storedValue !== undefined) session.ocrOutputFormat = storedValue;
    const parsed = parseWorkspaceDraft(
      JSON.stringify({
        version: WORKSPACE_DRAFT_VERSION,
        activeOperation: "recognizeText",
        sessions: { recognizeText: session },
      }),
    );
    assert.equal(parsed.sessions.recognizeText.ocrOutputFormat, "text");
  }

  restored.reset();
  useAppStore.getState().setOperation("convert");
});
