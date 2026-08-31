import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

const readSource = (path) => readFile(new URL(`../${path}`, import.meta.url), "utf8");

test("rejected input is announced immediately without additional visible copy", async () => {
  const app = await readSource("src/App.tsx");

  assert.match(
    app,
    /\{rejectionMessage && \([\s\S]*?role="alert"[\s\S]*?aria-atomic="true"[\s\S]*?\{rejectionMessage\}/,
  );
});

test("queue progress and terminal outcomes use one stable polite status region", async () => {
  const queue = await readSource("src/components/WorkspaceQueue.tsx");

  assert.match(queue, /const liveStatus = \(\(\) => \{/);
  assert.match(queue, /state === "converting"[\s\S]*?processed/);
  assert.match(queue, /state === "error" \? `Job stopped\./);
  assert.match(queue, /className="sr-only" role="status" aria-live="polite" aria-atomic="true"/);
  assert.match(queue, /aria-busy=\{state === "converting"\}/);
});

test("file and local model progress expose determinate or indeterminate progressbars", async () => {
  const [filePreview, transcription] = await Promise.all([
    readSource("src/components/FilePreview.tsx"),
    readSource("src/components/TranscriptionPanel.tsx"),
  ]);

  for (const source of [filePreview, transcription]) {
    assert.match(source, /role="progressbar"/);
    assert.match(source, /aria-valuemin=\{0\}/);
    assert.match(source, /aria-valuemax=\{100\}/);
    assert.match(source, /aria-valuenow=/);
    assert.match(source, /aria-valuetext=/);
  }
  assert.match(filePreview, /progress < 0 \? undefined/);
  assert.match(transcription, /Downloading \$\{selected\?\.label \?\? model\} transcription model/);
});
