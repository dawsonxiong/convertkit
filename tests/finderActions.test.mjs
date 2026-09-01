import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import test from "node:test";

import {
  BUILT_IN_FINDER_ACTIONS,
  operationForBuiltInFinderAction,
} from "../src/lib/finderActions.ts";
import { ACTIVE_OPERATION_IDS } from "../src/lib/operations.ts";

test("defines a small unique set of built-in Finder actions", () => {
  assert.deepEqual(
    BUILT_IN_FINDER_ACTIONS.map((action) => action.id),
    ["builtin-convert", "builtin-optimize", "builtin-remove-metadata", "builtin-inspect"],
  );
  assert.equal(new Set(BUILT_IN_FINDER_ACTIONS.map((action) => action.id)).size, 4);
  for (const action of BUILT_IN_FINDER_ACTIONS) {
    assert.match(action.id, /^builtin-[a-z-]+$/);
    assert.equal(ACTIVE_OPERATION_IDS.includes(action.operation), true);
    assert.equal(operationForBuiltInFinderAction(action.id), action.operation);
  }
  assert.equal(operationForBuiltInFinderAction("recipe-id"), null);
});

test("routes built-in Finder actions before looking up saved recipes", async () => {
  const app = await readFile(new URL("../src/App.tsx", import.meta.url), "utf8");
  const finderBranch = app.indexOf("operationForBuiltInFinderAction(request.recipeId)");
  const recipeBranch = app.indexOf("const recipe = useSavedRecipes");

  assert.ok(finderBranch > -1);
  assert.ok(recipeBranch > finderBranch);
  assert.match(app, /useAppStore\.getState\(\)\.reset\(\)/);
  assert.match(app, /await addPaths\(request\.paths, builtInOperation\)/);
});

test("dashboard exposes explicit Finder setup without installing on mount", async () => {
  const dashboard = await readFile(
    new URL("../src/components/DashboardWorkspace.tsx", import.meta.url),
    "utf8",
  );

  assert.match(dashboard, /Enable in Finder/);
  assert.match(dashboard, /removeFinderQuickAction/);
  assert.match(dashboard, /installFinderQuickAction/);
  const effect = dashboard.match(/useEffect\(\(\) => \{([\s\S]*?)\n  \}, \[onError\]\);/)?.[1] ?? "";
  assert.doesNotMatch(effect, /installFinderQuickAction/);
});
