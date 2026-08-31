import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import { isInputIntakeBlocked, resolveAppShortcut } from "../src/lib/appShortcuts.ts";
import { isInteractiveKeyboardTarget, isTextEntryTarget } from "../src/lib/interactionTargets.ts";

const base = {
  key: "",
  command: false,
  interactiveTarget: false,
  state: "empty",
  operation: "convert",
  auxiliaryViewActive: false,
  hasOutput: false,
};

test("resolves documented command shortcuts without hijacking automation", () => {
  assert.equal(resolveAppShortcut({ ...base, key: "o", command: true }), "open");
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "R",
      command: true,
      state: "done",
      hasOutput: true,
    }),
    "reveal",
  );
  assert.equal(resolveAppShortcut({ ...base, key: "r", command: true, state: "done" }), null);
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "o",
      command: true,
      auxiliaryViewActive: true,
    }),
    null,
  );
});

test("blocks every intake entry point during active work or auxiliary views", () => {
  assert.equal(isInputIntakeBlocked("loaded"), false);
  assert.equal(isInputIntakeBlocked("converting"), true);
  assert.equal(isInputIntakeBlocked("loaded", true), true);
  assert.equal(isInputIntakeBlocked("loaded", false, true), true);
  assert.equal(resolveAppShortcut({ ...base, key: "o", command: true, state: "converting" }), null);
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "o",
      command: true,
      state: "loaded",
      workspaceAdmissionPending: true,
    }),
    null,
  );
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "Enter",
      state: "loaded",
      workspaceAdmissionPending: true,
    }),
    null,
  );
});

test("runs only when focus is outside interactive controls", () => {
  assert.equal(resolveAppShortcut({ ...base, key: "Enter", state: "loaded" }), "run");
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "Enter",
      state: "loaded",
      interactiveTarget: true,
    }),
    null,
  );
  assert.equal(
    resolveAppShortcut({ ...base, key: "Enter", state: "loaded", operation: "inspect" }),
    null,
  );
  assert.equal(
    resolveAppShortcut({
      ...base,
      key: "Escape",
      state: "loaded",
      interactiveTarget: true,
    }),
    null,
  );
});

test("Escape cancels active work but never clears a settled workspace", () => {
  assert.equal(resolveAppShortcut({ ...base, key: "Escape", state: "converting" }), "cancel");
  for (const state of ["empty", "loaded", "done", "error"]) {
    assert.equal(resolveAppShortcut({ ...base, key: "Escape", state }), null, state);
  }
});

test("recognizes native and ARIA interactive targets through their nearest ancestor", () => {
  let selector = "";
  const target = {
    closest(value) {
      selector = value;
      return value.includes("input") ? this : null;
    },
  };

  assert.equal(isInteractiveKeyboardTarget(target), true);
  assert.match(selector, /button/);
  assert.match(selector, /role='radio'/);
  assert.equal(isTextEntryTarget(target), true);
  assert.equal(isInteractiveKeyboardTarget(null), false);
  assert.equal(isTextEntryTarget(null), false);
});

test("the reveal shortcut is wired to every completed batch output", async () => {
  const source = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");

  assert.match(source, /const results = useAppStore\(\(store\) => store\.results\)/);
  assert.match(source, /outputPathsForResults\(results\)/);
  assert.match(source, /hasOutput: completedOutputPaths\.length > 0/);
  assert.match(source, /revealPathsInFinder\(completedOutputPaths\)/);
  assert.doesNotMatch(source, /revealPathsInFinder\(outputPathsForResult\(result\)\)/);
});
