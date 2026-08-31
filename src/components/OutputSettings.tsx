import { open } from "@tauri-apps/plugin-dialog";
import { useEffect, useMemo, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  captureSavedRecipeSettings,
  MAX_SAVED_RECIPE_NAME_LENGTH,
  MAX_SAVED_RECIPES,
  savedRecipeMatches,
  savedRecipeNameExists,
} from "../lib/savedRecipes";
import { useAppStore } from "../store/useAppStore";
import { recipesForOperation, useSavedRecipes } from "../store/useSavedRecipes";
import {
  getFinderQuickActionStatus,
  installFinderQuickAction,
  isTauriRuntime,
  removeFinderQuickAction,
  type FinderQuickActionStatus,
} from "../lib/tauri";

function folderName(path: string) {
  const parts = path.split(/[\\/]/).filter(Boolean);
  return parts[parts.length - 1] ?? path;
}

export function OutputSettings() {
  const operation = useAppStore((state) => state.operation);
  const directory = useAppStore((state) => state.outputDirectory);
  const suffix = useAppStore((state) => state.outputSuffixes[operation]);
  const setOutputDirectory = useAppStore((state) => state.setOutputDirectory);
  const setOutputSuffix = useAppStore((state) => state.setOutputSuffix);
  const setRejection = useAppStore((state) => state.setRejection);
  const applyRecipeSettings = useAppStore((state) => state.applyRecipeSettings);
  const recipeSource = useAppStore(
    useShallow((state) => ({
      resizeWidth: state.resizeWidth,
      resizeHeight: state.resizeHeight,
      preserveAspect: state.preserveAspect,
      keepMetadata: state.keepMetadata,
      imageOptimizationGoal: state.imageOptimizationGoal,
      imageTargetSizeBytes: state.imageTargetSizeBytes,
      imageExportPresets: state.imageExportPresets,
      audioOutputFormat: state.audioOutputFormat,
      transcriptionModel: state.transcriptionModel,
      transcriptionLanguage: state.transcriptionLanguage,
      transcriptionOutputFormat: state.transcriptionOutputFormat,
      ocrOutputFormat: state.ocrOutputFormat,
      videoEncodingPreset: state.videoEncodingPreset,
      videoResolution: state.videoResolution,
      videoQuality: state.videoQuality,
      videoCompressionGoal: state.videoCompressionGoal,
      videoTargetSizeBytes: state.videoTargetSizeBytes,
      audioCompressionPreset: state.audioCompressionPreset,
      subtitleOutputFormat: state.subtitleOutputFormat,
      thumbnailMode: state.thumbnailMode,
      thumbnailOutputFormat: state.thumbnailOutputFormat,
      pdfSplitMode: state.pdfSplitMode,
      pdfPageImageFormat: state.pdfPageImageFormat,
      pdfPageImageResolution: state.pdfPageImageResolution,
      pdfCompressionPreset: state.pdfCompressionPreset,
      pdfCompressionGoal: state.pdfCompressionGoal,
      pdfTargetSizeBytes: state.pdfTargetSizeBytes,
      archiveFormat: state.archiveFormat,
    })),
  );
  const recipes = useSavedRecipes((state) => state.recipes);
  const addRecipe = useSavedRecipes((state) => state.addRecipe);
  const deleteRecipe = useSavedRecipes((state) => state.deleteRecipe);
  const [editingRecipe, setEditingRecipe] = useState(false);
  const [recipeName, setRecipeName] = useState("");
  const [recipeError, setRecipeError] = useState<string | null>(null);
  const [finderStatus, setFinderStatus] = useState<FinderQuickActionStatus | null>(null);
  const [finderBusy, setFinderBusy] = useState(false);
  const currentSettings = useMemo(
    () => captureSavedRecipeSettings(operation, recipeSource),
    [operation, recipeSource],
  );
  const operationRecipes = useMemo(
    () => recipesForOperation(recipes, operation),
    [operation, recipes],
  );
  const matchingRecipe = operationRecipes.find((recipe) =>
    savedRecipeMatches(recipe, operation, directory, suffix, currentSettings),
  );

  const chooseDirectory = async () => {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "Choose output folder",
    });
    if (typeof selected === "string") setOutputDirectory(selected);
  };

  const applyRecipe = (id: string) => {
    const recipe = operationRecipes.find((item) => item.id === id);
    if (!recipe) return;
    setOutputDirectory(recipe.directory);
    setOutputSuffix(operation, recipe.suffix);
    applyRecipeSettings(recipe.settings);
  };

  const closeRecipeEditor = () => {
    setEditingRecipe(false);
    setRecipeName("");
    setRecipeError(null);
  };

  useEffect(() => {
    setEditingRecipe(false);
    setRecipeName("");
    setRecipeError(null);
  }, [operation]);

  useEffect(() => {
    let active = true;
    setFinderStatus(null);
    if (!matchingRecipe || !isTauriRuntime()) return;
    void getFinderQuickActionStatus(matchingRecipe.id, matchingRecipe.name)
      .then((status) => {
        if (active) setFinderStatus(status);
      })
      .catch(() => {
        if (active) setFinderStatus(null);
      });
    return () => {
      active = false;
    };
  }, [matchingRecipe?.id, matchingRecipe?.name]);

  const toggleFinderAction = async () => {
    if (!matchingRecipe || finderBusy) return;
    setFinderBusy(true);
    try {
      if (finderStatus?.installed) {
        await removeFinderQuickAction(matchingRecipe.id);
        setFinderStatus({ ...finderStatus, installed: false });
      } else {
        setFinderStatus(await installFinderQuickAction(matchingRecipe.id, matchingRecipe.name));
      }
    } catch (error) {
      console.error("Failed to update Finder Quick Action:", error);
      setRejection("The Finder Quick Action could not be updated");
    } finally {
      setFinderBusy(false);
    }
  };

  const removeRecipe = async () => {
    if (!matchingRecipe) return;
    try {
      await removeFinderQuickAction(matchingRecipe.id);
    } catch (error) {
      console.error("Failed to remove Finder Quick Action:", error);
    }
    deleteRecipe(matchingRecipe.id);
  };

  const saveRecipe = () => {
    const name = recipeName.trim();
    if (!name) {
      setRecipeError("Enter a recipe name");
      return;
    }
    if (Array.from(name).length > MAX_SAVED_RECIPE_NAME_LENGTH) {
      setRecipeError(`Use ${MAX_SAVED_RECIPE_NAME_LENGTH} characters or fewer`);
      return;
    }
    if (savedRecipeNameExists(recipes, operation, name)) {
      setRecipeError("A recipe with that name already exists");
      return;
    }
    if (recipes.length >= MAX_SAVED_RECIPES) {
      setRecipeError(`Recipe limit reached (${MAX_SAVED_RECIPES})`);
      return;
    }
    const saved = addRecipe({
      name,
      operation,
      directory,
      suffix,
      ...(currentSettings ? { settings: currentSettings } : {}),
    });
    if (!saved) {
      setRecipeError("That recipe could not be saved");
      return;
    }
    closeRecipeEditor();
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <div className="mb-3 flex items-center justify-between">
        <h3 className="text-[13px] font-semibold text-white/85">Output</h3>
        {!editingRecipe && (
          <button
            type="button"
            onClick={() => setEditingRecipe(true)}
            className="text-button text-button-accent text-button-large"
          >
            Save recipe
          </button>
        )}
      </div>

      <div className="grid grid-cols-[78px_minmax(0,1fr)] items-center gap-x-3 gap-y-2.5">
        {operationRecipes.length > 0 && (
          <>
            <label htmlFor="saved-recipe" className="text-[11px] font-medium text-white/45">
              Recipe
            </label>
            <div className="flex min-w-0 items-center gap-1.5">
              <select
                id="saved-recipe"
                value={matchingRecipe?.id ?? ""}
                onChange={(event) => applyRecipe(event.target.value)}
                className="select-chevron h-8 min-w-0 flex-1 border border-[#44464f] bg-[#101012] px-2 text-[11px] text-white/75 outline-none hover:border-white/25 focus:border-[#b0c6ff]"
              >
                <option value="">Custom</option>
                {operationRecipes.map((recipe) => (
                  <option key={recipe.id} value={recipe.id}>
                    {recipe.name}
                  </option>
                ))}
              </select>
              {matchingRecipe && (
                <>
                  {isTauriRuntime() && (
                    <button
                      type="button"
                      onClick={() => void toggleFinderAction()}
                      disabled={finderBusy || finderStatus?.supported === false}
                      className={`icon-button ${finderStatus?.installed ? "text-[#b0c6ff]" : ""}`}
                      aria-label={`${finderStatus?.installed ? "Remove" : "Add"} ${matchingRecipe.name} ${finderStatus?.installed ? "from" : "to"} Finder Quick Actions`}
                      aria-pressed={finderStatus?.installed ?? false}
                      title={
                        finderStatus?.installed
                          ? "Remove from Finder Quick Actions"
                          : "Add to Finder Quick Actions"
                      }
                    >
                      <svg
                        className="size-3.5"
                        viewBox="0 0 24 24"
                        fill="none"
                        stroke="currentColor"
                        strokeWidth="1.7"
                        aria-hidden="true"
                      >
                        <path d="M13 2 5 13h6l-1 9 9-12h-6V2Z" />
                      </svg>
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void removeRecipe()}
                    className="icon-button"
                    aria-label={`Delete ${matchingRecipe.name} recipe`}
                    title="Delete recipe"
                  >
                    <svg
                      className="size-3.5"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="1.7"
                      aria-hidden="true"
                    >
                      <path d="M4 7h16M9 7V4h6v3m-8 0 1 13h8l1-13M10 11v5M14 11v5" />
                    </svg>
                  </button>
                </>
              )}
            </div>
          </>
        )}

        {editingRecipe && (
          <>
            <label htmlFor="recipe-name" className="text-[11px] font-medium text-white/45">
              Name
            </label>
            <div className="min-w-0">
              <div className="grid min-w-0 grid-cols-2 gap-1.5">
                <input
                  id="recipe-name"
                  type="text"
                  value={recipeName}
                  maxLength={MAX_SAVED_RECIPE_NAME_LENGTH}
                  autoFocus
                  spellCheck={false}
                  placeholder="Recipe name"
                  onChange={(event) => {
                    setRecipeName(event.target.value);
                    setRecipeError(null);
                  }}
                  onKeyDown={(event) => {
                    event.stopPropagation();
                    if (event.key === "Enter") saveRecipe();
                    if (event.key === "Escape") closeRecipeEditor();
                  }}
                  className="col-span-2 h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]"
                />
                <button type="button" onClick={saveRecipe} className="primary-button w-full">
                  Save
                </button>
                <button
                  type="button"
                  onClick={closeRecipeEditor}
                  className="secondary-button w-full"
                >
                  Cancel
                </button>
              </div>
              {recipeError && <p className="mt-1.5 text-[11px] text-red-300/80">{recipeError}</p>}
            </div>
          </>
        )}

        <label className="text-[11px] font-medium text-white/45">Folder</label>
        <div className="flex min-w-0 gap-1.5">
          <button
            type="button"
            onClick={chooseDirectory}
            title={directory ?? "Save beside each source file"}
            className="secondary-button button-align-start min-w-0 flex-1 truncate text-left"
          >
            {directory ? folderName(directory) : "Source folder"}
          </button>
          {directory && (
            <button
              type="button"
              onClick={() => setOutputDirectory(null)}
              className="secondary-button min-w-0"
            >
              Reset
            </button>
          )}
        </div>

        <label htmlFor="output-suffix" className="text-[11px] font-medium text-white/45">
          {operation === "exportImages" ? "Name tag" : "Suffix"}
        </label>
        <input
          id="output-suffix"
          type="text"
          value={suffix}
          maxLength={80}
          spellCheck={false}
          placeholder={operation === "exportImages" ? "Optional" : "None"}
          onChange={(event) => setOutputSuffix(operation, event.target.value)}
          className="h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/80 outline-none placeholder:text-white/25 hover:border-white/25 focus:border-[#b0c6ff]"
        />
      </div>
    </section>
  );
}
