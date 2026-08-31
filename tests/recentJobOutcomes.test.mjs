import assert from "node:assert/strict";
import test from "node:test";

import { buildRecentJobItems } from "../src/lib/recentJobOutcomes.ts";

const files = [
  {
    path: "/tmp/complete.wav",
    name: "complete.wav",
    size: 1,
    format: "wav",
    category: "audio",
  },
  {
    path: "/tmp/failed.wav",
    name: "failed.wav",
    size: 1,
    format: "wav",
    category: "audio",
  },
  {
    path: "/tmp/cancelled.wav",
    name: "cancelled.wav",
    size: 1,
    format: "wav",
    category: "audio",
  },
  {
    path: "/tmp/pending.wav",
    name: "pending.wav",
    size: 1,
    format: "wav",
    category: "audio",
  },
];

test("builds bounded recent outcomes for completed, failed, and cancelled queue items", () => {
  const outcomes = buildRecentJobItems(files, {
    "/tmp/complete.wav": {
      status: "completed",
      progress: 100,
      stage: "Complete",
      result: {
        output_path: "/tmp/complete.txt",
        output_paths: [],
        output_size: 20,
        duration_ms: 50,
      },
      error: null,
    },
    "/tmp/failed.wav": {
      status: "failed",
      progress: 0,
      stage: "Failed",
      result: null,
      error: { kind: "MissingDependency", detail: { message: "private engine detail" } },
    },
    "/tmp/cancelled.wav": {
      status: "cancelled",
      progress: 0,
      stage: "Cancelled",
      result: null,
      error: { kind: "Cancelled", detail: {} },
    },
    "/tmp/pending.wav": {
      status: "pending",
      progress: 0,
      stage: "Pending",
      result: null,
      error: null,
    },
  });

  assert.deepEqual(outcomes, [
    {
      inputPath: "/tmp/complete.wav",
      inputName: "complete.wav",
      outputPath: "/tmp/complete.txt",
      outputPaths: ["/tmp/complete.txt"],
      status: "completed",
    },
    {
      inputPath: "/tmp/failed.wav",
      inputName: "failed.wav",
      status: "failed",
      errorKind: "MissingDependency",
    },
    {
      inputPath: "/tmp/cancelled.wav",
      inputName: "cancelled.wav",
      status: "cancelled",
      errorKind: "Cancelled",
    },
  ]);
  assert.equal(JSON.stringify(outcomes).includes("private engine detail"), false);
});
