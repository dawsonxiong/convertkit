import { CONVERSION_ERROR_MESSAGES } from "../lib/conversionErrorMessages";
import { OPERATIONS } from "../lib/operations";
import {
  outputPathsForRecentJob,
  recentJobIssue,
  recentJobOutcome,
  relativeJobTime,
} from "../lib/recentJobDisplay";
import { useRecentJobActions } from "../hooks/useRecentJobActions";
import { useRecentJobs } from "../store/useRecentJobs";
import type { Operation } from "../types";
import { OpenInFinderButton } from "./OpenInFinderButton";

interface HistoryWorkspaceProps {
  disabled: boolean;
  onOpen: (operation: Operation) => void;
}

export function HistoryWorkspace({ disabled, onOpen }: HistoryWorkspaceProps) {
  const jobs = useRecentJobs((state) => state.jobs);
  const recordingEnabled = useRecentJobs((state) => state.recordingEnabled);
  const setRecordingEnabled = useRecentJobs((state) => state.setRecordingEnabled);
  const removeJob = useRecentJobs((state) => state.removeJob);
  const clear = useRecentJobs((state) => state.clear);
  const { reopen, load, undo } = useRecentJobActions(onOpen);

  return (
    <section className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-start justify-between gap-4">
        <div>
          <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">Activity</h1>
          <p className="mt-1 text-sm text-[#a8a8b1]">Load a previous job or reopen its sources.</p>
        </div>
        <div className="flex items-center gap-3">
          <button
            type="button"
            onClick={() => setRecordingEnabled(!recordingEnabled)}
            disabled={disabled}
            className="text-button text-button-large"
          >
            {recordingEnabled ? "Pause history" : "Resume history"}
          </button>
          {jobs.length > 0 && (
            <button
              type="button"
              onClick={clear}
              disabled={disabled}
              className="text-button text-button-large"
            >
              Clear
            </button>
          )}
        </div>
      </header>

      <div className="mt-5 min-h-0 flex-1">
        <div className="flex max-h-full flex-col border border-[#3b3d46] bg-[#131315]">
          <header className="flex h-11 shrink-0 items-center border-b border-[#3b3d46] bg-[#1b1b1d] px-3">
            <h2 className="text-[13px] font-semibold text-white/85">Recent jobs ({jobs.length})</h2>
          </header>

          {jobs.length === 0 ? (
            <div className="grid min-h-40 place-items-center text-sm text-white/40">
              No recent jobs
            </div>
          ) : (
            <div className="queue-scroll min-h-0 overflow-y-auto">
              {jobs.map((job) => {
                const first = job.items[0];
                const outputPaths = outputPathsForRecentJob(job);
                const issue = recentJobIssue(job);
                const errorKind = job.items.find((item) => item.errorKind)?.errorKind;
                const actionLabel = job.setup ? "Load job" : "Open sources";
                const title = errorKind
                  ? `${actionLabel}: ${first.inputName} — ${CONVERSION_ERROR_MESSAGES[errorKind]}`
                  : `${actionLabel}: ${first.inputName}`;

                return (
                  <article
                    key={job.id}
                    className="group flex min-h-14 items-center border-b border-white/8 last:border-b-0 hover:bg-[#18181b]"
                  >
                    <button
                      type="button"
                      onClick={() => void (job.setup ? load(job) : reopen(job))}
                      disabled={disabled}
                      title={title}
                      className="min-w-0 flex-1 px-4 py-2.5 text-left disabled:opacity-40"
                    >
                      <span className="block truncate text-[13px] font-medium text-[#e5e1e4]">
                        {job.items.length > 1
                          ? `${first.inputName} +${job.items.length - 1}`
                          : first.inputName}
                      </span>
                      <span className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[10px] text-white/35">
                        <span className={job.setup ? "text-[#b0c6ff]" : "text-white/40"}>
                          {actionLabel}
                        </span>
                        <span aria-hidden="true" className="text-white/15">
                          ·
                        </span>
                        <span>{OPERATIONS[job.operation].label}</span>
                        <span aria-hidden="true" className="text-white/15">
                          ·
                        </span>
                        <span className={issue ? "text-red-300/65" : "text-white/40"}>
                          {recentJobOutcome(job)}
                        </span>
                        {outputPaths.length > job.items.length && (
                          <>
                            <span aria-hidden="true" className="text-white/15">
                              ·
                            </span>
                            <span>{outputPaths.length} outputs</span>
                          </>
                        )}
                        <span aria-hidden="true" className="text-white/15">
                          ·
                        </span>
                        <time dateTime={new Date(job.completedAt).toISOString()}>
                          {relativeJobTime(job.completedAt)}
                        </time>
                      </span>
                    </button>

                    <div className="mr-3 flex shrink-0 items-center gap-2">
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
                      <button
                        type="button"
                        onClick={() => removeJob(job.id)}
                        disabled={disabled}
                        className="text-button"
                      >
                        Remove from history
                      </button>
                    </div>
                  </article>
                );
              })}
            </div>
          )}
        </div>
      </div>
    </section>
  );
}
