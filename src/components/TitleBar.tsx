export function TitleBar() {
  return (
    <header
      data-tauri-drag-region
      className="
        h-7 flex items-center justify-center shrink-0
        pl-[76px] pr-3
        border-b border-white/[0.06] light:border-black/[0.06]
      "
    >
      <span
        data-tauri-drag-region
        className="text-[11px] font-medium tracking-wide text-white/30 light:text-black/30"
      >
        ConvertKit
      </span>
    </header>
  );
}
