import {
  getFinderQuickActionStatus,
  installFinderQuickAction,
  removeFinderQuickAction,
  type FinderQuickActionStatus,
} from "./tauri.ts";
import { useAppStore } from "../store/useAppStore.ts";
import { useSavedRecipes } from "../store/useSavedRecipes.ts";
import type { SavedRecipe } from "./savedRecipes.ts";

export function applySavedRecipe(recipe: SavedRecipe) {
  const store = useAppStore.getState();
  if (recipe.operation !== store.operation) store.setOperation(recipe.operation);
  const target = useAppStore.getState();
  target.setOutputDirectory(recipe.directory);
  target.setOutputSuffix(recipe.operation, recipe.suffix);
  target.applyRecipeSettings(recipe.settings);
}

export async function deleteSavedRecipe(id: string) {
  try {
    await removeFinderQuickAction(id);
  } catch (error) {
    console.error("Failed to remove Finder Quick Action:", error);
  }
  useSavedRecipes.getState().deleteRecipe(id);
}

export async function toggleSavedRecipeFinderAction(
  recipe: SavedRecipe,
  status: FinderQuickActionStatus | null | undefined,
): Promise<FinderQuickActionStatus> {
  if (status?.installed) {
    await removeFinderQuickAction(recipe.id);
    return { ...status, installed: false };
  }
  return installFinderQuickAction(recipe.id, recipe.name);
}

export async function loadRecipeFinderStatuses(
  recipes: readonly SavedRecipe[],
): Promise<Record<string, FinderQuickActionStatus>> {
  const entries = await Promise.all(
    recipes.map(async (recipe) => {
      try {
        return [recipe.id, await getFinderQuickActionStatus(recipe.id, recipe.name)] as const;
      } catch {
        return [recipe.id, null] as const;
      }
    }),
  );
  return Object.fromEntries(
    entries.filter((entry): entry is readonly [string, FinderQuickActionStatus] => entry[1] !== null),
  );
}
