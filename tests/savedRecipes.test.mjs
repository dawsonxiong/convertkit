import assert from "node:assert/strict";
import test from "node:test";

import {
  captureSavedRecipeSettings,
  groupedSavedRecipes,
  MAX_SAVED_RECIPES,
  parseSavedRecipes,
  retiredSavedRecipeIds,
  SAVED_RECIPE_OPERATIONS,
  savedRecipeDestinationLabel,
  savedRecipeMatches,
  savedRecipeNameExists,
  serializeSavedRecipes,
  updatedSavedRecipe,
} from "../src/lib/savedRecipes.ts";
import { applySavedRecipe } from "../src/lib/recipeActions.ts";
import { useAppStore } from "../src/store/useAppStore.ts";

const legacyRecipe = {
  id: "recipe-1",
  name: "Client exports",
  operation: "convert",
  directory: "/tmp/client",
  suffix: "-final",
};

const videoRecipe = {
  id: "video-recipe",
  name: "Small web video",
  operation: "encodeVideo",
  directory: "/tmp/client",
  suffix: "-web",
  settings: {
    kind: "encodeVideo",
    preset: "web",
    resolution: "hd",
    quality: "smallest",
    compressionGoal: "quality",
    targetSizeBytes: null,
  },
};

const legacyVideoRecipe = {
  ...videoRecipe,
  settings: {
    kind: "encodeVideo",
    preset: "web",
    resolution: "hd",
    quality: "smallest",
  },
};

const ocrRecipe = {
  id: "ocr-recipe",
  name: "Searchable scans",
  operation: "recognizeText",
  directory: "/tmp/client",
  suffix: "-ocr",
  settings: { kind: "recognizeText", outputFormat: "searchablePdf" },
};

const pdfTargetRecipe = {
  id: "pdf-target-recipe",
  name: "Email PDF",
  operation: "compressPdf",
  directory: "/tmp/client",
  suffix: "-email",
  settings: {
    kind: "compressPdf",
    preset: "smallest",
    compressionGoal: "fileSize",
    targetSizeBytes: 5 * 1024 * 1024,
  },
};

test("round-trips a versioned recipe with processing and output settings", () => {
  assert.deepEqual(
    parseSavedRecipes(serializeSavedRecipes([videoRecipe, ocrRecipe, pdfTargetRecipe])),
    [videoRecipe, ocrRecipe, pdfTargetRecipe],
  );
});

test("migrates legacy raw arrays and version-one output presets", () => {
  assert.deepEqual(parseSavedRecipes(JSON.stringify([legacyRecipe])), [legacyRecipe]);
  assert.deepEqual(parseSavedRecipes(JSON.stringify({ version: 1, presets: [legacyRecipe] })), [
    legacyRecipe,
  ]);
  assert.deepEqual(parseSavedRecipes(serializeSavedRecipes([legacyVideoRecipe])), [videoRecipe]);
  assert.deepEqual(
    parseSavedRecipes(
      serializeSavedRecipes([{ ...ocrRecipe, id: "legacy-ocr", settings: undefined }]),
    ),
    [
      {
        ...ocrRecipe,
        id: "legacy-ocr",
        settings: { kind: "recognizeText", outputFormat: "text" },
      },
    ],
  );
  assert.deepEqual(
    parseSavedRecipes(
      serializeSavedRecipes([
        {
          ...pdfTargetRecipe,
          id: "legacy-pdf-compression",
          settings: { kind: "compressPdf", preset: "smallest" },
        },
      ]),
    ),
    [
      {
        ...pdfTargetRecipe,
        id: "legacy-pdf-compression",
        settings: {
          kind: "compressPdf",
          preset: "smallest",
          compressionGoal: "quality",
          targetSizeBytes: null,
        },
      },
    ],
  );
});

