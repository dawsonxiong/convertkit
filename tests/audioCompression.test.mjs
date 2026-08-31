import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { OPERATIONS, getOperationDialogFilter } from "../src/lib/operations.ts";
import { captureRecentJobSetup } from "../src/lib/recentJobSetup.ts";
import {
  captureSavedRecipeSettings,
  parseSavedRecipes,
  serializeSavedRecipes,
} from "../src/lib/savedRecipes.ts";
import { TOOL_SECTIONS } from "../src/lib/toolNavigation.ts";
import { currentWorkspaceDraft, restoreWorkspaceDraft } from "../src/store/useAppStore.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

const audio = {
  path: "/tmp/interview.wav",
  name: "interview.wav",
  extension: "wav",
  size: 8 * 1024 * 1024,
  category: "audio",
  format: "wav",
  width: null,
  height: null,
  mimeType: "audio/wav",
  createdAt: null,
  modifiedAt: null,
  readOnly: false,
};

test("registers Compress audio as an active M4A utility for every supported input", () => {
  const operation = OPERATIONS.compressAudio;
  assert.deepEqual(operation.extensions, ["mp3", "wav", "wave", "aac", "flac", "ogg", "oga", "m4a"]);
  assert.match(operation.description, /M4A/);
  assert.deepEqual(getOperationDialogFilter("compressAudio"), [
    { name: "Audio files", extensions: operation.extensions },
  ]);
  assert.equal(
    TOOL_SECTIONS.find((section) => section.id === "video-audio")?.operations.includes(
      "compressAudio",
    ),
    true,
  );
});

test("uses three compact shared preset choices with exact bitrate labels", async () => {
  const source = await readFile(
    new URL("../src/components/AudioCompressionPanel.tsx", import.meta.url),
    "utf8",
  );
  for (const value of ["high", "balanced", "smallest"]) assert.match(source, new RegExp(`value: "${value}"`));
  for (const bitrate of ["256 kbps", "160 kbps", "96 kbps"]) assert.match(source, new RegExp(bitrate));
  assert.match(source, /choice-button choice-button-detail/);
  assert.match(source, /role="radiogroup"/);
  assert.match(source, /handleRadioGroupKeyDown/);
  assert.doesNotMatch(source, /animate-|transition-|description/i);
});

test("retains and restores the preset, and applies saved recipes", () => {
  useAppStore.getState().setOperation("compressAudio");
  useAppStore.getState().reset();
  assert.equal(useAppStore.getState().audioCompressionPreset, "balanced");
  useAppStore.getState().addFiles([audio]);
  useAppStore.getState().setAudioCompressionPreset("smallest");
  useAppStore.getState().setOperation("convert");
  useAppStore.getState().setOperation("compressAudio");
  assert.equal(useAppStore.getState().audioCompressionPreset, "smallest");

  const draft = currentWorkspaceDraft();
  restoreWorkspaceDraft(draft, { compressAudio: [audio] });
  assert.equal(useAppStore.getState().audioCompressionPreset, "smallest");
  assert.equal(useAppStore.getState().queueItems[audio.path].status, "pending");

  const settings = captureSavedRecipeSettings("compressAudio", useAppStore.getState());
  assert.deepEqual(settings, { kind: "compressAudio", preset: "smallest" });
  const recipes = parseSavedRecipes(
    serializeSavedRecipes([
      {
        id: "audio-small",
        name: "Small audio",
        operation: "compressAudio",
        directory: null,
        suffix: "-small",
        settings,
      },
    ]),
  );
  assert.equal(recipes.length, 1);
  useAppStore.getState().setAudioCompressionPreset("high");
  useAppStore.getState().applyRecipeSettings(recipes[0].settings);
  assert.equal(useAppStore.getState().audioCompressionPreset, "smallest");

  useAppStore.getState().reset();
  useAppStore.getState().setOperation("convert");
});

test("preserves the typed preset through Activity setup", () => {
  const outputOptions = { directory: "/tmp/out", suffix: "-compressed" };
  const request = {
    operation: "compressAudio",
    inputPath: audio.path,
    preset: "high",
    jobId: "audio-job",
    outputOptions,
  };
  assert.deepEqual(
    captureRecentJobSetup([request], [audio], "/tmp/out", "-compressed"),
    {
      version: 1,
      directory: "/tmp/out",
      suffix: "-compressed",
      settings: { kind: "compressAudio", preset: "high" },
    },
  );

});

test("builds capability-checked compressAudio requests from the selected preset", async () => {
  const [builder, capabilities] = await Promise.all([
    readFile(new URL("../src/hooks/useJobRequestBuilder.ts", import.meta.url), "utf8"),
    readFile(new URL("../src/hooks/useJobCapabilities.ts", import.meta.url), "utf8"),
  ]);
  assert.match(builder, /operation === "compressAudio"/);
  assert.match(builder, /preset: audioCompressionPreset/);
  assert.match(builder, /audioCompressionPreset,/);
  assert.match(capabilities, /checkJobCapabilities\(requests\)/);
});
