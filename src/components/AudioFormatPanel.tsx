import { useEffect } from "react";
import { getAudioTracks } from "../lib/tauri";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { useAppStore } from "../store/useAppStore";
import type { AudioOutputFormat } from "../types";

const FORMATS: Array<{ value: AudioOutputFormat; label: string }> = [
  { value: "mp3", label: "MP3" },
  { value: "m4a", label: "M4A" },
  { value: "wav", label: "WAV" },
  { value: "flac", label: "FLAC" },
];

export function AudioFormatPanel() {
  const files = useAppStore((state) => state.files);
  const tracks = useAppStore((state) => state.audioTracks);
  const trackErrors = useAppStore((state) => state.audioTrackErrors);
  const format = useAppStore((state) => state.audioOutputFormat);
  const setTracks = useAppStore((state) => state.setAudioTracks);
  const setFormat = useAppStore((state) => state.setAudioOutputFormat);

  useEffect(() => {
    let active = true;
    const state = useAppStore.getState();
    const pending = files.filter(
      (file) =>
        state.audioTracks[file.path] === undefined &&
        state.audioTrackErrors[file.path] === undefined,
    );
    for (const file of pending) {
      getAudioTracks(file.path)
        .then((result) => {
          if (active) setTracks(file.path, result);
        })
        .catch((error: unknown) => {
          const missingDependency =
            typeof error === "object" &&
            error !== null &&
            "kind" in error &&
            (error as { kind?: unknown }).kind === "MissingDependency";
          if (active) {
            setTracks(
              file.path,
              [],
              missingDependency
                ? "FFmpeg is required to inspect tracks"
                : "Audio tracks could not be inspected",
            );
          }
        });
    }
    return () => {
      active = false;
    };
  }, [files, setTracks]);

  const inspected = files.filter((file) => tracks[file.path] !== undefined).length;
  const withoutAudio = files.filter(
    (file) => tracks[file.path]?.length === 0 && !trackErrors[file.path],
  ).length;

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Audio format</h3>
      <div
        className="grid grid-cols-4 gap-2"
        role="radiogroup"
        aria-label="Audio format"
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
        <p className="mt-2 text-[10px] text-white/35">Reading audio tracks…</p>
      )}
      {withoutAudio > 0 && inspected === files.length && (
        <p className="mt-2 text-[10px] text-amber-200/70">
          {withoutAudio} {withoutAudio === 1 ? "file has" : "files have"} no audio track
        </p>
      )}
    </section>
  );
}
