import { CONVERSION_ERROR_MESSAGES } from "../lib/conversionErrorMessages";
import { OPERATIONS } from "../lib/operations";
import { outputPathsForRecentJob, recentJobIssue, relativeJobTime } from "../lib/recentJobDisplay";
import { useRecentJobActions } from "../hooks/useRecentJobActions";
import { useRecentJobs } from "../store/useRecentJobs";
import type { Operation } from "../types";
import { OpenInFinderButton } from "./OpenInFinderButton";

interface RecentJobsProps {
  disabled: boolean;
  onOpen: (operation: Operation) => void;
  onViewAll: () => void;
  active: boolean;
}

export function RecentJobs({ disabled, onOpen, onViewAll, active }: RecentJobsProps) {
  const jobs = useRecentJobs((state) => state.jobs);
  const clear = useRecentJobs((state) => state.clear);
  const { reopen, load, undo } = useRecentJobActions(onOpen);

  if (jobs.length === 0) return null;

  return (
    <section className="mt-auto pt-5" aria-label="Recent jobs">
      <header className="mb-0.5 flex items-center justify-between px-2">
        <h2 className="text-[11px] font-medium text-white/40">Recent</h2>
        <button
          type="button"
          onClick={clear}
          disabled={disabled}
          className="text-button text-button-large"
        >
          Clear
        </button>
      </header>

      <div className="flex flex-col gap-0.5">
        {jobs.slice(0, 3).map((job) => {
          const first = job.items[0];
          const outputPaths = outputPathsForRecentJob(job);
          const outputCount = outputPaths.length;
          const outcome = recentJobIssue(job).toLocaleLowerCase();
          const errorKind = job.items.find((item) => item.errorKind)?.errorKind;
          const actionLabel = job.setup ? "Load job" : "Open sources";
          const title = errorKind
            ? `${actionLabel}: ${first.inputName} — ${CONVERSION_ERROR_MESSAGES[errorKind]}`
            : `${actionLabel}: ${first.inputName}`;
          return (
            <div key={job.id} className="group flex min-w-0 items-center hover:bg-[#201f22]">
              <button
                type="button"
                onClick={() => void (job.setup ? load(job) : reopen(job))}
                disabled={disabled}
                title={title}
                className="min-w-0 flex-1 px-2 py-1 text-left disabled:opacity-40"
              >
                <span className="block truncate text-[10px] font-medium text-white/60">
                  {job.items.length > 1
                    ? `${first.inputName} +${job.items.length - 1}`
                    : first.inputName}
                </span>
                <span className="block text-[9px] text-white/30">
                  {OPERATIONS[job.operation].label}
                  {outputCount > job.items.length && (
                    <span className="text-white/25">, {outputCount} outputs</span>
                  )}
                  {outcome && (
                    <span className={errorKind ? "text-red-300/55" : "text-white/35"}>
                      , {outcome}
                    </span>
                  )}
                  <span className="pr-1 text-white/15">,</span>
                  {relativeJobTime(job.completedAt)}
                </span>
              </button>
              <div className="mr-1 flex shrink-0 items-center gap-1">
                <OpenInFinderButton
                  paths={outputPaths}
                  disabled={disabled}
                  ariaLabel={`Open ${first.inputName} in Finder`}
                />
                {job.operation === "rename" && job.undoManifest && (
                  <button
                    type="button"
                    onClick={() => void undo(job)}
                    disabled={disabled}
                    className="text-button"
                  >
                    Undo rename
                  </button>
                )}
              </div>
            </div>
          );
        })}
      </div>
      {jobs.length > 0 && (
        <button
          type="button"
          onClick={onViewAll}
          disabled={disabled}
          aria-current={active ? "page" : undefined}
          className={`text-button mt-1 w-full justify-start px-2 text-left ${active ? "text-button-accent" : ""}`}
        >
          View all activity
        </button>
      )}
    </section>
  );
}
