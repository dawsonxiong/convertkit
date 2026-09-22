import { useEffect } from "react";
import { listen } from "@tauri-apps/api/event";
import { confirm } from "@tauri-apps/plugin-dialog";
import { handleRadioGroupKeyDown } from "../lib/radioGroup";
import { TRANSCRIPTION_LANGUAGES, TRANSCRIPTION_MODELS } from "../lib/transcription";
import { useAppStore } from "../store/useAppStore";
import { useTranscriptionModels } from "../store/useTranscriptionModels";
import type { ModelDownloadProgress, TranscriptionOutputFormat } from "../types";

const OUTPUT_FORMATS: Array<{ value: TranscriptionOutputFormat; label: string }> = [
  { value: "txt", label: "Text" },
  { value: "srt", label: "SRT" },
  { value: "vtt", label: "VTT" },
];

const controlClass =
  "select-chevron h-8 min-w-0 border border-[#44464f] bg-[#101012] px-2.5 text-[11px] text-white/75 outline-none hover:border-white/25 focus:border-[#b0c6ff]";

export function TranscriptionPanel() {
  const model = useAppStore((state) => state.transcriptionModel);
  const language = useAppStore((state) => state.transcriptionLanguage);
  const outputFormat = useAppStore((state) => state.transcriptionOutputFormat);
  const setModel = useAppStore((state) => state.setTranscriptionModel);
  const setLanguage = useAppStore((state) => state.setTranscriptionLanguage);
  const setOutputFormat = useAppStore((state) => state.setTranscriptionOutputFormat);
  const models = useTranscriptionModels((state) => state.models);
  const loading = useTranscriptionModels((state) => state.loading);
  const downloadingModel = useTranscriptionModels((state) => state.downloadingModel);
  const progress = useTranscriptionModels((state) => state.progress);
  const error = useTranscriptionModels((state) => state.error);
  const refresh = useTranscriptionModels((state) => state.refresh);
  const download = useTranscriptionModels((state) => state.download);
  const cancelDownload = useTranscriptionModels((state) => state.cancelDownload);
  const remove = useTranscriptionModels((state) => state.remove);
  const updateProgress = useTranscriptionModels((state) => state.updateProgress);
  const selected = models.find((candidate) => candidate.model === model);
  const downloading = downloadingModel === model;
  const downloadBusy = downloadingModel !== null;
  const downloadPercent = Math.min(100, Math.max(0, progress?.percent ?? 0));

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let dispose: (() => void) | undefined;
    void listen<ModelDownloadProgress>("model-download-progress", (event) => {
      updateProgress(event.payload);
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, [updateProgress]);

  const deleteModel = async () => {
    if (!selected?.downloaded) return;
    const approved = await confirm(
      `Delete the ${selected.label} transcription model? You can download it again later.`,
      {
        title: "Delete transcription model",
        kind: "warning",
        okLabel: "Delete",
        cancelLabel: "Cancel",
      },
    );
    if (approved) await remove(model);
  };

  return (
    <section className="border-t border-[#2c2d33] pt-4">
      <h3 className="mb-3 text-[13px] font-semibold text-white/85">Transcription</h3>

      <div className="grid grid-cols-[78px_minmax(0,1fr)] items-center gap-x-3 gap-y-2.5">
        <label htmlFor="transcription-model" className="text-[11px] font-medium text-white/45">
          Model
        </label>
        <div className="flex min-w-0 items-center gap-2">
          <select
            id="transcription-model"
            value={model}
            onChange={(event) => setModel(event.target.value as typeof model)}
            disabled={downloadBusy}
            className={`${controlClass} flex-1 disabled:cursor-not-allowed disabled:opacity-40`}
          >
            {TRANSCRIPTION_MODELS.map((value) => {
              const status = models.find((candidate) => candidate.model === value);
              return (
                <option key={value} value={value}>
                  {status?.label ?? `${value[0].toUpperCase()}${value.slice(1)}`}
                </option>
              );
            })}
          </select>
          {downloading ? (
            <button
              type="button"
              onClick={() => void cancelDownload()}
              className="secondary-button"
            >
              Cancel
            </button>
          ) : selected?.downloaded ? (
            <button
              type="button"
              onClick={() => void deleteModel()}
              disabled={loading || downloadBusy}
              className="danger-button"
            >
              Delete
            </button>
          ) : (
            <button
              type="button"
              onClick={() => void download(model)}
              disabled={loading || downloadBusy || !selected}
              className="primary-button"
            >
              Download
            </button>
          )}
        </div>
        {downloading && (
          <div className="col-start-2 min-w-0">
            <div className="flex items-center justify-between gap-3 text-[10px] text-white/45">
              <span>Downloading locally</span>
              <span className="tabular-nums">{downloadPercent}%</span>
            </div>
            <div
              role="progressbar"
              aria-label={`Downloading ${selected?.label ?? model} transcription model`}
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={downloadPercent}
              aria-valuetext={`${downloadPercent}% downloaded`}
              className="mt-1.5 h-1 bg-white/[0.07]"
            >
              <div className="h-full bg-[#b0c6ff]" style={{ width: `${downloadPercent}%` }} />
            </div>
          </div>
        )}

        <label htmlFor="transcription-language" className="text-[11px] font-medium text-white/45">
          Language
        </label>
        <select
          id="transcription-language"
          value={language}
          onChange={(event) => setLanguage(event.target.value)}
          className={controlClass}
        >
          {TRANSCRIPTION_LANGUAGES.map((option) => (
            <option key={option.code} value={option.code}>
              {option.label}
            </option>
          ))}
        </select>

        <span className="text-[11px] font-medium text-white/45">Format</span>
        <div
          className="grid grid-cols-3 gap-2"
          role="radiogroup"
          aria-label="Transcript format"
          onKeyDown={handleRadioGroupKeyDown}
        >
          {OUTPUT_FORMATS.map((option) => (
            <button
              key={option.value}
              type="button"
              role="radio"
              aria-checked={outputFormat === option.value}
              tabIndex={outputFormat === option.value ? 0 : -1}
              onClick={() => setOutputFormat(option.value)}
              className="choice-button"
            >
              {option.label}
            </button>
          ))}
        </div>
      </div>

      {error && <p className="mt-2 text-[10px] text-red-300/75">{error}</p>}
    </section>
  );
}
