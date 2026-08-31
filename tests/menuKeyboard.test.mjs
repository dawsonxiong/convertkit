import assert from "node:assert/strict";
import test from "node:test";

import { nextMenuItemIndex } from "../src/lib/menuKeyboard.ts";

test("moves through menu items and wraps at both ends", () => {
  assert.equal(nextMenuItemIndex("ArrowDown", 0, 3), 1);
  assert.equal(nextMenuItemIndex("ArrowDown", 2, 3), 0);
  assert.equal(nextMenuItemIndex("ArrowUp", 0, 3), 2);
  assert.equal(nextMenuItemIndex("ArrowUp", 2, 3), 1);
});

test("moves directly to menu bounds and handles an empty menu", () => {
  assert.equal(nextMenuItemIndex("Home", 2, 4), 0);
  assert.equal(nextMenuItemIndex("End", 0, 4), 3);
  assert.equal(nextMenuItemIndex("ArrowDown", 0, 0), -1);
});
