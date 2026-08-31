import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import test from "node:test";

import {
  TRANSCRIPTION_LANGUAGES,
  TRANSCRIPTION_LANGUAGE_CODES,
  TRANSCRIPTION_LANGUAGE_SET,
  TRANSCRIPTION_MODELS,
} from "../src/lib/transcription.ts";

test("exposes every transcription model and one unique language option", () => {
  assert.deepEqual(TRANSCRIPTION_MODELS, ["tiny", "base", "small"]);
  assert.equal(TRANSCRIPTION_LANGUAGES[0].code, "auto");
  assert.equal(TRANSCRIPTION_LANGUAGES[0].label, "Auto-detect");
  assert.equal(new Set(TRANSCRIPTION_LANGUAGE_CODES).size, TRANSCRIPTION_LANGUAGE_CODES.length);
  assert.equal(
    new Set(TRANSCRIPTION_LANGUAGES.map(({ code }) => code)).size,
    TRANSCRIPTION_LANGUAGES.length,
  );
  for (const { code, label } of TRANSCRIPTION_LANGUAGES) {
    assert.equal(TRANSCRIPTION_LANGUAGE_SET.has(code), true);
    assert.equal(label.length > 0, true);
  }
});

test("keeps the packaged smoke pinned to the backend Tiny-model manifest", () => {
  const backend = readFileSync(resolve("src-tauri/src/commands/transcribe.rs"), "utf8");
  const smoke = readFileSync(resolve("scripts/smoke-whisper-runtime.sh"), "utf8");
  const tiny = backend.match(
    /model: TranscriptionModel::Tiny,[\s\S]*?url: "([^"]+)",[\s\S]*?expected_size: ([\d_]+),[\s\S]*?sha1: "([^"]+)"/,
  );

  assert.ok(tiny, "Tiny model manifest must remain readable");
  const [, url, size, sha1] = tiny;
  assert.match(smoke, new RegExp(`^model_url=${url.replaceAll(".", "\\.")}$`, "m"));
  assert.match(smoke, new RegExp(`^expected_size=${size.replaceAll("_", "")}$`, "m"));
  assert.match(smoke, new RegExp(`^expected_sha1=${sha1}$`, "m"));
});