test("keeps every active output-producing workspace operation recipe-capable", () => {
  assert.deepEqual(SAVED_RECIPE_OPERATIONS, [
    "convert",
    "resize",
    "optimize",
    "exportImages",
    "encodeVideo",
    "compressAudio",
    "removeAudio",
    "extractSubtitles",
    "generateThumbnails",
    "removeMetadata",
    "extractAudio",
    "transcribe",
    "extractText",
    "recognizeText",
    "mergePdf",
    "splitPdf",
    "exportPdfPages",
    "compressPdf",
    "createArchive",
    "extractArchive",
  ]);
});

test("drops saved recipes for retired operations", () => {
  const retiredRecipes = [
    { ...legacyRecipe, id: "app-icon", name: "App icon", operation: "packageAppIcon" },
    {
      ...legacyRecipe,
      id: "normalize",
      name: "Normalize",
      operation: "normalizeAudio",
      settings: { kind: "normalizeAudio", preset: "broadcast" },
    },
    { ...legacyRecipe, id: "duplicates", name: "Duplicates", operation: "duplicates" },
  ];

  assert.deepEqual(parseSavedRecipes(serializeSavedRecipes([legacyRecipe, ...retiredRecipes])), [
    legacyRecipe,
  ]);
});

test("extracts retired recipe IDs from raw, version-one, and version-two storage", () => {
  const retired = [
    { id: "app-icon", operation: "packageAppIcon" },
    { id: "normalize", operation: "normalizeAudio" },
    { id: "duplicates", operation: "duplicates" },
  ];
  const serializedShapes = [
    JSON.stringify(retired),
    JSON.stringify({ version: 1, presets: retired }),
    JSON.stringify({ version: 2, recipes: retired }),
  ];

  for (const serialized of serializedShapes) {
    assert.deepEqual(retiredSavedRecipeIds(serialized), ["app-icon", "normalize", "duplicates"]);
  }
});

test("returns only bounded unique valid IDs for retired operations", () => {
  const manyRetired = Array.from({ length: MAX_SAVED_RECIPES + 3 }, (_, index) => ({
    id: `retired-${index}`,
    operation: index % 2 === 0 ? "packageAppIcon" : "normalizeAudio",
  }));
  const stored = [
    { id: "keep", operation: "duplicates" },
    { id: "keep", operation: "packageAppIcon" },
    { id: "active", operation: "convert" },
    { id: "unknown", operation: "notAnOperation" },
    { id: "", operation: "duplicates" },
    { id: "   ", operation: "duplicates" },
    { id: "x".repeat(101), operation: "duplicates" },
    { id: 42, operation: "duplicates" },
    null,
    ...manyRetired,
  ];

  assert.deepEqual(retiredSavedRecipeIds(JSON.stringify(stored)), [
    "keep",
    ...manyRetired.slice(0, MAX_SAVED_RECIPES - 1).map((recipe) => recipe.id),
  ]);
});

test("safely ignores invalid JSON and unsupported stored shapes", () => {
  assert.deepEqual(retiredSavedRecipeIds("not-json"), []);
  assert.deepEqual(retiredSavedRecipeIds(null), []);
  assert.deepEqual(retiredSavedRecipeIds(JSON.stringify({ version: 99, recipes: [] })), []);
  assert.deepEqual(retiredSavedRecipeIds(JSON.stringify({ version: 2, recipes: {} })), []);
});

test("drops malformed, duplicate, unsafe, and operation-mismatched recipes", () => {
  const parsed = parseSavedRecipes(
    JSON.stringify({
      version: 2,
      recipes: [
        videoRecipe,
        { ...videoRecipe, id: "duplicate-name", name: "SMALL WEB VIDEO" },
        { ...videoRecipe, name: "Other" },
        { ...videoRecipe, id: "bad-operation", operation: "duplicates" },
        { ...videoRecipe, id: "bad-suffix", name: "Unsafe", suffix: "../escape" },
        {
          ...videoRecipe,
          id: "mismatched-settings",
          name: "Mismatch",
          settings: { kind: "resize", width: 100, height: 100, preserveAspect: true },
        },
        null,
      ],
    }),
  );
  assert.deepEqual(parsed, [videoRecipe]);
});

