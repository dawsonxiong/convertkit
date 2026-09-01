import { useCallback, useEffect, useMemo, useState } from "react";
import { useRecentJobActions } from "../hooks/useRecentJobActions";
import { BUILT_IN_FINDER_ACTIONS } from "../lib/finderActions";
import { OPERATIONS } from "../lib/operations";
import { recentJobOutcome, relativeJobTime } from "../lib/recentJobDisplay";
import {
  getFinderQuickActionStatus,
  installFinderQuickAction,
  isTauriRuntime,
  removeFinderQuickAction,
  type FinderQuickActionStatus,
} from "../lib/tauri";
import { useRecentJobs } from "../store/useRecentJobs";
import type { Operation } from "../types";
import { OperationIcon } from "./ToolNav";

interface DashboardWorkspaceProps {
  disabled: boolean;
  onOpen: (operation: Operation) => void;
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
  onOpenActivity,
  onError,
}: DashboardWorkspaceProps) {
  const jobs = useRecentJobs((state) => state.jobs);
  const { reopen, load } = useRecentJobActions(onOpen);
  const [finderStatuses, setFinderStatuses] = useState<FinderQuickActionStatus[]>([]);
  const [finderLoading, setFinderLoading] = useState(isTauriRuntime);
  const [finderBusy, setFinderBusy] = useState(false);

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

  const installedCount = useMemo(
    () => finderStatuses.filter((status) => status.installed).length,
    [finderStatuses],
  );
  const allFinderActionsInstalled = installedCount === BUILT_IN_FINDER_ACTIONS.length;

  const updateFinderActions = async () => {
    if (finderBusy || finderLoading || !isTauriRuntime()) return;
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

  return (
    <section className="queue-scroll h-full min-h-0 overflow-y-auto pr-1">
      <header>
        <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">Dashboard</h1>
        <p className="mt-1 text-sm text-[#a8a8b1]">File tools and recent work.</p>
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
                <span className="min-w-0 text-left">
                  <span className="block truncate text-[12px] font-medium text-[#e5e1e4]">
                    {OPERATIONS[operation].label}
                  </span>
                  <span className="mt-0.5 block text-[10px] text-white/35">
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
                Run common tools from a file's Quick Actions menu.
              </p>
            </div>
            {!finderLoading && isTauriRuntime() && (
              <span className="text-[10px] text-white/35">
                {installedCount}/{BUILT_IN_FINDER_ACTIONS.length}
              </span>
            )}
          </header>

          <div className="my-3 divide-y divide-white/6 border-y border-white/8">
            {BUILT_IN_FINDER_ACTIONS.map((action, index) => (
              <div
                key={action.id}
                className="flex h-9 items-center gap-2 text-[11px] text-white/60"
              >
                <span className="size-3.5 text-white/35">
                  <OperationIcon operation={action.operation} />
                </span>
                <span>{action.label}</span>
                <span
                  className={`ml-auto text-[9px] ${finderStatuses[index]?.installed ? "text-[#b0c6ff]" : "text-white/25"}`}
                >
                  {finderStatuses[index]?.installed ? "Enabled" : "Off"}
                </span>
              </div>
            ))}
          </div>

          <button
            type="button"
            disabled={disabled || finderBusy || finderLoading || !isTauriRuntime()}
            onClick={() => void updateFinderActions()}
            className={`${allFinderActionsInstalled ? "secondary-button" : "primary-button"} w-full`}
          >
            {finderBusy
              ? "Updating…"
              : finderLoading
                ? "Checking…"
                : allFinderActionsInstalled
                  ? "Remove from Finder"
                  : "Enable in Finder"}
          </button>
        </section>
      </div>

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
