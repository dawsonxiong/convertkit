import { useEffect, useState } from "react";

interface RejectionToastProps {
  message: string | null;
}

// Keeps the last message mounted after it clears so the exit animation can play.
export function RejectionToast({ message }: RejectionToastProps) {
  const [shown, setShown] = useState<{ message: string; leaving: boolean } | null>(null);

  useEffect(() => {
    if (message) setShown({ message, leaving: false });
    else setShown((current) => (current ? { ...current, leaving: true } : null));
  }, [message]);

  if (!shown) return null;

  return (
    <div className="pointer-events-none absolute inset-x-0 bottom-5 flex justify-center">
      <div
        key={shown.leaving ? undefined : shown.message}
        role={shown.leaving ? undefined : "alert"}
        aria-atomic="true"
        aria-hidden={shown.leaving || undefined}
        onAnimationEnd={() => {
          if (shown.leaving) setShown(null);
        }}
        className={`${shown.leaving ? "toast-exit" : "toast-enter"} border border-red-400/20 bg-[#2a171a] px-3 py-2 text-xs text-red-200 shadow-[0_8px_24px_rgba(0,0,0,0.35)]`}
      >
        {shown.message}
      </div>
    </div>
  );
}