test("caps saved recipes and safely handles invalid JSON", () => {
  const many = Array.from({ length: MAX_SAVED_RECIPES + 10 }, (_, index) => ({
    ...legacyRecipe,
    id: `recipe-${index}`,
    name: `Recipe ${index}`,
  }));
  assert.equal(parseSavedRecipes(JSON.stringify(many)).length, MAX_SAVED_RECIPES);
  assert.deepEqual(parseSavedRecipes("not-json"), []);
  assert.deepEqual(parseSavedRecipes(JSON.stringify({ version: 99, recipes: many })), []);
});

test("matches names and the complete active recipe exactly", () => {
  assert.equal(savedRecipeNameExists([videoRecipe], "encodeVideo", " small WEB video "), true);
  assert.equal(savedRecipeNameExists([videoRecipe], "resize", "Small web video"), false);
  assert.equal(
    savedRecipeMatches(videoRecipe, "encodeVideo", "/tmp/client", "-web", videoRecipe.settings),
    true,
  );
  assert.equal(
    savedRecipeMatches(videoRecipe, "encodeVideo", "/tmp/client", "-web", {
      ...videoRecipe.settings,
      quality: "high",
    }),
    false,
  );
  assert.equal(
    savedRecipeMatches(legacyRecipe, "convert", "/tmp/client", "-final", {
      kind: "optimize",
      keepMetadata: true,
    }),
    true,
  );
});

test("updates a recipe in place without changing its identity", () => {
  const updated = updatedSavedRecipe(legacyRecipe, "/tmp/out", "-v2");
  assert.equal(updated.id, legacyRecipe.id);
  assert.equal(updated.name, legacyRecipe.name);
  assert.equal(updated.directory, "/tmp/out");
  assert.equal(updated.suffix, "-v2");
  assert.equal("settings" in updated, false);

  const withSettings = updatedSavedRecipe(videoRecipe, "/tmp/web", "-small", {
    ...videoRecipe.settings,
    quality: "high",
  });
  assert.equal(withSettings.id, videoRecipe.id);
  assert.equal(withSettings.settings?.kind, "encodeVideo");
  assert.equal(withSettings.settings?.quality, "high");
  assert.deepEqual(parseSavedRecipes(serializeSavedRecipes([withSettings])), [withSettings]);
});

test("groups recipes by tool order and labels destinations", () => {
  assert.deepEqual(
    groupedSavedRecipes([videoRecipe, legacyRecipe, ocrRecipe]).map((group) => group.operation),
    ["convert", "encodeVideo", "recognizeText"],
  );
  assert.equal(savedRecipeDestinationLabel(legacyRecipe), "client · -final");
  assert.equal(
    savedRecipeDestinationLabel({ ...legacyRecipe, directory: null, suffix: "  " }),
    "Source folder",
  );
});

test("applies a recipe to the matching workspace session", () => {
  applySavedRecipe(videoRecipe);
  const state = useAppStore.getState();
  assert.equal(state.operation, "encodeVideo");
  assert.equal(state.outputDirectory, "/tmp/client");
  assert.equal(state.outputSuffixes.encodeVideo, "-web");
  assert.equal(state.videoEncodingPreset, "web");
  assert.equal(state.videoResolution, "hd");
});

