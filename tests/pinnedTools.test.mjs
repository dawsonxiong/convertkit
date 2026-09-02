import assert from "node:assert/strict";
import test from "node:test";

import {
  MAX_PINNED_TOOLS,
  normalizePinnedTools,
  parsePinnedTools,
  serializePinnedTools,
  togglePinnedTool,
} from "../src/lib/pinnedTools.ts";
import { SIDEBAR_OPERATIONS } from "../src/lib/toolNavigation.ts";

test("round-trips a versioned pinned tool list and drops unknown identifiers", () => {
  assert.deepEqual(
    parsePinnedTools(serializePinnedTools(["convert", "inspect", "convert"])),
    ["convert", "inspect"],
  );
  assert.deepEqual(
    parsePinnedTools(
      JSON.stringify({
        version: 1,
        operations: ["convert", "packageAppIcon", "inspect", "not-a-tool", 12, null],
      }),
    ),
    ["convert", "inspect"],
  );
  assert.deepEqual(parsePinnedTools(JSON.stringify(["resize", "optimize"])), ["resize", "optimize"]);
  assert.deepEqual(parsePinnedTools(JSON.stringify({ version: 2, operations: ["convert"] })), []);
  assert.deepEqual(parsePinnedTools("not-json"), []);
  assert.deepEqual(parsePinnedTools(null), []);
});

test("keeps pin order, uniqueness, and the bounded list", () => {
  const overflow = [...SIDEBAR_OPERATIONS, ...SIDEBAR_OPERATIONS];
  const normalized = normalizePinnedTools(overflow);
  assert.deepEqual(normalized, SIDEBAR_OPERATIONS.slice(0, MAX_PINNED_TOOLS));
  assert.equal(new Set(normalized).size, normalized.length);
  assert.equal(normalized.length, MAX_PINNED_TOOLS);
});

test("toggles a pin without disturbing the remaining order", () => {
  assert.deepEqual(togglePinnedTool(["convert"], "inspect"), ["convert", "inspect"]);
  assert.deepEqual(togglePinnedTool(["convert", "inspect"], "convert"), ["inspect"]);

  const full = SIDEBAR_OPERATIONS.slice(0, MAX_PINNED_TOOLS);
  const extra = SIDEBAR_OPERATIONS[MAX_PINNED_TOOLS];
  assert.ok(extra);
  assert.deepEqual(togglePinnedTool(full, extra), full);
  assert.deepEqual(togglePinnedTool(full, full[0]), full.slice(1));
});
