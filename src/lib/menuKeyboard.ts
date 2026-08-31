export type MenuNavigationKey = "ArrowDown" | "ArrowUp" | "End" | "Home";

export function nextMenuItemIndex(
  key: MenuNavigationKey,
  currentIndex: number,
  count: number,
): number {
  if (count <= 0) return -1;
  if (key === "Home") return 0;
  if (key === "End") return count - 1;
  if (key === "ArrowDown") return (currentIndex + 1 + count) % count;
  return (currentIndex - 1 + count) % count;
}