test("captures and applies operation-aware settings without file-specific choices", () => {
  const source = {
    resizeWidth: 1200,
    resizeHeight: 800,
    preserveAspect: true,
    keepMetadata: false,
    imageOptimizationGoal: "quality",
    imageTargetSizeBytes: 500 * 1024,
    imageExportPresets: ["web", "preview"],
    audioOutputFormat: "flac",
    transcriptionModel: "small",
    transcriptionLanguage: "fr",
    transcriptionOutputFormat: "vtt",
    ocrOutputFormat: "searchablePdf",
    videoEncodingPreset: "web",
    videoResolution: "hd",
    videoQuality: "smallest",
    videoCompressionGoal: "quality",
    videoTargetSizeBytes: null,
    loudnessPreset: "standard",
    subtitleOutputFormat: "vtt",
    thumbnailMode: "contactSheet",
    thumbnailOutputFormat: "png",
    pdfSplitMode: "extract",
    pdfPageImageFormat: "jpeg",
    pdfPageImageResolution: "print",
    pdfCompressionPreset: "smallest",
    pdfCompressionGoal: "fileSize",
    pdfTargetSizeBytes: 5 * 1024 * 1024,
    archiveFormat: "sevenZ",
  };

  const settings = captureSavedRecipeSettings("encodeVideo", source);
  assert.deepEqual(settings, videoRecipe.settings);

  useAppStore.getState().applyRecipeSettings(settings);
  const applied = useAppStore.getState();
  assert.equal(applied.videoEncodingPreset, "web");
  assert.equal(applied.videoResolution, "hd");
  assert.equal(applied.videoQuality, "smallest");
  assert.equal(applied.videoCompressionGoal, "quality");

  const targetSizeBytes = 25 * 1024 * 1024;
  const targetSizeSettings = captureSavedRecipeSettings("encodeVideo", {
    ...source,
    videoCompressionGoal: "fileSize",
    videoTargetSizeBytes: targetSizeBytes,
  });
  assert.deepEqual(targetSizeSettings, {
    kind: "encodeVideo",
    preset: "web",
    resolution: "hd",
    quality: "smallest",
    compressionGoal: "fileSize",
    targetSizeBytes,
  });
  useAppStore.getState().applyRecipeSettings(targetSizeSettings);
  assert.equal(useAppStore.getState().videoCompressionGoal, "fileSize");
  assert.equal(useAppStore.getState().videoTargetSizeBytes, targetSizeBytes);

  const imageTargetSettings = captureSavedRecipeSettings("optimize", {
    ...source,
    imageOptimizationGoal: "fileSize",
    imageTargetSizeBytes: 500 * 1024,
  });
  assert.deepEqual(imageTargetSettings, {
    kind: "optimize",
    keepMetadata: false,
    compressionGoal: "fileSize",
    targetSizeBytes: 500 * 1024,
  });
  useAppStore.getState().applyRecipeSettings(imageTargetSettings);
  assert.equal(useAppStore.getState().imageOptimizationGoal, "fileSize");
  assert.equal(useAppStore.getState().imageTargetSizeBytes, 500 * 1024);

  assert.equal(captureSavedRecipeSettings("removeAudio", source), undefined);

  const transcription = captureSavedRecipeSettings("transcribe", source);
  assert.deepEqual(transcription, {
    kind: "transcribe",
    model: "small",
    language: "fr",
    outputFormat: "vtt",
  });
  useAppStore.getState().applyRecipeSettings(transcription);
  assert.equal(useAppStore.getState().transcriptionModel, "small");
  assert.equal(useAppStore.getState().transcriptionLanguage, "fr");
  assert.equal(useAppStore.getState().transcriptionOutputFormat, "vtt");

  const ocr = captureSavedRecipeSettings("recognizeText", source);
  assert.deepEqual(ocr, { kind: "recognizeText", outputFormat: "searchablePdf" });
  useAppStore.getState().applyRecipeSettings(ocr);
  assert.equal(useAppStore.getState().ocrOutputFormat, "searchablePdf");

  const pdfCompression = captureSavedRecipeSettings("compressPdf", source);
  assert.deepEqual(pdfCompression, pdfTargetRecipe.settings);
  useAppStore.getState().applyRecipeSettings(pdfCompression);
  assert.equal(useAppStore.getState().pdfCompressionPreset, "smallest");
  assert.equal(useAppStore.getState().pdfCompressionGoal, "fileSize");
  assert.equal(useAppStore.getState().pdfTargetSizeBytes, 5 * 1024 * 1024);

  const archive = captureSavedRecipeSettings("createArchive", source);
  assert.deepEqual(archive, { kind: "createArchive", format: "sevenZ" });
  useAppStore.getState().applyRecipeSettings(archive);
  assert.equal(useAppStore.getState().archiveFormat, "sevenZ");
});

