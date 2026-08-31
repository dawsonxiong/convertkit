import assert from "node:assert/strict";
import test from "node:test";

import { isActiveOperation, JOB_OPERATIONS, RETIRED_OPERATIONS } from "../src/lib/operations.ts";
import {
  parseRecentJobs,
  parseRecentJobsRecording,
  useRecentJobs,
} from "../src/store/useRecentJobs.ts";

const baseJob = {
  id: "job-1",
  operation: "exportImages",
  completedAt: 1_700_000_000_000,
  items: [
    {
      inputPath: "/tmp/source.png",
      inputName: "source.png",
      outputPath: "/tmp/source-web.webp",
      outputPaths: ["/tmp/source-web.webp", "/tmp/source-email.jpg", "/tmp/source-preview.webp"],
    },
  ],
};

test("retains every output path from a multi-variant image export", () => {
  const parsed = parseRecentJobs(JSON.stringify([baseJob]));
  assert.equal(parsed.length, 1);
  assert.deepEqual(parsed[0].items[0].outputPaths, baseJob.items[0].outputPaths);
});

test("keeps legacy single-output history entries compatible", () => {
  const legacy = {
    ...baseJob,
    operation: "resize",
    items: [
      {
        inputPath: "/tmp/source.png",
        inputName: "source.png",
        outputPath: "/tmp/source-resized.png",
      },
    ],
  };
  assert.deepEqual(parseRecentJobs(JSON.stringify([legacy])), [legacy]);
});

test("rejects invalid or unbounded output path lists", () => {
  const invalid = {
    ...baseJob,
    items: [{ ...baseJob.items[0], outputPaths: Array(101).fill("/tmp/output.webp") }],
  };
  assert.deepEqual(parseRecentJobs(JSON.stringify([invalid])), []);
  assert.deepEqual(parseRecentJobs("not-json"), []);
});

test("drops persisted history for retired operations", () => {
  const retiredJobs = RETIRED_OPERATIONS.map((operation, index) => ({
    ...baseJob,
    id: `retired-${index}`,
    operation,
  }));
  assert.deepEqual(parseRecentJobs(JSON.stringify(retiredJobs)), []);
});

test("retains completed transcription history after restart", () => {
  const transcriptionJob = {
    ...baseJob,
    operation: "transcribe",
    items: [
      {
        inputPath: "/tmp/interview.wav",
        inputName: "interview.wav",
        outputPath: "/tmp/interview.txt",
        outputPaths: ["/tmp/interview.txt"],
        status: "completed",
      },
    ],
  };
  assert.deepEqual(parseRecentJobs(JSON.stringify([transcriptionJob])), [transcriptionJob]);
});

test("accepts history for every active classified job operation", () => {
  for (const operation of JOB_OPERATIONS.filter(isActiveOperation)) {
    const job = { ...baseJob, operation };
    assert.equal(parseRecentJobs(JSON.stringify([job])).length, 1, operation);
  }
});

test("retains safe failed and cancelled outcomes without requiring output paths", () => {
  const outcomeJob = {
    ...baseJob,
    operation: "transcribe",
    items: [
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
    ],
  };
  assert.deepEqual(parseRecentJobs(JSON.stringify([outcomeJob])), [outcomeJob]);
});

test("rejects malformed or unbounded recent outcomes", () => {
  const failedWithoutReason = {
    ...baseJob,
    items: [{ inputPath: "/tmp/fail.png", inputName: "fail.png", status: "failed" }],
  };
  const failedWithOutput = {
    ...baseJob,
    items: [
      {
        inputPath: "/tmp/fail.png",
        inputName: "fail.png",
        outputPath: "/tmp/should-not-exist.png",
        status: "failed",
        errorKind: "ProcessFailed",
      },
    ],
  };
  const tooManyItems = {
    ...baseJob,
    items: Array.from({ length: 101 }, (_, index) => ({
      inputPath: `/tmp/${index}.png`,
      inputName: `${index}.png`,
      outputPath: `/tmp/${index}-out.png`,
    })),
  };

  assert.deepEqual(parseRecentJobs(JSON.stringify([failedWithoutReason])), []);
  assert.deepEqual(parseRecentJobs(JSON.stringify([failedWithOutput])), []);
  assert.deepEqual(parseRecentJobs(JSON.stringify([tooManyItems])), []);
});

test("defaults recent recording on and restores an explicit pause", () => {
  assert.equal(parseRecentJobsRecording(null), true);
  assert.equal(parseRecentJobsRecording("true"), true);
  assert.equal(parseRecentJobsRecording("invalid"), true);
  assert.equal(parseRecentJobsRecording("false"), false);
});

test("does not retain new file paths while recent recording is paused", () => {
  useRecentJobs.setState({ jobs: [], recordingEnabled: false });
  useRecentJobs.getState().addJob({
    operation: baseJob.operation,
    items: baseJob.items,
  });
  assert.deepEqual(useRecentJobs.getState().jobs, []);
  useRecentJobs.setState({ jobs: [], recordingEnabled: true });
});

test("does not retain newly submitted retired jobs", () => {
  useRecentJobs.setState({ jobs: [], recordingEnabled: true });
  for (const operation of RETIRED_OPERATIONS) {
    useRecentJobs.getState().addJob({ operation, items: baseJob.items });
  }
  assert.deepEqual(useRecentJobs.getState().jobs, []);
});
