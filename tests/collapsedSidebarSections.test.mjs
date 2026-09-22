import assert from "node:assert/strict";
import test from "node:test";

import {
  COLLAPSED_SIDEBAR_SECTIONS_KEY,
  parseCollapsedSidebarSections,
  serializeCollapsedSidebarSections,
  SIDEBAR_SECTION_IDS,
  toggleCollapsedSidebarSection,
} from "../src/lib/collapsedSidebarSections.ts";
import { TOOL_SECTIONS } from "../src/lib/toolNavigation.ts";

test("round-trips collapsed sidebar sections and drops unknown identifiers", () => {
  assert.equal(COLLAPSED_SIDEBAR_SECTIONS_KEY, "convertkit.collapsedSidebarSections.v1");
  assert.deepEqual(SIDEBAR_SECTION_IDS, ["pinned", ...TOOL_SECTIONS.map((section) => section.id)]);
  assert.deepEqual(
    parseCollapsedSidebarSections(
      serializeCollapsedSidebarSections(["images", "pinned", "images"]),
    ),
    ["images", "pinned"],
  );
  assert.deepEqual(
    parseCollapsedSidebarSections(
      JSON.stringify({
        version: 1,
        sections: ["video-audio", "not-a-section", "organize", 12, null],
      }),
    ),
    ["video-audio", "organize"],
  );
  assert.deepEqual(parseCollapsedSidebarSections(JSON.stringify(["general", "pdf-documents"])), [
    "general",
    "pdf-documents",
  ]);
  assert.deepEqual(
    parseCollapsedSidebarSections(JSON.stringify({ version: 2, sections: ["images"] })),
    [],
  );
  assert.deepEqual(parseCollapsedSidebarSections("not-json"), []);
  assert.deepEqual(parseCollapsedSidebarSections(null), []);
});

test("toggles a collapsed section without disturbing the remaining order", () => {
  assert.deepEqual(toggleCollapsedSidebarSection([], "images"), ["images"]);
  assert.deepEqual(toggleCollapsedSidebarSection(["images", "organize"], "images"), ["organize"]);
  assert.deepEqual(toggleCollapsedSidebarSection(["images"], "pinned"), ["images", "pinned"]);
});
