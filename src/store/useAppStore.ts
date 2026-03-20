import { create } from "zustand";
import type { AppState, FileInfo, ConversionResult, ConversionError } from "../types";

interface AppStore {
  state: AppState;
  file: FileInfo | null;
  outputFormat: string | null;
  progress: number; // 0-100, -1 for indeterminate
  progressStage: string;
  result: ConversionResult | null;
  error: ConversionError | null;
  jobId: string | null;

  setFile: (file: FileInfo) => void;
  setOutputFormat: (format: string) => void;
  startConversion: (jobId: string) => void;
  updateProgress: (percent: number, stage: string) => void;
  setResult: (result: ConversionResult) => void;
  setError: (error: ConversionError) => void;
  reset: () => void;
}

const initialState = {
  state: "empty" as AppState,
  file: null,
  outputFormat: null,
  progress: 0,
  progressStage: "",
  result: null,
  error: null,
  jobId: null,
};

export const useAppStore = create<AppStore>((set) => ({
  ...initialState,

  setFile: (file) =>
    set({
      state: "loaded",
      file,
      outputFormat: null,
      progress: 0,
      progressStage: "",
      result: null,
      error: null,
      jobId: null,
    }),

  setOutputFormat: (format) => set({ outputFormat: format }),

  startConversion: (jobId) =>
    set({
      state: "converting",
      progress: -1,
      progressStage: "Starting...",
      result: null,
      error: null,
      jobId,
    }),

  updateProgress: (percent, stage) => set({ progress: percent, progressStage: stage }),

  setResult: (result) =>
    set({
      state: "done",
      progress: 100,
      progressStage: "Complete",
      result,
    }),

  setError: (error) =>
    set({
      state: "error",
      progress: 0,
      progressStage: "",
      error,
    }),

  reset: () => set({ ...initialState }),
}));
