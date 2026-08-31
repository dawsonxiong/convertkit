import { create } from "zustand";
import {
  cancelConversion,
  deleteTranscriptionModel,
  downloadTranscriptionModel,
  getTranscriptionModels,
} from "../lib/tauri";
import type { ModelDownloadProgress, TranscriptionModel, TranscriptionModelStatus } from "../types";

interface TranscriptionModelStore {
  models: TranscriptionModelStatus[];
  loading: boolean;
  downloadingModel: TranscriptionModel | null;
  downloadJobId: string | null;
  progress: ModelDownloadProgress | null;
  error: string | null;
  refresh: () => Promise<void>;
  download: (model: TranscriptionModel) => Promise<void>;
  cancelDownload: () => Promise<void>;
  remove: (model: TranscriptionModel) => Promise<void>;
  updateProgress: (progress: ModelDownloadProgress) => void;
}

function errorMessage(error: unknown): string {
  if (typeof error === "object" && error !== null && "kind" in error) {
    if (error.kind === "Cancelled") return "Download cancelled";
    if ("detail" in error && typeof error.detail === "object" && error.detail !== null) {
      const detail = error.detail as Record<string, unknown>;
      if (typeof detail.message === "string") return detail.message;
    }
  }
  return "The model could not be downloaded";
}

export const useTranscriptionModels = create<TranscriptionModelStore>((set, get) => ({
  models: [],
  loading: false,
  downloadingModel: null,
  downloadJobId: null,
  progress: null,
  error: null,

  refresh: async () => {
    set({ loading: true, error: null });
    try {
      set({ models: await getTranscriptionModels(), loading: false });
    } catch {
      set({ loading: false, error: "Model status is unavailable" });
    }
  },

  download: async (model) => {
    if (get().downloadJobId) return;
    const jobId = crypto.randomUUID();
    set({
      downloadingModel: model,
      downloadJobId: jobId,
      progress: null,
      error: null,
    });
    try {
      const downloaded = await downloadTranscriptionModel(model, jobId);
      set((state) => ({
        models: state.models.map((item) => (item.model === model ? downloaded : item)),
        downloadingModel: null,
        downloadJobId: null,
        progress: null,
      }));
    } catch (error) {
      set({
        downloadingModel: null,
        downloadJobId: null,
        progress: null,
        error: errorMessage(error),
      });
    }
  },

  cancelDownload: async () => {
    const jobId = get().downloadJobId;
    if (!jobId) return;
    try {
      await cancelConversion(jobId);
    } catch {
      set({ error: "The download could not be cancelled" });
    }
  },

  remove: async (model) => {
    if (get().downloadJobId) return;
    set({ loading: true, error: null });
    try {
      set({ models: await deleteTranscriptionModel(model), loading: false });
    } catch {
      set({ loading: false, error: "The model could not be deleted" });
    }
  },

  updateProgress: (progress) => {
    if (progress.model === get().downloadingModel) set({ progress });
  },
}));
