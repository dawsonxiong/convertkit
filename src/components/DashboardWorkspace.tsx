import { useCallback, useEffect, useMemo, useState } from "react";
import { useRecentJobActions } from "../hooks/useRecentJobActions";
import { BUILT_IN_FINDER_ACTIONS } from "../lib/finderActions";
import { OPERATIONS } from "../lib/operations";
import { recentJobOutcome, relativeJobTime } from "../lib/recentJobDisplay";
import { deleteSavedRecipe, loadRecipeFinderStatuses, toggleSavedRecipeFinderAction } from "../lib/recipeActions";
import {
  groupedSavedRecipes,
  savedRecipeDestinationLabel,
  type SavedRecipe,
} from "../lib/savedRecipes";
import {
  getFinderQuickActionStatus,
  installFinderQuickAction,
  isTauriRuntime,
  removeFinderQuickAction,
  type FinderQuickActionStatus,
} from "../lib/tauri";
import { useRecentJobs } from "../store/useRecentJobs";
import { useSavedRecipes } from "../store/useSavedRecipes";
import type { Operation } from "../types";
import { OperationIcon } from "./ToolNav";
import { FinderQuickActionButton } from "./FinderQuickActionButton";

interface DashboardWorkspaceProps {
  disabled: boolean;
  onOpen: (operation: Operation) => void;
  onOpenRecipe: (recipe: SavedRecipe) => void;
  onOpenActivity: () => void;
  onError: (message: string) => void;
}

const FEATURED_OPERATIONS = [
  "convert",
  "optimize",
  "compressPdf",
  "createArchive",
  "rename",
  "inspect",
] as const satisfies readonly Operation[];

