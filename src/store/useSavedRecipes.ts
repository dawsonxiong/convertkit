import { create } from "zustand";
import type { Operation } from "../types";
import {
  MAX_SAVED_RECIPES,
  parseSavedRecipes,
  retiredSavedRecipeIds,
  serializeSavedRecipes,
  updatedSavedRecipe,
  type SavedRecipe,
} from "../lib/savedRecipes.ts";

const SAVED_RECIPES_KEY = "convertkit.savedRecipes.v2";
const LEGACY_OUTPUT_PRESETS_KEY = "convertkit.outputPresets.v1";

function saveRecipes(recipes: SavedRecipe[]) {
  try {
    window.localStorage.setItem(SAVED_RECIPES_KEY, serializeSavedRecipes(recipes));
  } catch {
    // Recipes are optional and must not block file processing.
  }
}

interface LoadedRecipes {
  recipes: SavedRecipe[];
  retiredRecipeIds: string[];
}

function loadRecipes(): LoadedRecipes {
  try {
    const current = window.localStorage.getItem(SAVED_RECIPES_KEY);
    if (current) {
      return {
        recipes: parseSavedRecipes(current),
        retiredRecipeIds: retiredSavedRecipeIds(current),
      };
    }

    const legacy = window.localStorage.getItem(LEGACY_OUTPUT_PRESETS_KEY);
    const recipes = parseSavedRecipes(legacy);
    const retiredRecipeIds = retiredSavedRecipeIds(legacy);
    if (recipes.length > 0 && retiredRecipeIds.length === 0) saveRecipes(recipes);
    return { recipes, retiredRecipeIds };
  } catch {
    return { recipes: [], retiredRecipeIds: [] };
  }
}

interface SavedRecipeStore {
  recipes: SavedRecipe[];
  retiredRecipeIds: string[];
  addRecipe: (recipe: Omit<SavedRecipe, "id">) => SavedRecipe | null;
  updateRecipe: (
    id: string,
    directory: string | null,
    suffix: string,
    settings?: SavedRecipe["settings"],
  ) => SavedRecipe | null;
  deleteRecipe: (id: string) => void;
  finishRetiredRecipeMigration: () => void;
}

const loaded = loadRecipes();

export const useSavedRecipes = create<SavedRecipeStore>((set, get) => ({
  ...loaded,
  addRecipe: (input) => {
    if (get().recipes.length >= MAX_SAVED_RECIPES) return null;
    const candidate: SavedRecipe = { ...input, id: crypto.randomUUID() };
    const recipes = parseSavedRecipes(serializeSavedRecipes([...get().recipes, candidate]));
    const recipe = recipes.find((item) => item.id === candidate.id) ?? null;
    if (!recipe) return null;
    saveRecipes(recipes);
    set({ recipes });
    return recipe;
  },
  updateRecipe: (id, directory, suffix, settings) => {
    const current = get().recipes.find((recipe) => recipe.id === id);
    if (!current) return null;
    const candidate = updatedSavedRecipe(current, directory, suffix, settings);
    const recipes = parseSavedRecipes(
      serializeSavedRecipes(get().recipes.map((recipe) => (recipe.id === id ? candidate : recipe))),
    );
    const recipe = recipes.find((item) => item.id === id) ?? null;
    if (!recipe) return null;
    saveRecipes(recipes);
    set({ recipes });
    return recipe;
  },
  deleteRecipe: (id) => {
    const recipes = get().recipes.filter((recipe) => recipe.id !== id);
    saveRecipes(recipes);
    set({ recipes });
  },
  finishRetiredRecipeMigration: () => {
    saveRecipes(get().recipes);
    set({ retiredRecipeIds: [] });
  },
}));

export function recipesForOperation(recipes: SavedRecipe[], operation: Operation) {
  return recipes.filter((recipe) => recipe.operation === operation);
}