test("validates every supported operation settings shape", () => {
  const recipes = [
    {
      ...legacyRecipe,
      id: "resize",
      name: "Resize",
      operation: "resize",
      settings: { kind: "resize", width: 1280, height: 720, preserveAspect: true },
    },
    {
      ...legacyRecipe,
      id: "optimize",
      name: "Optimize",
      operation: "optimize",
      settings: {
        kind: "optimize",
        keepMetadata: true,
        compressionGoal: "quality",
        targetSizeBytes: null,
      },
    },
    {
      ...legacyRecipe,
      id: "images",
      name: "Images",
      operation: "exportImages",
      settings: { kind: "exportImages", presets: ["web", "preview"] },
    },
    videoRecipe,
    {
      ...legacyRecipe,
      id: "audio",
      name: "Audio",
      operation: "extractAudio",
      settings: { kind: "extractAudio", outputFormat: "flac" },
    },
    {
      ...legacyRecipe,
      id: "transcribe",
      name: "Transcribe",
      operation: "transcribe",
      settings: { kind: "transcribe", model: "base", language: "auto", outputFormat: "srt" },
    },
    ocrRecipe,
    {
      ...legacyRecipe,
      id: "subtitles",
      name: "Subtitles",
      operation: "extractSubtitles",
      settings: { kind: "extractSubtitles", outputFormat: "vtt" },
    },
    {
      ...legacyRecipe,
      id: "thumbnails",
      name: "Thumbnails",
      operation: "generateThumbnails",
      settings: { kind: "generateThumbnails", mode: "contactSheet", outputFormat: "png" },
    },
    {
      ...legacyRecipe,
      id: "split",
      name: "Split",
      operation: "splitPdf",
      settings: { kind: "splitPdf", mode: "extract" },
    },
    {
      ...legacyRecipe,
      id: "pages",
      name: "Pages",
      operation: "exportPdfPages",
      settings: {
        kind: "exportPdfPages",
        mode: "everyPage",
        outputFormat: "png",
        resolution: "print",
      },
    },
    {
      ...legacyRecipe,
      id: "archive",
      name: "Archive",
      operation: "createArchive",
      settings: { kind: "createArchive", format: "sevenZ" },
    },
    {
      ...legacyRecipe,
      id: "compress",
      name: "Compress",
      operation: "compressPdf",
      settings: {
        kind: "compressPdf",
        preset: "smallest",
        compressionGoal: "quality",
        targetSizeBytes: null,
      },
    },
  ];
  assert.deepEqual(parseSavedRecipes(serializeSavedRecipes(recipes)), recipes);
});

test("rejects invalid settings rather than silently changing a recipe", () => {
  const invalid = [
    {
      ...videoRecipe,
      id: "bad-quality",
      name: "Bad quality",
      settings: { ...videoRecipe.settings, quality: "tiny" },
    },
    {
      ...videoRecipe,
      id: "bad-target",
      name: "Bad target",
      settings: {
        ...videoRecipe.settings,
        compressionGoal: "fileSize",
        targetSizeBytes: 512,
      },
    },
    {
      ...videoRecipe,
      id: "lossless-target",
      name: "Lossless target",
      settings: {
        ...videoRecipe.settings,
        preset: "archive",
        compressionGoal: "fileSize",
        targetSizeBytes: 25 * 1024 * 1024,
      },
    },
    {
      ...legacyRecipe,
      id: "bad-resize",
      name: "Bad resize",
      operation: "resize",
      settings: { kind: "resize", width: 32_769, height: 720, preserveAspect: true },
    },
    {
      ...legacyRecipe,
      id: "duplicate-variants",
      name: "Duplicate variants",
      operation: "exportImages",
      settings: { kind: "exportImages", presets: ["web", "web"] },
    },
    {
      ...ocrRecipe,
      id: "bad-ocr-output",
      name: "Bad OCR output",
      settings: { kind: "recognizeText", outputFormat: "html" },
    },
    {
      ...pdfTargetRecipe,
      id: "bad-pdf-target",
      name: "Bad PDF target",
      settings: { ...pdfTargetRecipe.settings, targetSizeBytes: 512 },
    },
  ];
  assert.deepEqual(parseSavedRecipes(serializeSavedRecipes(invalid)), []);
});