export function DashboardWorkspace({
  disabled,
  onOpen,
  onOpenRecipe,
  onOpenActivity,
  onError,
}: DashboardWorkspaceProps) {
  const jobs = useRecentJobs((state) => state.jobs);
  const recipes = useSavedRecipes((state) => state.recipes);
  const recipeGroups = useMemo(() => groupedSavedRecipes(recipes), [recipes]);
  const { reopen, load } = useRecentJobActions(onOpen);
  const [finderStatuses, setFinderStatuses] = useState<FinderQuickActionStatus[]>([]);
  const [finderLoading, setFinderLoading] = useState(isTauriRuntime);
  const [finderBusy, setFinderBusy] = useState(false);
  const [finderBusyId, setFinderBusyId] = useState<string | null>(null);
  const [recipeFinderStatuses, setRecipeFinderStatuses] = useState<
    Record<string, FinderQuickActionStatus>
  >({});
  const [finderRecipeBusyId, setFinderRecipeBusyId] = useState<string | null>(null);
  const recipeFinderKey = recipes.map((recipe) => `${recipe.id}:${recipe.name}`).join("|");

  const refreshFinderStatuses = useCallback(async () => {
    if (!isTauriRuntime()) return [];
    const statuses = await Promise.all(
      BUILT_IN_FINDER_ACTIONS.map((action) => getFinderQuickActionStatus(action.id, action.label)),
    );
    setFinderStatuses(statuses);
    return statuses;
  }, []);

  useEffect(() => {
    let active = true;
    if (!isTauriRuntime()) return;
    void Promise.all(
      BUILT_IN_FINDER_ACTIONS.map((action) => getFinderQuickActionStatus(action.id, action.label)),
    )
      .then((statuses) => {
        if (active) setFinderStatuses(statuses);
      })
      .catch(() => {
        if (active) onError("Finder actions could not be checked");
      })
      .finally(() => {
        if (active) setFinderLoading(false);
      });
    return () => {
      active = false;
    };
  }, [onError]);

  useEffect(() => {
    let active = true;
    if (!isTauriRuntime() || recipes.length === 0) {
      setRecipeFinderStatuses({});
      return;
    }
    void loadRecipeFinderStatuses(recipes)
      .then((statuses) => {
        if (active) setRecipeFinderStatuses(statuses);
      })
      .catch(() => {
        if (active) onError("Finder actions could not be checked");
      });
    return () => {
      active = false;
    };
  }, [onError, recipeFinderKey, recipes]);

  const installedCount = useMemo(
    () => finderStatuses.filter((status) => status.installed).length,
    [finderStatuses],
  );
  const allFinderActionsInstalled = installedCount === BUILT_IN_FINDER_ACTIONS.length;

  const toggleFinderAction = async (action: (typeof BUILT_IN_FINDER_ACTIONS)[number]) => {
    if (disabled || finderBusy || finderBusyId || finderLoading || !isTauriRuntime()) return;
    const index = BUILT_IN_FINDER_ACTIONS.findIndex((item) => item.id === action.id);
    const installed = finderStatuses[index]?.installed;
    setFinderBusyId(action.id);
    try {
      if (installed) {
        await removeFinderQuickAction(action.id);
      } else {
        await installFinderQuickAction(action.id, action.label);
      }
      await refreshFinderStatuses();
    } catch (error) {
      console.error("Failed to update Finder Quick Action:", error);
      onError("The Finder Quick Action could not be updated");
    } finally {
      setFinderBusyId(null);
    }
  };

  const updateFinderActions = async () => {
    if (finderBusy || finderBusyId || finderLoading || !isTauriRuntime()) return;
    setFinderBusy(true);
    try {
      if (allFinderActionsInstalled) {
        await Promise.all(
          BUILT_IN_FINDER_ACTIONS.map((action) => removeFinderQuickAction(action.id)),
        );
      } else {
        await Promise.all(
          BUILT_IN_FINDER_ACTIONS.map(async (action, index) => {
            if (finderStatuses[index]?.installed) return finderStatuses[index];
            return installFinderQuickAction(action.id, action.label);
          }),
        );
      }
      await refreshFinderStatuses();
    } catch (error) {
      console.error("Failed to update built-in Finder actions:", error);
      onError("Finder actions could not be updated");
    } finally {
      setFinderBusy(false);
    }
  };

  const toggleRecipeFinder = async (recipe: SavedRecipe) => {
    if (disabled || finderRecipeBusyId) return;
    setFinderRecipeBusyId(recipe.id);
    try {
      const next = await toggleSavedRecipeFinderAction(recipe, recipeFinderStatuses[recipe.id]);
      setRecipeFinderStatuses((current) => ({ ...current, [recipe.id]: next }));
    } catch (error) {
      console.error("Failed to update Finder Quick Action:", error);
      onError("The Finder Quick Action could not be updated");
    } finally {
      setFinderRecipeBusyId(null);
    }
  };

  return (
    <section className="queue-scroll h-full min-h-0 overflow-y-auto pr-1">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">Dashboard</h1>
      </header>

      <div className="mt-5 grid grid-cols-[minmax(0,3fr)_minmax(260px,2fr)] gap-4">
        <section aria-labelledby="dashboard-tools-title">
          <div className="mb-2.5 flex items-center justify-between">
            <h2 id="dashboard-tools-title" className="text-[13px] font-semibold text-white/80">
              Popular tools
            </h2>
          </div>
          <div className="grid grid-cols-2 gap-2">
            {FEATURED_OPERATIONS.map((operation) => (
              <button
                key={operation}
                type="button"
                disabled={disabled}
                onClick={() => onOpen(operation)}
                className="dashboard-action group"
              >
                <span className="dashboard-action-icon">
                  <OperationIcon operation={operation} />
                </span>
                <span className="flex min-w-0 flex-col gap-1 text-left">
                  <span className="truncate text-[12px]/none font-medium text-[#e5e1e4]">
                    {OPERATIONS[operation].label}
                  </span>
                  <span className="text-[10px]/none text-white/35">
                    {operation === "compressPdf"
                      ? "PDF"
                      : operation === "createArchive"
                        ? "Files and folders"
                        : operation === "optimize"
                          ? "Images"
                          : "Files"}
                  </span>
                </span>
                <span
                  aria-hidden="true"
                  className="ml-auto text-sm text-white/20 group-hover:text-white/50"
                >
                  →
                </span>
              </button>
            ))}
          </div>
        </section>

        <section className="dashboard-panel" aria-labelledby="finder-actions-title">
          <header className="flex items-start justify-between gap-3">
            <div>
              <h2 id="finder-actions-title" className="text-[13px] font-semibold text-white/80">
                Finder actions
              </h2>
              <p className="mt-1 text-[10px] leading-4 text-white/40">
                Check the tools to add to a file's Quick Actions menu.
              </p>
            </div>
            {!finderLoading && isTauriRuntime() && (
              <button
                type="button"
                disabled={disabled || finderBusy || finderBusyId !== null}
                onClick={() => void updateFinderActions()}
                className="text-button text-button-large"
              >
                {finderBusy
                  ? "Updating…"
                  : allFinderActionsInstalled
                    ? "Disable all"
                    : "Enable all"}
              </button>
            )}
          </header>

          <div className="mt-3 divide-y divide-white/6 border-y border-white/8">
            {BUILT_IN_FINDER_ACTIONS.map((action, index) => {
              const installed = finderStatuses[index]?.installed ?? false;
              return (
                <button
                  key={action.id}
                  type="button"
                  disabled={
                    disabled ||
                    finderBusy ||
                    finderBusyId === action.id ||
                    finderLoading ||
                    !isTauriRuntime()
                  }
                  onClick={() => void toggleFinderAction(action)}
                  aria-pressed={installed}
                  title={
                    installed
                      ? `${action.label} is in Finder. Click to remove.`
                      : `Add ${action.label} to Finder.`
                  }
                  className="flex h-9 w-full items-center gap-2.5 text-left text-[11px] text-white/70 hover:bg-white/3 disabled:opacity-40"
                >
                  <span
                    className={`grid size-4 shrink-0 place-items-center border ${
                      installed
                        ? "border-[#9bb6ff] bg-[#9bb6ff] text-[#0b1d43]"
                        : "border-[#555761] text-transparent"
                    }`}
                    aria-hidden="true"
                  >
                    <svg
                      className="size-2.5"
                      viewBox="0 0 24 24"
                      fill="none"
                      stroke="currentColor"
                      strokeWidth="2.5"
                    >
                      <path d="m5 12 4 4 10-10" />
                    </svg>
                  </span>
                  <span className="size-3.5 text-white/35">
                    <OperationIcon operation={action.operation} />
                  </span>
                  <span className="min-w-0 flex-1 truncate">{action.label}</span>
                </button>
              );
            })}
          </div>
        </section>
      </div>

      <section className="mt-6" aria-labelledby="dashboard-recipes-title">
        <header className="mb-2.5">
          <h2 id="dashboard-recipes-title" className="text-[13px] font-semibold text-white/80">
            Recipes
          </h2>
          <p className="mt-1 text-[10px] leading-4 text-white/40">
            Saved folder, suffix, and tool settings. Create one from Output on a tool.
          </p>
        </header>

        <div className="dashboard-panel !p-0 overflow-hidden">
          {recipeGroups.length === 0 ? (
            <div className="grid h-24 place-items-center text-[11px] text-white/35">
              No saved recipes
            </div>
          ) : (
            recipeGroups.flatMap((group) =>
              group.recipes.map((recipe) => (
                <div
                  key={recipe.id}
                  className="flex min-h-14 items-center gap-3 border-b border-white/6 px-4 py-2.5 last:border-b-0 hover:bg-white/3"
                >
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => onOpenRecipe(recipe)}
                    className="flex min-w-0 flex-1 items-center gap-3 text-left disabled:opacity-40"
                  >
                    <span className="size-4 shrink-0 text-white/35">
                      <OperationIcon operation={recipe.operation} />
                    </span>
                    <span className="min-w-0 flex-1">
                      <span className="block truncate text-[11px] font-medium text-white/75">
                        {recipe.name}
                      </span>
                      <span className="mt-0.5 block truncate text-[9px] text-white/30">
                        {OPERATIONS[recipe.operation].label}
                        <span className="text-white/20"> · </span>
                        {savedRecipeDestinationLabel(recipe)}
                      </span>
                    </span>
                  </button>
                  {isTauriRuntime() && (
                    <FinderQuickActionButton
                      name={recipe.name}
                      status={recipeFinderStatuses[recipe.id]}
                      disabled={disabled || finderRecipeBusyId === recipe.id}
                      onClick={() => void toggleRecipeFinder(recipe)}
                    />
                  )}
                  <button
                    type="button"
                    disabled={disabled}
                    onClick={() => void deleteSavedRecipe(recipe.id)}
                    className="icon-button"
                    aria-label={`Delete ${recipe.name} recipe`}
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
                </div>
              )),
            )
          )}
        </div>
      </section>

      <section className="mt-6" aria-labelledby="dashboard-recent-title">
        <header className="mb-2.5 flex items-center justify-between">
          <h2 id="dashboard-recent-title" className="text-[13px] font-semibold text-white/80">
            Recent activity
          </h2>
          {jobs.length > 0 && (
            <button
              type="button"
              onClick={onOpenActivity}
              className="text-button text-button-large"
            >
              View all
            </button>
          )}
        </header>

        <div className="dashboard-panel !p-0 overflow-hidden">
          {jobs.length === 0 ? (
            <div className="grid h-24 place-items-center text-[11px] text-white/35">
              No recent jobs
            </div>
          ) : (
            jobs.slice(0, 4).map((job) => {
              const first = job.items[0];
              return (
                <button
                  key={job.id}
                  type="button"
                  disabled={disabled}
                  onClick={() => void (job.setup ? load(job) : reopen(job))}
                  className="flex min-h-14 w-full items-center gap-3 border-b border-white/6 px-4 py-2.5 text-left last:border-b-0 hover:bg-white/3 disabled:opacity-40"
                >
                  <span className="size-4 shrink-0 text-white/35">
                    <OperationIcon operation={job.operation} />
                  </span>
                  <span className="min-w-0 flex-1">
                    <span className="block truncate text-[11px] font-medium text-white/75">
                      {job.items.length > 1
                        ? `${first.inputName} +${job.items.length - 1}`
                        : first.inputName}
                    </span>
                    <span className="mt-0.5 block text-[9px] text-white/30">
                      {OPERATIONS[job.operation].label}
                    </span>
                  </span>
                  <span className="text-[10px] text-white/30">{recentJobOutcome(job)}</span>
                  <time className="w-12 text-right text-[9px] text-white/25">
                    {relativeJobTime(job.completedAt)}
                  </time>
                </button>
              );
            })
          )}
        </div>
      </section>
    </section>
  );
}
