import assert from "node:assert/strict";
import { readdir, readFile } from "node:fs/promises";
import test from "node:test";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const styles = await readFile(new URL("../src/styles/globals.css", import.meta.url), "utf8");

function selectorCount(selector) {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return styles.match(new RegExp(`^${escaped}\\s*\\{`, "gm"))?.length ?? 0;
}

async function componentSources(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  const sources = [];
  for (const entry of entries) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) sources.push(...(await componentSources(path)));
    if (entry.isFile() && entry.name.endsWith(".tsx")) {
      sources.push([path, await readFile(path, "utf8")]);
    }
  }
  return sources;
}

test("shared action buttons use one size and typography contract", () => {
  assert.match(
    styles,
    /\.primary-button,\s*\.secondary-button,\s*\.danger-button\s*\{[^}]*height:\s*32px;[^}]*padding:\s*0 12px;[^}]*font-size:\s*11px;/s,
  );
  assert.match(styles, /\.compact-button\s*\{[^}]*height:\s*28px;[^}]*font-size:\s*10px;/s);
  assert.match(styles, /\.icon-button\s*\{[^}]*width:\s*28px;[^}]*height:\s*28px;/s);
  assert.match(styles, /\.text-button\s*\{[^}]*height:\s*28px;/s);
  assert.match(styles, /button\s*\{[^}]*border-radius:\s*0\s*!important;/s);
});

test("the compact sidebar scale is defined once", () => {
  assert.match(styles, /\.sidebar-search\s*\{[^}]*height:\s*26px;[^}]*font-size:\s*11px;/s);
  assert.match(styles, /\.sidebar-tool-button\s*\{[^}]*height:\s*26px;[^}]*font-size:\s*11px;/s);
  assert.match(styles, /\.sidebar-tool-icon\s*\{[^}]*width:\s*14px;[^}]*height:\s*14px;/s);
  assert.match(styles, /\.sidebar-section-title\s*\{[^}]*font-size:\s*11px;/s);
  assert.match(styles, /\.sidebar-pin-button\s*\{[^}]*width:\s*18px;[^}]*height:\s*18px;/s);
  assert.match(styles, /\.sidebar-tool-label\s*\{[^}]*line-height:\s*14px;/s);

  for (const selector of [
    ".sidebar-search",
    ".sidebar-section-title",
    ".sidebar-tool-button",
    ".sidebar-tool-icon",
    ".sidebar-tool-label",
    ".sidebar-pin-button",
  ]) {
    assert.equal(selectorCount(selector), 1, `${selector} should have one base declaration`);
  }
});

test("the global button reset does not override component font sizes", () => {
  const reset = styles.match(/\nbutton\s*\{([^}]*)\}/s)?.[1] ?? "";
  assert.doesNotMatch(reset, /(?:^|[;\s])font\s*:/);
  assert.match(styles, /button,\s*input,\s*select\s*\{[^}]*font-family:\s*inherit;/s);
});

test("native selects use square WebKit-safe styling", async () => {
  assert.match(styles, /select\s*\{[^}]*appearance:\s*none;[^}]*border-radius:\s*0\s*!important;/s);
  assert.match(
    styles,
    /\.select-chevron\s*\{[^}]*background-image:\s*url\([^}]*background-repeat:\s*no-repeat;/s,
  );

  for (const file of ["TranscriptionPanel.tsx", "OutputSettings.tsx"]) {
    const source = await readFile(new URL(`../src/components/${file}`, import.meta.url), "utf8");
    assert.match(source, /select-chevron/, `${file}: native select needs a consistent chevron`);
  }
});

