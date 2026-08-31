import { useEffect } from "react";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { getSubtitleTracks } from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import type { SubtitleOutputFormat } from "../types";

const FORMATS: Array<{ value: SubtitleOutputFormat; label: string }> = [
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "WebVTT" },
];

function probeErrorMessage(error: unknown) {
  if (typeof error === "object" && error !== null && "kind" in error) {
    const candidate = error as { kind?: unknown };
    if (candidate.kind === "MissingDependency") return "FFmpeg is required to inspect tracks";
  }
  return "Subtitle tracks could not be inspected";
}

export function SubtitlePanel() {
  const files = useAppStore((state) => state.files);
  const tracks = useAppStore((state) => state.subtitleTracks);
  const format = useAppStore((state) => state.subtitleOutputFormat);
  const setTracks = useAppStore((state) => state.setSubtitleTracks);
  const setFormat = useAppStore((state) => state.setSubtitleOutputFormat);

  useEffect(() => {
    let active = true;
    const state = useAppStore.getState();
    const pending = files.filter(
      (file) =>
        state.subtitleTracks[file.path] === undefined &&
        state.subtitleTrackErrors[file.path] === undefined,
    );
    for (const file of pending) {
      getSubtitleTracks(file.path)
        .then((result) => {
          if (active) setTracks(file.path, result);
        })
        .catch((error: unknown) => {
          if (active) setTracks(file.path, [], probeErrorMessage(error));
        });
    }
    return () => {
      active = false;
    };
  }, [files, setTracks]);

  const inspected = files.filter((file) => tracks[file.path] !== undefined).length;
  const withoutText = files.filter(
    (file) => tracks[file.path] && !tracks[file.path].some((track) => track.supported),
  ).length;

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Subtitle output</h3>
      <div
        className="grid grid-cols-2 gap-2"
        role="radiogroup"
        aria-label="Subtitle format"
        onKeyDown={handleRadioGroupKeyDown}
      >
        {FORMATS.map((option) => {
          const active = option.value === format;
          return (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={active}
              tabIndex={active ? 0 : -1}
              onClick={() => setFormat(option.value)}
              className="choice-button"
            >
              {option.label}
            </button>
          );
        })}
      </div>
      {inspected < files.length && (
        <p className="mt-2 text-[10px] text-white/35">Reading subtitle tracks…</p>
      )}
      {withoutText > 0 && inspected === files.length && (
        <p className="mt-2 text-[10px] text-amber-200/70">
          {withoutText} {withoutText === 1 ? "file has" : "files have"} no exportable text track
        </p>
      )}
    </section>
  );
}
