import assert from "node:assert/strict";
import test from "node:test";

import { unfinishedQueuePaths } from "../src/lib/queueSelection.ts";

function queueItem(status) {
  return { status, progress: 0, stage: "", result: null, error: null };
}

test("default queue runs leave completed handoff targets untouched", () => {
  const files = [{ path: "/tmp/completed.png" }, { path: "/tmp/new.png" }];
  const queueItems = {
    "/tmp/completed.png": queueItem("completed"),
    "/tmp/new.png": queueItem("pending"),
  };

  assert.deepEqual(unfinishedQueuePaths(files, queueItems), ["/tmp/new.png"]);
});

test("default queue runs retain failed and cancelled work for retry", () => {
  const files = [
    { path: "/tmp/failed.png" },
    { path: "/tmp/cancelled.png" },
    { path: "/tmp/missing-state.png" },
  ];
  const queueItems = {
    "/tmp/failed.png": queueItem("failed"),
    "/tmp/cancelled.png": queueItem("cancelled"),
  };

  assert.deepEqual(
    unfinishedQueuePaths(files, queueItems),
    files.map((file) => file.path),
  );
});
