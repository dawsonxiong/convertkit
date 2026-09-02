import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { loadRecentJobSetup } from "../src/hooks/useRecentJobActions.ts";
import {
  captureRecentJobSetup,
  MAX_RECENT_PAGE_SELECTION_LENGTH,
} from "../src/lib/recentJobSetup.ts";
import { runQueue } from "../src/lib/runQueue.ts";
import { parseRecentJobs } from "../src/store/useRecentJobs.ts";
import { useAppStore } from "../src/store/useAppStore.ts";
import { useRecentJobs } from "../src/store/useRecentJobs.ts";

function file(path, format = "png", relativePath = null) {
  return {
    path,
    name: path.split("/").pop(),
    extension: format,
    size: 2_048,
    category: format === "pdf" ? "document" : format === "mkv" ? "video" : "image",
    format,
    width: format === "pdf" ? null : 1_200,
    height: format === "pdf" ? null : 942,
    relativePath,
    mimeType: null,
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function outputOptions(directory, suffix) {
  return { directory, suffix };
}

function recentJob(operation, input, setup) {
  return {
    id: `job-${operation}`,
    operation,
    completedAt: 1_700_000_000_000,
    items: [
      {
        inputPath: input.path,
        inputName: input.name,
        outputPath: `/tmp/out-${input.name}`,
        outputPaths: [`/tmp/out-${input.name}`],
        status: "completed",
      },
    ],
    setup,
  };
}

function releaseAndReset(...operations) {
  const admission = useAppStore.getState().workspaceAdmission;
  if (admission.token) useAppStore.getState().releaseWorkspaceClaim(admission.token);
  const resetOperations = new Set(["convert", ...operations]);
  for (const operation of resetOperations) {
    if (useAppStore.getState().operation !== operation)
      useAppStore.getState().setOperation(operation);
    useAppStore.getState().reset();
  }
  if (useAppStore.getState().operation !== "convert")
    useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
}

test("captures bounded output, per-file, page, track, and relative-path setup from frozen requests", () => {
  const first = file("/tmp/a.png", "png", "nested/a.png");
  const second = file("/tmp/b.jpg", "jpeg");
  const convert = captureRecentJobSetup(
    [
      {
        operation: "convert",
        inputPath: first.path,
        outputFormat: "webp",
        jobId: null,
        outputOptions: outputOptions("/tmp/out/nested", "-ready"),
      },
      {
        operation: "convert",
        inputPath: second.path,
        outputFormat: "png",
        jobId: null,
        outputOptions: outputOptions("/tmp/out", "-ready"),
      },
    ],
    [first, second],
    "/tmp/out",
    "-ready",
  );
  assert.deepEqual(convert, {
    version: 1,
    directory: "/tmp/out",
    suffix: "-ready",
    inputs: {
      [first.path]: { outputFormat: "webp", relativePath: "nested/a.png" },
      [second.path]: { outputFormat: "png" },
    },
  });

  const pdf = file("/tmp/report.pdf", "pdf");
  assert.deepEqual(
    captureRecentJobSetup(
      [
        {
          operation: "exportPdfPages",
          inputPath: pdf.path,
          mode: "extract",
          pageSelection: "1-3, 5",
          outputFormat: "jpeg",
          resolution: "print",
          jobId: null,
          outputOptions: outputOptions(null, "-page"),
        },
      ],
      [pdf],
      null,
      "-page",
    ),
    {
      version: 1,
      directory: null,
      suffix: "-page",
      settings: {
        kind: "exportPdfPages",
        mode: "extract",
        outputFormat: "jpeg",
        resolution: "print",
      },
      pageSelection: "1-3, 5",
    },
  );

  const video = file("/tmp/movie.mkv", "mkv");
  assert.deepEqual(
    captureRecentJobSetup(
      [
        {
          operation: "extractAudio",
          inputPath: video.path,
          streamIndex: 3,
          streamCodec: "aac",
          outputFormat: "m4a",
          jobId: null,
          outputOptions: outputOptions(null, "-audio"),
        },
      ],
      [video],
      null,
      "-audio",
    )?.inputs,
    { [video.path]: { trackIndex: 3 } },
  );

  const photo = { ...file("/tmp/photo.jpeg", "jpeg"), size: 4 * 1024 * 1024 };
  assert.deepEqual(
    captureRecentJobSetup(
      [
        {
          operation: "optimize",
          inputPath: photo.path,
          keepMetadata: false,
          compressionGoal: "fileSize",
          targetSizeBytes: 500 * 1024,
          jobId: null,
          outputOptions: outputOptions(null, "-optimized"),
        },
      ],
      [photo],
      null,
      "-optimized",
    )?.settings,
    {
      kind: "optimize",
      keepMetadata: false,
      compressionGoal: "fileSize",
      targetSizeBytes: 500 * 1024,
    },
  );
});

test("runQueue records the claimed request setup instead of later store changes", async () => {
  releaseAndReset("resize");
  useRecentJobs.setState({ jobs: [], recordingEnabled: true });
  const source = file("/tmp/claimed.png");
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().addFiles([source]);
  useAppStore.getState().setResizeDimensions(640, 480);
  useAppStore.getState().setPreserveAspect(false);
  useAppStore.getState().setOutputDirectory("/tmp/claimed-output");
  useAppStore.getState().setOutputSuffix("resize", "-claimed");

  const buildRequest = (input, jobId = null) => ({
    operation: "resize",
    inputPath: input.path,
    width: useAppStore.getState().resizeWidth,
    height: useAppStore.getState().resizeHeight,
    preserveAspect: useAppStore.getState().preserveAspect,
    jobId,
    outputOptions: outputOptions(
      useAppStore.getState().outputDirectory,
      useAppStore.getState().outputSuffixes.resize,
    ),
  });
  await runQueue([source], buildRequest, undefined, {
    dependencies: {
      checkCapabilities: async () => {
        useAppStore.getState().setResizeDimensions(10, 10);
        useAppStore.getState().setOutputSuffix("resize", "-racing");
        return [
          {
            inputPath: source.path,
            available: true,
            engine: "test",
            missingTools: [],
            issue: null,
            message: null,
          },
        ];
      },
      executeJob: async () => ({
        output_path: "/tmp/claimed-output/claimed-claimed.png",
        output_paths: ["/tmp/claimed-output/claimed-claimed.png"],
        output_size: 1_024,
        duration_ms: 5,
      }),
      createId: (() => {
        const ids = ["run-claim", "job-claim"];
        return () => ids.shift();
      })(),
    },
  });

  assert.deepEqual(useRecentJobs.getState().jobs[0].setup, {
    version: 1,
    directory: "/tmp/claimed-output",
    suffix: "-claimed",
    settings: { kind: "resize", width: 640, height: 480, preserveAspect: false },
  });
  useRecentJobs.setState({ jobs: [], recordingEnabled: true });
  releaseAndReset("resize");
});

test("parses version-one setup while retaining legacy or malformed setup as source-only history", () => {
  const source = file("/tmp/source.png");
  const setup = {
    version: 1,
    directory: "/tmp/out",
    suffix: "-small",
    settings: { kind: "resize", width: 640, height: 480, preserveAspect: true },
    inputs: { [source.path]: { relativePath: "folder/source.png" } },
  };
  const current = recentJob("resize", source, setup);
  const legacy = { ...recentJob("resize", source, undefined) };
  delete legacy.setup;
  const pdfSource = file("/tmp/unbounded.pdf", "pdf");
  const unbounded = recentJob("exportPdfPages", pdfSource, {
    version: 1,
    directory: null,
    suffix: "-page",
    settings: {
      kind: "exportPdfPages",
      mode: "extract",
      outputFormat: "png",
      resolution: "screen",
    },
    pageSelection: "1".repeat(MAX_RECENT_PAGE_SELECTION_LENGTH + 1),
  });
  const unknown = recentJob("resize", source, { ...setup, version: 2 });

  const parsed = parseRecentJobs(JSON.stringify([current, legacy, unbounded, unknown]));
  assert.deepEqual(parsed[0].setup, setup);
  assert.equal("setup" in parsed[1], false);
  assert.equal("setup" in parsed[2], false);
  assert.equal("setup" in parsed[3], false);
});

test("loads a job under admission and atomically replaces only its target session", async () => {
  releaseAndReset("resize");
  const unrelated = file("/tmp/unrelated.png");
  useAppStore.getState().addFiles([unrelated]);
  useAppStore.getState().setOutputFormat("jpeg", unrelated.path);

  const staleTarget = file("/tmp/stale.png");
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().addFiles([staleTarget]);
  useAppStore.getState().setResizeDimensions(10, 10);
  useAppStore.getState().setOperation("convert");

  const restored = file("/tmp/restored.png", "png", null);
  const setup = {
    version: 1,
    directory: "/tmp/exports",
    suffix: "-client",
    settings: { kind: "resize", width: 800, height: 600, preserveAspect: false },
    inputs: { [restored.path]: { relativePath: "client/restored.png" } },
  };
  const loaded = await loadRecentJobSetup(recentJob("resize", restored, setup), async (...args) => {
    assert.equal(useAppStore.getState().workspaceAdmission.phase, "restoring");
    assert.equal(useAppStore.getState().workspaceAdmission.operation, "resize");
    assert.deepEqual(args.slice(1), ["resize", 100, []]);
    useAppStore.getState().setOperation("resize");
    useAppStore.getState().addFiles([file("/tmp/racing.png")]);
    return { files: [restored], skippedCount: 0, truncated: false };
  });

  assert.equal(loaded, true);
  const active = useAppStore.getState();
  assert.equal(active.operation, "resize");
  assert.deepEqual(
    active.files.map((item) => item.path),
    [restored.path],
  );
  assert.equal(active.files[0].relativePath, "client/restored.png");
  assert.equal(active.resizeWidth, 800);
  assert.equal(active.resizeHeight, 600);
  assert.equal(active.preserveAspect, false);
  assert.equal(active.outputDirectory, "/tmp/exports");
  assert.equal(active.outputSuffixes.resize, "-client");
  assert.equal(active.state, "loaded");

  active.setOperation("convert");
  assert.deepEqual(
    useAppStore.getState().files.map((item) => item.path),
    [unrelated.path],
  );
  assert.equal(useAppStore.getState().outputFormats[unrelated.path], "jpeg");
  releaseAndReset("resize");
});

test("revalidates duplicate sources without exclusions and restores safe per-input settings", async () => {
  releaseAndReset("convert", "exportPdfPages");
  const source = file("/tmp/duplicate.png");
  useAppStore.getState().addFiles([source]);
  useAppStore.getState().setOutputFormat("jpeg", source.path);
  const setup = {
    version: 1,
    directory: null,
    suffix: "-again",
    inputs: { [source.path]: { outputFormat: "webp" } },
  };
  let exclusions;
  assert.equal(
    await loadRecentJobSetup(recentJob("convert", source, setup), async (...args) => {
      exclusions = args[3];
      return { files: [source], skippedCount: 0, truncated: false };
    }),
    true,
  );
  assert.deepEqual(exclusions, []);
  assert.deepEqual(
    useAppStore.getState().files.map((item) => item.path),
    [source.path],
  );
  assert.equal(useAppStore.getState().outputFormats[source.path], "webp");

  const pdf = file("/tmp/pages.pdf", "pdf");
  const pdfSetup = {
    version: 1,
    directory: null,
    suffix: "-pages",
    settings: {
      kind: "exportPdfPages",
      mode: "extract",
      outputFormat: "jpeg",
      resolution: "print",
    },
    pageSelection: "2, 4-6",
  };
  assert.equal(
    await loadRecentJobSetup(recentJob("exportPdfPages", pdf, pdfSetup), async () => ({
      files: [pdf],
      skippedCount: 0,
      truncated: false,
    })),
    true,
  );
  assert.equal(useAppStore.getState().pdfSplitMode, "extract");
  assert.equal(useAppStore.getState().pdfPageSelection, "2, 4-6");
  assert.equal(useAppStore.getState().pdfPageImageFormat, "jpeg");
  assert.equal(useAppStore.getState().pdfPageImageResolution, "print");
  releaseAndReset("exportPdfPages");
});

test("one synchronous load claim prevents a second collector from racing it", async () => {
  releaseAndReset("resize");
  const first = file("/tmp/first.png");
  const second = file("/tmp/second.png");
  const setup = {
    version: 1,
    directory: null,
    suffix: "-loaded",
    settings: { kind: "resize", width: 500, height: 500, preserveAspect: true },
  };
  let finishFirst;
  const firstGate = new Promise((resolve) => {
    finishFirst = resolve;
  });
  let secondCollected = false;
  const firstLoad = loadRecentJobSetup(recentJob("resize", first, setup), async () => {
    await firstGate;
    return { files: [first], skippedCount: 0, truncated: false };
  });
  const secondLoad = loadRecentJobSetup(recentJob("resize", second, setup), async () => {
    secondCollected = true;
    return { files: [second], skippedCount: 0, truncated: false };
  });

  assert.equal(await secondLoad, false);
  assert.equal(secondCollected, false);
  finishFirst();
  assert.equal(await firstLoad, true);
  assert.deepEqual(
    useAppStore.getState().files.map((item) => item.path),
    [first.path],
  );
  releaseAndReset("resize");
});

test("missing or failed source validation releases admission without clobbering target state", async () => {
  releaseAndReset("resize");
  const existing = file("/tmp/existing.png");
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().addFiles([existing]);
  useAppStore.getState().setResizeDimensions(320, 200);
  const setup = {
    version: 1,
    directory: null,
    suffix: "-new",
    settings: { kind: "resize", width: 800, height: 600, preserveAspect: true },
  };
  const missing = file("/tmp/missing.png");

  assert.equal(
    await loadRecentJobSetup(recentJob("resize", missing, setup), async () => ({
      files: [],
      skippedCount: 1,
      truncated: false,
    })),
    false,
  );
  assert.deepEqual(
    useAppStore.getState().files.map((item) => item.path),
    [existing.path],
  );
  assert.equal(useAppStore.getState().resizeWidth, 320);
  assert.equal(useAppStore.getState().workspaceAdmission.phase, "idle");

  const originalConsoleError = console.error;
  console.error = () => {};
  try {
    assert.equal(
      await loadRecentJobSetup(recentJob("resize", missing, setup), async () => {
        throw new Error("validation failed");
      }),
      false,
    );
  } finally {
    console.error = originalConsoleError;
  }
  assert.deepEqual(
    useAppStore.getState().files.map((item) => item.path),
    [existing.path],
  );
  assert.equal(useAppStore.getState().resizeWidth, 320);
  assert.equal(useAppStore.getState().workspaceAdmission.phase, "idle");
  releaseAndReset("resize");
});

test("Activity distinguishes setup-aware Load job from legacy Open sources", async () => {
  const [history, recent] = await Promise.all([
    readFile(new URL("../src/components/HistoryWorkspace.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/components/RecentJobs.tsx", import.meta.url), "utf8"),
  ]);
  assert.match(history, /job\.setup \? "Load job" : "Open sources"/);
  assert.match(recent, /job\.setup \? "Load job" : "Open sources"/);
});
