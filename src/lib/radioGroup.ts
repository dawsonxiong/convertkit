import type { KeyboardEvent } from "react";

type RadioNavigationKey = "ArrowDown" | "ArrowLeft" | "ArrowRight" | "ArrowUp" | "End" | "Home";

export function nextRadioIndex(
  key: RadioNavigationKey,
  currentIndex: number,
  count: number,
): number {
  if (count <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  if (key === "ArrowRight" || key === "ArrowDown") return (currentIndex + 1 + count) % count;
  return (currentIndex - 1 + count) % count;
}

export function handleRadioGroupKeyDown(event: KeyboardEvent<HTMLElement>) {
  if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
  if (
    event.key !== "ArrowDown" &&
    event.key !== "ArrowLeft" &&
    event.key !== "ArrowRight" &&
    event.key !== "ArrowUp" &&
    event.key !== "End" &&
    event.key !== "Home"
  )
    return;

  const radios = Array.from(
    event.currentTarget.querySelectorAll<HTMLButtonElement>('[role="radio"]:not(:disabled)'),
  );
  if (radios.length === 0) return;

  const current = radios.findIndex((radio) => radio === document.activeElement);
  const next = nextRadioIndex(event.key, current < 0 ? 0 : current, radios.length);
  const radio = radios[next];
  if (!radio) return;

  event.preventDefault();
  radio.focus();
  radio.click();
}
