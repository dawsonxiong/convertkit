import assert from "node:assert/strict";
import test from "node:test";

import { addPathsToOperation } from "../src/hooks/useAddPaths.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

function arbitraryFile(path) {
  return {
    path,
    name: path.split("/").pop(),
    extension: "bin",
    size: 12,
    category: "other",
    format: "unknown",
    width: null,
    height: null,
    mimeType: null,
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

test("an in-flight collection stays pinned to its requesting operation", async () => {
  useAppStore.getState().setOperation("inspect");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();

  let finishCollection;
  const collectionGate = new Promise((resolve) => {
    finishCollection = resolve;
  });
  const inspected = arbitraryFile("/tmp/slow-inspection.bin");
  const collect = async () => {
    await collectionGate;
    return { files: [inspected], skippedCount: 0, truncated: false };
  };

  const pending = addPathsToOperation(["/tmp/slow-folder"], "inspect", collect);
  useAppStore.getState().setOperation("resize");
  finishCollection();
  await pending;

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

test("collection capacity and exclusions come from the target session", async () => {
  const existing = arbitraryFile("/tmp/existing.bin");
  useAppStore.getState().setOperation("inspect");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([existing]);
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();

  let invocation;
  await addPathsToOperation(["/tmp/folder"], "inspect", async (...args) => {
    invocation = args;
    return { files: [], skippedCount: 0, truncated: false };
  });

  assert.equal(invocation[1], "inspect");
  assert.equal(invocation[2], 99);
  assert.deepEqual(invocation[3], [existing.path]);

  useAppStore.getState().setOperation("inspect");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("does not start collecting for a target session that is already running", async () => {
  const existing = arbitraryFile("/tmp/running-source.bin");
  const addition = arbitraryFile("/tmp/running-addition.bin");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([existing]);
  useAppStore.getState().startBatch([existing.path]);

  let collected = false;
  await addPathsToOperation([addition.path], "convert", async () => {
    collected = true;
    return { files: [addition], skippedCount: 0, truncated: false };
  });

  assert.equal(collected, false);
  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [existing.path],
  );
  assert.match(useAppStore.getState().rejectionMessage, /current job to finish/i);
  useAppStore.getState().reset();
});

test("drops an async intake result if its target session starts running", async () => {
  const existing = arbitraryFile("/tmp/racing-source.bin");
  const addition = arbitraryFile("/tmp/racing-addition.bin");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([existing]);

  let finishCollection;
  const collectionGate = new Promise((resolve) => {
    finishCollection = resolve;
  });
  const pending = addPathsToOperation([addition.path], "convert", async () => {
    await collectionGate;
    return { files: [addition], skippedCount: 0, truncated: false };
  });

  useAppStore.getState().startBatch([existing.path]);
  finishCollection();
  await pending;

  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [existing.path],
  );
  assert.equal(useAppStore.getState().queueItems[addition.path], undefined);
  assert.match(useAppStore.getState().rejectionMessage, /current job to finish/i);
  useAppStore.getState().reset();
});

test("a running session does not block intake into an independent idle session", async () => {
  const running = arbitraryFile("/tmp/active-running.bin");
  const independent = arbitraryFile("/tmp/independent-idle.bin");
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().reset();
  useAppStore.getState().addFiles([running]);
  useAppStore.getState().startBatch([running.path]);

  await addPathsToOperation([independent.path], "resize", async () => ({
    files: [independent],
    skippedCount: 0,
    truncated: false,
  }));

  assert.deepEqual(
    useAppStore.getState().files.map((file) => file.path),
    [running.path],
  );
  assert.deepEqual(
    useAppStore.getState().sessions.resize.files.map((file) => file.path),
    [independent.path],
  );

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("resize");
  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});