test("components do not restyle shared button roles locally", async () => {
  const sources = await componentSources(
    fileURLToPath(new URL("../src/components", import.meta.url)),
  );
  const forbiddenActionOverride =
    /(?:primary-button|secondary-button|danger-button)[^"'`]*(?:\bh-(?!full)|\btext-\[|\bborder(?:-|\b)|\bbg-|\brounded)/g;
  const forbiddenChoiceOverride =
    /(?:choice-button|segmented-button)[^"'`]*(?:\bh-(?!full)|\bborder(?:-|\b)|\bbg-|\brounded)/g;
  const forbiddenIconOverride = /icon-button[^"'`]*\bsize-/g;

  for (const [path, source] of sources) {
    assert.doesNotMatch(source, forbiddenActionOverride, path);
    assert.doesNotMatch(source, forbiddenChoiceOverride, path);
    assert.doesNotMatch(source, forbiddenIconOverride, path);
  }
});

test("radio selectors use one shared keyboard and tab-stop contract", async () => {
  const sources = await componentSources(
    fileURLToPath(new URL("../src/components", import.meta.url)),
  );

  for (const [path, source] of sources) {
    const groupCount = source.match(/role="radiogroup"/g)?.length ?? 0;
    const handlerCount = source.match(/onKeyDown=\{handleRadioGroupKeyDown\}/g)?.length ?? 0;
    const radioCount = source.match(/role="radio"/g)?.length ?? 0;
    const radioTabStopCount = source.match(/role="radio"[\s\S]{0,200}?tabIndex=\{/g)?.length ?? 0;

    assert.equal(handlerCount, groupCount, `${path}: every radio group needs arrow-key handling`);
    assert.equal(radioTabStopCount, radioCount, `${path}: every radio needs a roving tab stop`);
  }
});

test("shipping UI excludes rejected visual and marketing motifs", async () => {
  const sources = await componentSources(fileURLToPath(new URL("../src", import.meta.url)));
  const shippingUi = [styles, ...sources.map(([, source]) => source)].join("\n");

  assert.doesNotMatch(shippingUi, /(?:linear|radial|conic)-gradient|bg-gradient/);
  assert.doesNotMatch(shippingUi, /\buppercase\b|\btracking-(?:wide|wider|widest)\b/);
  assert.doesNotMatch(shippingUi, /\brounded-(?:sm|md|lg|xl|2xl|3xl|full)\b/);
  assert.doesNotMatch(shippingUi, /\bshadow-(?:sm|md|lg|xl|2xl)\b/);
  assert.doesNotMatch(shippingUi, /local\s+(?:and|&)\s+private/i);
});

test("dense layouts retain their hierarchy and minimum-width affordances", async () => {
  const [toolNav, output, resize, optimize, transcription, queue, preview, app] = await Promise.all(
    [
      "ToolNav.tsx",
      "OutputSettings.tsx",
      "ResizePanel.tsx",
      "OptimizePanel.tsx",
      "TranscriptionPanel.tsx",
      "WorkspaceQueue.tsx",
      "FilePreview.tsx",
    ]
      .map((file) => readFile(new URL(`../src/components/${file}`, import.meta.url), "utf8"))
      .concat([readFile(new URL("../src/App.tsx", import.meta.url), "utf8")]),
  );

  assert.match(toolNav, /sidebar-scroll queue-scroll/);
  assert.match(toolNav, /tool-section-pinned/);
  assert.match(toolNav, /aria-pressed=\{pinned\}/);
  assert.match(toolNav, /onTogglePin=\{togglePinned\}/);
  assert.match(output, /grid min-w-0 grid-cols-2 gap-1\.5/);
  assert.match(output, /id="recipe-name"[\s\S]*?className="h-8 min-w-0 border/);
  assert.match(output, /className="col-start-2 min-w-0"/);
  assert.match(output, /!editingRecipe && !matchingRecipe &&/);
  assert.match(output, /Update recipe/);
  assert.match(
    resize,
    />Dimensions<[\s\S]*?text-\[13px\] font-semibold|text-\[13px\] font-semibold[^>]*>Dimensions</,
  );
  assert.match(optimize, /text-\[13px\] font-semibold[^>]*>Metadata</);
  assert.match(transcription, /deleteModel\(\)[\s\S]*?className="danger-button"/);
  assert.match(queue, /className="workspace-queue /);
  assert.match(preview, /className="file-preview-row /);
  assert.match(preview, /title=\{file\.name\}/);
  assert.match(styles, /\.workspace-queue\s*\{[^}]*container-type:\s*inline-size;/s);
  assert.match(
    styles,
    /@container workspace-queue \(max-width: 380px\)[\s\S]*?\.file-preview-row\s*\{[^}]*display:\s*grid;[^}]*grid-template-columns:\s*36px minmax\(0, 1fr\);/s,
  );
  assert.match(
    styles,
    /\.file-preview-trailing\s*\{[^}]*grid-column:\s*1 \/ -1;[^}]*flex-wrap:\s*wrap;/s,
  );
  assert.match(styles, /\.file-preview-format-select\s*\{[^}]*width:\s*min\(112px, 100%\);/s);
  assert.doesNotMatch(app, /rounded-lg|shadow-lg/);
});

test("motion stays limited to file intake and progress feedback", async () => {
  const sources = await componentSources(fileURLToPath(new URL("../src", import.meta.url)));
  const motionTokens = sources.flatMap(([path, source]) =>
    [...source.matchAll(/\b(?:animate|transition)-[^\s"'`}]+/g)].map((match) => [path, match[0]]),
  );

  assert.deepEqual(
    motionTokens.map(([, token]) => token).sort(),
    ["animate-indeterminate", "transition-[width]", "transition-colors"].sort(),
  );
});

test("every visible file-intake trigger follows the shared blocked state", async () => {
  const [app, dropZone] = await Promise.all([
    readFile(new URL("../src/App.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/components/DropZone.tsx", import.meta.url), "utf8"),
  ]);

  assert.match(
    app,
    /const intakeBlocked = isInputIntakeBlocked\(state, auxiliaryViewActive, admissionPending\)/,
  );
  assert.match(app, /useFileDrop\(!intakeBlocked\)/);
  assert.match(app, /<DropZone[^>]*disabled=\{intakeBlocked\}/);
  assert.match(app, /if \(intakeBlocked\) \{[\s\S]*?INPUT_INTAKE_BLOCKED_MESSAGE/);
  assert.equal(dropZone.match(/\sdisabled=\{disabled\}/g)?.length, 2);
  assert.equal(dropZone.match(/if \(disabled\) return;/g)?.length, 2);
});

test("completed outputs open in Finder with one compact icon button", async () => {
  const source = await readFile(
    new URL("../src/components/OpenInFinderButton.tsx", import.meta.url),
    "utf8",
  );

  assert.match(source, /revealPathsInFinder\(outputPaths\)/);
  assert.match(source, /Open in Finder/);
  assert.match(source, /className=\{`icon-button \$\{className\}`\.trim\(\)\}/);
  assert.match(source, /<circle cx="11" cy="11" r="6" \/>/);
  assert.match(source, /event\.currentTarget\.blur\(\)/);
  assert.match(source, /onMouseDown=\{\(event\) => event\.preventDefault\(\)\}/);
  assert.doesNotMatch(source, /createPortal|aria-haspopup|role="menu"|Use in/);
  assert.doesNotMatch(source, /rounded|shadow|animate-|transition-/);
});

test("the compact recent preview links to an untruncated activity workspace", async () => {
  const [recent, history] = await Promise.all([
    readFile(new URL("../src/components/RecentJobs.tsx", import.meta.url), "utf8"),
    readFile(new URL("../src/components/HistoryWorkspace.tsx", import.meta.url), "utf8"),
  ]);

  assert.match(recent, /jobs\.slice\(0, 3\)/);
  assert.match(recent, /View all activity/);
  assert.match(history, /jobs\.map\(\(job\)/);
  assert.doesNotMatch(history, /jobs\.slice/);
  assert.match(history, /Remove from history/);
  assert.doesNotMatch(history, /rounded|shadow|animate-|transition-/);
});

test("completed batches expose one compact Finder button in the queue header", async () => {
  const source = await readFile(
    new URL("../src/components/WorkspaceQueue.tsx", import.meta.url),
    "utf8",
  );

  assert.match(
    source,
    /const batchOutputPaths = useMemo\(\(\) => outputPathsForResults\(results\)/,
  );
  assert.match(source, /state === "done" && results\.length > 1 && batchOutputPaths\.length > 0/);
  assert.match(source, /paths=\{batchOutputPaths\}/);
  assert.match(source, /ariaLabel="Open completed batch in Finder"/);
});
