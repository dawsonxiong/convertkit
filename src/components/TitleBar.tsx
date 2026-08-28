import { useCallback } from "react";
import { getCurrentWindow } from "@tauri-apps/api/window";

export function TitleBar() {
  const handleMouseDown = useCallback(async (e: React.MouseEvent) => {
    if (e.button === 0) {
      await getCurrentWindow().startDragging();
    }
  }, []);

  return (
    <header
      onMouseDown={handleMouseDown}
      className="
        h-11 flex items-center justify-center shrink-0
        pl-[76px] pr-3 cursor-default
        border-b border-white/[0.06]
      "
    >
      <span className="pointer-events-none text-xs text-white/40">ConvertKit</span>
    </header>
  );
}
