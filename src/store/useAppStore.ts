import { create } from "zustand";
import type { AppState, FileInfo, ConversionResult, ConversionError, Operation } from "../types";
import { getCompatibleFormats } from "../lib/formats";

interface OperationSession {
  state: AppState;
  files: FileInfo[];
  file: FileInfo | null;
  outputFormats: Record<string, string>;
  outputFormat: string | null;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  progress: number;
  progressStage: string;
  result: ConversionResult | null;
  results: ConversionResult[];
  error: ConversionError | null;
  jobId: string | null;
}

interface AppStore {
  state: AppState;
  operation: Operation;
  sessions: Record<Operation, OperationSession>;
  files: FileInfo[];
  file: FileInfo | null;
  outputFormats: Record<string, string>;
  outputFormat: string | null;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  progress: number; // 0-100, -1 for indeterminate
  progressStage: string;
  result: ConversionResult | null;
  results: ConversionResult[];
  error: ConversionError | null;
  jobId: string | null;
  rejectionMessage: string | null;

  setFile: (file: FileInfo) => void;
  addFiles: (files: FileInfo[]) => void;
  removeFile: (path: string) => void;
  setOperation: (operation: Operation) => void;
  setRejection: (message: string) => void;
  clearRejection: () => void;
  setOutputFormat: (format: string, path?: string) => void;
  setResizeDimensions: (width: number | null, height: number | null) => void;
  setPreserveAspect: (preserve: boolean) => void;
  startConversion: (jobId: string) => void;
  setActiveJob: (jobId: string) => void;
  updateProgress: (percent: number, stage: string) => void;
  setResult: (result: ConversionResult) => void;
  setResults: (results: ConversionResult[]) => void;
  setError: (error: ConversionError) => void;
  reset: () => void;
}

const createEmptySession = (): OperationSession => ({
  state: "empty",
  files: [],
  file: null,
  outputFormats: {},
  outputFormat: null,
  resizeWidth: null,
  resizeHeight: null,
  preserveAspect: true,
  progress: 0,
  progressStage: "",
  result: null,
  results: [],
  error: null,
  jobId: null,
});

const snapshotSession = (state: AppStore): OperationSession => ({
  state: state.state,
  files: state.files,
  file: state.file,
  outputFormats: state.outputFormats,
  outputFormat: state.outputFormat,
  resizeWidth: state.resizeWidth,
  resizeHeight: state.resizeHeight,
  preserveAspect: state.preserveAspect,
  progress: state.progress,
  progressStage: state.progressStage,
  result: state.result,
  results: state.results,
  error: state.error,
  jobId: state.jobId,
});

const initialState = {
  ...createEmptySession(),
  operation: "convert" as Operation,
  sessions: {
    convert: createEmptySession(),
    resize: createEmptySession(),
  },
  rejectionMessage: null,
};

export const useAppStore = create<AppStore>((set) => ({
  ...initialState,

  setFile: (file) =>
    set({
      state: "loaded",
      files: [file],
      file,
      outputFormats: { [file.path]: getCompatibleFormats(file.format)[0] ?? "" },
      outputFormat: getCompatibleFormats(file.format)[0] ?? null,
      resizeWidth: null,
      resizeHeight: null,
      progress: 0,
      progressStage: "",
      result: null,
      results: [],
      error: null,
      jobId: null,
    }),

  addFiles: (incoming) =>
    set((state) => {
      if (incoming.length === 0) return state;

      const knownPaths = new Set(state.files.map((file) => file.path));
      const additions = incoming.filter((file) => !knownPaths.has(file.path));
      const files = [...state.files, ...additions];
      const file = files[0] ?? null;
      const outputFormats = { ...state.outputFormats };

      for (const item of additions) {
        outputFormats[item.path] = getCompatibleFormats(item.format)[0] ?? "";
      }

      return {
        state: files.length > 0 ? "loaded" : "empty",
        files,
        file,
        outputFormats,
        outputFormat: file ? (outputFormats[file.path] ?? null) : null,
        progress: 0,
        progressStage: "",
        result: null,
        results: [],
        error: null,
        jobId: null,
      };
    }),

  removeFile: (path) =>
    set((state) => {
      const files = state.files.filter((file) => file.path !== path);
      const file = files[0] ?? null;
      const outputFormats = { ...state.outputFormats };
      delete outputFormats[path];

      if (!file) return createEmptySession();

      return {
        files,
        file,
        outputFormats,
        outputFormat: outputFormats[file.path] ?? null,
        resizeWidth: state.file?.path === file.path ? state.resizeWidth : null,
        resizeHeight: state.file?.path === file.path ? state.resizeHeight : null,
      };
    }),

  setOperation: (operation) =>
    set((state) => {
      if (operation === state.operation) return state;

      const sessions = {
        ...state.sessions,
        [state.operation]: snapshotSession(state),
      };

      return {
        ...sessions[operation],
        operation,
        sessions,
        rejectionMessage: null,
      };
    }),

  setOutputFormat: (format, path) =>
    set((state) => {
      const targetPath = path ?? state.file?.path;
      if (!targetPath) return state;
      return {
        outputFormats: { ...state.outputFormats, [targetPath]: format },
        outputFormat: targetPath === state.file?.path ? format : state.outputFormat,
      };
    }),
  setResizeDimensions: (resizeWidth, resizeHeight) => set({ resizeWidth, resizeHeight }),
  setPreserveAspect: (preserveAspect) => set({ preserveAspect }),

  startConversion: (jobId) =>
    set({
      state: "converting",
      progress: -1,
      progressStage: "Starting...",
      result: null,
      error: null,
      jobId,
    }),

  setActiveJob: (jobId) => set({ jobId }),

  updateProgress: (percent, stage) => set({ progress: percent, progressStage: stage }),

  setResult: (result) =>
    set({
      state: "done",
      progress: 100,
      progressStage: "Complete",
      result,
      results: [result],
    }),

  setResults: (results) =>
    set({
      state: "done",
      progress: 100,
      progressStage: "Complete",
      result: results[results.length - 1] ?? null,
      results,
      jobId: null,
    }),

  setError: (error) =>
    set({
      state: "error",
      progress: 0,
      progressStage: "",
      error,
    }),

  setRejection: (message) => set({ rejectionMessage: message }),
  clearRejection: () => set({ rejectionMessage: null }),

  reset: () =>
    set((state) => {
      const session = createEmptySession();
      return {
        ...session,
        sessions: { ...state.sessions, [state.operation]: session },
        rejectionMessage: null,
      };
    }),
}));
