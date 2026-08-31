import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { SIDEBAR_OPERATIONS } from "../src/lib/toolNavigation.ts";

test("provides a representative loaded fixture for every active sidebar operation", async () => {
  const source = await readFile(
    new URL("../src/dev/visualFixture.ts", import.meta.url),
    "utf8",
  );
  const operationList = source.match(
    /export const VISUAL_FIXTURE_OPERATIONS = new Set<Operation>\(\[([\s\S]*?)\]\);/,
  )?.[1];
  assert.ok(operationList);
  const fixtureOperations = [...operationList.matchAll(/"([A-Za-z]+)"/g)].map(
    (match) => match[1],
  );

  assert.equal(new Set(fixtureOperations).size, fixtureOperations.length);
  assert.deepEqual(new Set(fixtureOperations), new Set(SIDEBAR_OPERATIONS));
  assert.match(source, /store\.reset\(\);\s*store\.addFiles\(loadedFilesForOperation\(operation\)\);/);
  assert.match(
    source,
    /operation === "recognizeText"\) store\.setOcrOutputFormat\("searchablePdf"\)/,
  );
});

test("track-dependent fixtures include usable representative tracks", async () => {
  const source = await readFile(
    new URL("../src/dev/visualFixture.ts", import.meta.url),
    "utf8",
  );
  assert.match(source, /operation === "extractAudio"[\s\S]*store\.setAudioTracks\([\s\S]*codec: "aac"/);
  assert.match(source, /operation === "extractSubtitles"[\s\S]*store\.setSubtitleTracks\([\s\S]*supported: true/);
});

test("the transcription fixture seeds model status before mount and stubs only its refresh", async () => {
  const [source, productionStore] = await Promise.all([
    readFile(new URL("../src/dev/visualFixture.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/store/useTranscriptionModels.ts", import.meta.url), "utf8"),
  ]);
  const seed = source.match(/function seedTranscriptionModelFixture\(\) \{([\s\S]*?)\n\}/)?.[1];
  assert.ok(seed);
  assert.match(seed, /model: "base"[\s\S]*downloaded: true/);
  assert.match(seed, /error: null/);
  assert.match(seed, /refresh: async \(\) => \{\}/);
  assert.match(
    source,
    /if \(operation === "transcribe"\) seedTranscriptionModelFixture\(\);\s*const store = useAppStore/,
  );
  assert.match(
    productionStore,
    /catch \{\s*set\(\{ loading: false, error: "Model status is unavailable" \}\);/,
  );
});
