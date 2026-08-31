import assert from "node:assert/strict";
import test from "node:test";
import {
  outputPathsForRecentJob,
  recentJobIssue,
  recentJobOutcome,
  relativeJobTime,
} from "../src/lib/recentJobDisplay.ts";

const completedJob = {
  id: "job-1",
  operation: "exportImages",
  completedAt: 1_000,
  items: [
    {
      inputPath: "/in/photo.png",
      inputName: "photo.png",
      outputPath: "/out/photo-web.webp",
      outputPaths: ["/out/photo-web.webp", "/out/photo-email.jpg"],
      status: "completed",
    },
    {
      inputPath: "/in/hero.png",
      inputName: "hero.png",
      outputPath: "/out/photo-email.jpg",
      outputPaths: ["/out/photo-email.jpg", "/out/hero-web.webp"],
      status: "completed",
    },
  ],
};

test("activity output paths preserve order and deduplicate multi-output jobs", () => {
  assert.deepEqual(outputPathsForRecentJob(completedJob), [
    "/out/photo-web.webp",
    "/out/photo-email.jpg",
    "/out/hero-web.webp",
  ]);
  assert.equal(recentJobIssue(completedJob), "");
  assert.equal(recentJobOutcome(completedJob), "2 completed");
});

test("activity summarizes partial and cancelled outcomes honestly", () => {
  const partial = {
    ...completedJob,
    items: [
      completedJob.items[0],
      {
        inputPath: "/in/broken.png",
        inputName: "broken.png",
        status: "failed",
        errorKind: "ProcessFailed",
      },
      {
        inputPath: "/in/skipped.png",
        inputName: "skipped.png",
        status: "skipped",
      },
    ],
  };
  const cancelled = {
    ...completedJob,
    items: [
      {
        inputPath: "/in/video.mp4",
        inputName: "video.mp4",
        status: "cancelled",
        errorKind: "Cancelled",
      },
    ],
  };

  assert.equal(recentJobIssue(partial), "1 failed, 1 skipped");
  assert.equal(recentJobOutcome(partial), "1 failed, 1 skipped");
  assert.equal(recentJobIssue(cancelled), "Cancelled");
  assert.equal(recentJobOutcome(cancelled), "Cancelled");
});

test("activity relative time is stable across minute, hour, and day boundaries", () => {
  const now = 10 * 24 * 60 * 60_000;
  assert.equal(relativeJobTime(now, now), "Now");
  assert.equal(relativeJobTime(now - 12 * 60_000, now), "12m");
  assert.equal(relativeJobTime(now - 5 * 60 * 60_000, now), "5h");
  assert.equal(relativeJobTime(now - 3 * 24 * 60 * 60_000, now), "3d");
});
