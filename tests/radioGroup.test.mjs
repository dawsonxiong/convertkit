import assert from "node:assert/strict";
import test from "node:test";

import { nextRadioIndex } from "../src/lib/radioGroup.ts";

test("moves through radio options and wraps at both ends", () => {
  assert.equal(nextRadioIndex("ArrowRight", 0, 3), 1);
  assert.equal(nextRadioIndex("ArrowDown", 2, 3), 0);
  assert.equal(nextRadioIndex("ArrowLeft", 0, 3), 2);
  assert.equal(nextRadioIndex("ArrowUp", 1, 3), 0);
});

test("moves directly to the first or last radio option", () => {
  assert.equal(nextRadioIndex("Home", 2, 4), 0);
  assert.equal(nextRadioIndex("End", 0, 4), 3);
  assert.equal(nextRadioIndex("ArrowRight", 0, 0), -1);
});
