import { create } from "zustand";
import type {
  AppState,
  FileInfo,
  ConversionResult,
  ConversionError,
  Operation,
  QueueItemState,
  QueueItemStatus,
} from "../types";
import { getCompatibleFormats } from "../lib/formats";

export const MAX_QUEUE_ITEMS = 100;

const createQueueItem = (): QueueItemState => ({
  status: "pending",
  progress: 0,
  stage: "Waiting",
  result: null,
  error: null,
});

interface OperationSession {
  state: AppState;
  files: FileInfo[];
  file: FileInfo | null;
  outputFormats: Record<string, string>;
  outputFormat: string | null;
  resizeWidth: number | null;
  resizeHeight: number | null;
  preserveAspect: boolean;
  keepMetadata: boolean;
  queueItems: Record<string, QueueItemState>;
  activePath: string | null;
  cancelBatchRequested: boolean;
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
  keepMetadata: boolean;
  queueItems: Record<string, QueueItemState>;
  activePath: string | null;
  cancelBatchRequested: boolean;
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
  setKeepMetadata: (keep: boolean) => void;
  startBatch: (paths: string[]) => void;
  startQueueItem: (path: string, jobId: string) => void;
  completeQueueItem: (path: string, result: ConversionResult) => void;
  failQueueItem: (path: string, error: ConversionError, status?: QueueItemStatus) => void;
  skipQueueItem: (path: string) => void;
  requestBatchCancel: () => void;
  cancelRemainingItems: (paths: string[]) => void;
  finishBatch: () => void;
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
  keepMetadata: false,
  queueItems: {},
  activePath: null,
  cancelBatchRequested: false,
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
  keepMetadata: state.keepMetadata,
  queueItems: state.queueItems,
  activePath: state.activePath,
  cancelBatchRequested: state.cancelBatchRequested,
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
    optimize: createEmptySession(),
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
      queueItems: { [file.path]: createQueueItem() },
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
      const available = Math.max(0, MAX_QUEUE_ITEMS - state.files.length);
      const unique = incoming.filter((file) => !knownPaths.has(file.path));
      const additions = unique.slice(0, available);
      if (additions.length === 0) {
        return unique.length > 0
          ? {
              rejectionMessage: `Queue limit reached. ${unique.length} file(s) skipped.`,
            }
          : state;
      }
      const files = [...state.files, ...additions];
      const file = files[0] ?? null;
      const outputFormats = { ...state.outputFormats };

      for (const item of additions) {
        outputFormats[item.path] = getCompatibleFormats(item.format)[0] ?? "";
      }

      const queueItems = Object.fromEntries(files.map((item) => [item.path, createQueueItem()]));

      return {
        state: files.length > 0 ? "loaded" : "empty",
        files,
        file,
        outputFormats,
        queueItems,
        outputFormat: file ? (outputFormats[file.path] ?? null) : null,
        progress: 0,
        progressStage: "",
        result: null,
        results: [],
        error: null,
        jobId: null,
        activePath: null,
        cancelBatchRequested: false,
        rejectionMessage:
          unique.length > additions.length
            ? `Queue limit reached. ${unique.length - additions.length} file(s) skipped.`
            : state.rejectionMessage,
      };
    }),

  removeFile: (path) =>
    set((state) => {
      const files = state.files.filter((file) => file.path !== path);
      const file = files[0] ?? null;
      const outputFormats = { ...state.outputFormats };
      const queueItems = { ...state.queueItems };
      delete outputFormats[path];
      delete queueItems[path];

      if (!file) return createEmptySession();

      return {
        files,
        file,
        outputFormats,
        queueItems,
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
  setKeepMetadata: (keepMetadata) => set({ keepMetadata }),

  startBatch: (paths) =>
    set((state) => {
      const queueItems = { ...state.queueItems };
      for (const path of paths) queueItems[path] = createQueueItem();
      return {
        state: "converting",
        queueItems,
        activePath: null,
        cancelBatchRequested: false,
        progress: -1,
        progressStage: "Starting...",
        result: null,
        results: [],
        error: null,
        jobId: null,
      };
    }),

  startQueueItem: (path, jobId) =>
    set((state) => ({
      activePath: path,
      jobId,
      queueItems: {
        ...state.queueItems,
        [path]: {
          ...createQueueItem(),
          status: "running",
          progress: -1,
          stage: "Starting",
        },
      },
    })),

  completeQueueItem: (path, result) =>
    set((state) => ({
      queueItems: {
        ...state.queueItems,
        [path]: {
          status: "completed",
          progress: 100,
          stage: "Complete",
          result,
          error: null,
        },
      },
      activePath: state.activePath === path ? null : state.activePath,
      jobId: state.activePath === path ? null : state.jobId,
    })),

  failQueueItem: (path, error, status = "failed") =>
    set((state) => ({
      queueItems: {
        ...state.queueItems,
        [path]: {
          status,
          progress: 0,
          stage: status === "cancelled" ? "Cancelled" : "Failed",
          result: null,
          error,
        },
      },
      activePath: state.activePath === path ? null : state.activePath,
      jobId: state.activePath === path ? null : state.jobId,
    })),

  skipQueueItem: (path) =>
    set((state) => {
      const item = state.queueItems[path];
      if (!item || item.status !== "pending") return state;
      return {
        queueItems: {
          ...state.queueItems,
          [path]: { ...item, status: "skipped", stage: "Skipped" },
        },
      };
    }),

  requestBatchCancel: () => set({ cancelBatchRequested: true }),

  cancelRemainingItems: (paths) =>
    set((state) => {
      const queueItems = { ...state.queueItems };
      for (const path of paths) {
        const item = queueItems[path];
        if (item?.status === "pending") {
          queueItems[path] = {
            ...item,
            status: "cancelled",
            stage: "Cancelled",
            error: { kind: "Cancelled", detail: {} },
          };
        }
      }
      return { queueItems };
    }),

  finishBatch: () =>
    set((state) => {
      const results = state.files
        .map((file) => state.queueItems[file.path]?.result)
        .filter((result): result is ConversionResult => Boolean(result));
      return {
        state: "done",
        progress: 100,
        progressStage: "Complete",
        result: results[results.length - 1] ?? null,
        results,
        activePath: null,
        jobId: null,
        cancelBatchRequested: false,
      };
    }),

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

  updateProgress: (percent, stage) =>
    set((state) => {
      if (!state.activePath) return { progress: percent, progressStage: stage };
      const item = state.queueItems[state.activePath];
      return {
        progress: percent,
        progressStage: stage,
        queueItems: {
          ...state.queueItems,
          [state.activePath]: {
            ...item,
            progress: percent,
            stage,
          },
        },
      };
    }),

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
