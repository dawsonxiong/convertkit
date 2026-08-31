export const CANCEL_FALLBACK_MS = 5_000;

interface CancellationTimers {
  setTimeout: (callback: () => void, delay: number) => ReturnType<typeof setTimeout>;
  clearTimeout: (timer: ReturnType<typeof setTimeout>) => void;
}

const DEFAULT_TIMERS: CancellationTimers = {
  setTimeout: (callback, delay) => setTimeout(callback, delay),
  clearTimeout: (timer) => clearTimeout(timer),
};

export async function cancelJobWithFallback(
  jobId: string,
  cancel: (targetJobId: string) => Promise<void>,
  onFallback: (targetJobId: string) => void,
  timers: CancellationTimers = DEFAULT_TIMERS,
) {
  const timer = timers.setTimeout(() => onFallback(jobId), CANCEL_FALLBACK_MS);
  await cancel(jobId);
  timers.clearTimeout(timer);
}
