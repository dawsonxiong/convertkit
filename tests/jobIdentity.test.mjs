import assert from "node:assert/strict";
import test from "node:test";

import {
  CANCEL_FALLBACK_MS,
  cancelJobWithFallback,
} from "../src/lib/jobCancellation.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

function image(path) {
  return {
    path,
    name: path.split("/").pop(),
    extension: "png",
    size: 2048,
    category: "image",
    format: "png",
    width: 1200,
    height: 942,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function fakeTimers() {
  let nextId = 1;
  const callbacks = new Map();
  const cleared = [];

  return {
    timers: {
      setTimeout(callback, delay) {
        assert.equal(delay, CANCEL_FALLBACK_MS);
        const id = nextId++;
        callbacks.set(id, callback);
        return id;
      },
      clearTimeout(id) {
        cleared.push(id);
        callbacks.delete(id);
      },
    },
    cleared,
    get pendingCount() {
      return callbacks.size;
    },
    advance() {
      const pending = [...callbacks.entries()];
      callbacks.clear();
      for (const [, callback] of pending) callback();
    },
  };
}

function startJob(path, jobId) {
  const file = image(path);
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([file]);
  useAppStore.getState().startBatch([path]);
  useAppStore.getState().startQueueItem(path, jobId);
  return file;
}

test("successful cancellation clears its fallback timer", async () => {
  const clock = fakeTimers();
  const cancelled = [];
  const fallbacks = [];

  await cancelJobWithFallback(
    "job-a",
    async (jobId) => cancelled.push(jobId),
    (jobId) => fallbacks.push(jobId),
    clock.timers,
  );

  assert.deepEqual(cancelled, ["job-a"]);
  assert.equal(clock.pendingCount, 0);
  assert.deepEqual(clock.cleared, [1]);
  clock.advance();
  assert.deepEqual(fallbacks, []);
});

test("cancel A fallback cannot terminalize a subsequently active job B", async () => {
  startJob("/tmp/job-a.png", "job-a");
  const clock = fakeTimers();
  let finishCancellation;
  const cancellation = cancelJobWithFallback(
    "job-a",
    () => new Promise((resolve) => (finishCancellation = resolve)),
    (jobId) =>
      useAppStore.getState().setJobError(jobId, { kind: "Cancelled", detail: {} }),
    clock.timers,
  );

  const second = startJob("/tmp/job-b.png", "job-b");
  clock.advance();

  const active = useAppStore.getState();
  assert.equal(active.state, "converting");
  assert.equal(active.jobId, "job-b");
  assert.equal(active.activePath, second.path);
  assert.equal(active.queueItems[second.path].status, "running");

  finishCancellation();
  await cancellation;
  useAppStore.getState().reset();
});

test("progress updates are accepted only for the active job identity", () => {
  const file = startJob("/tmp/progress.png", "job-current");

  useAppStore.getState().updateProgress("job-stale", 93, "Stale stage");
  let current = useAppStore.getState();
  assert.equal(current.progress, -1);
  assert.equal(current.queueItems[file.path].progress, -1);
  assert.equal(current.queueItems[file.path].stage, "Starting");

  useAppStore.getState().updateProgress("job-current", 42, "Encoding");
  current = useAppStore.getState();
  assert.equal(current.progress, 42);
  assert.equal(current.progressStage, "Encoding");
  assert.equal(current.queueItems[file.path].progress, 42);
  assert.equal(current.queueItems[file.path].stage, "Encoding");
  current.reset();
});

test("matching cancel fallback clears identity and every running queue row", async () => {
  const first = image("/tmp/first.png");
  const second = image("/tmp/second.png");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([first, second]);
  useAppStore.getState().startBatch([first.path, second.path]);
  useAppStore.getState().startGroupJob([first.path, second.path], "job-group");
  useAppStore.getState().requestBatchCancel();
  const clock = fakeTimers();
  let finishCancellation;
  const cancellation = cancelJobWithFallback(
    "job-group",
    () => new Promise((resolve) => (finishCancellation = resolve)),
    (jobId) =>
      useAppStore.getState().setJobError(jobId, { kind: "Cancelled", detail: {} }),
    clock.timers,
  );
  clock.advance();

  const terminal = useAppStore.getState();
  assert.equal(terminal.state, "error");
  assert.equal(terminal.jobId, null);
  assert.equal(terminal.activePath, null);
  assert.equal(terminal.cancelBatchRequested, false);
  assert.equal(
    Object.values(terminal.queueItems).some((item) => item.status === "running"),
    false,
  );
  assert.deepEqual(
    Object.values(terminal.queueItems).map((item) => item.status),
    ["cancelled", "cancelled"],
  );

  finishCancellation();
  await cancellation;
  terminal.reset();
  const reset = useAppStore.getState();
  assert.equal(reset.state, "empty");
  assert.equal(reset.jobId, null);
  assert.equal(reset.activePath, null);
  assert.equal(reset.cancelBatchRequested, false);
  assert.deepEqual(reset.queueItems, {});
});
