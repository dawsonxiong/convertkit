import { create } from "zustand";
import type {
  ChecksumManifestCreationResult,
  ChecksumManifestVerificationResult,
  FileHashes,
  TechnicalMetadata,
} from "../types";

interface ActiveHashJob {
  path: string;
  jobId: string;
  progress: number;
}

export interface InspectManifestTask {
  action: "create" | "verify";
  jobId: string;
  progress: number;
}

export type InspectManifestView =
  | { kind: "created"; result: ChecksumManifestCreationResult }
  | { kind: "verified"; result: ChecksumManifestVerificationResult };

interface InspectStore {
  hashes: Record<string, FileHashes>;
  errors: Record<string, string>;
  technicalMetadata: Record<string, TechnicalMetadata>;
  technicalLoading: Record<string, boolean>;
  activeJob: ActiveHashJob | null;
  manifestTask: InspectManifestTask | null;
  manifestView: InspectManifestView | null;
  manifestError: string | null;
  start: (path: string, jobId: string) => void;
  updateProgress: (jobId: string, progress: number) => void;
  complete: (path: string, hashes: FileHashes) => void;
  fail: (path: string, message: string) => void;
  cancel: (path: string) => void;
  startTechnical: (path: string) => void;
  completeTechnical: (path: string, metadata: TechnicalMetadata) => void;
  failTechnical: (path: string) => void;
  startManifest: (action: InspectManifestTask["action"], jobId: string) => void;
  updateManifestProgress: (jobId: string, progress: number) => void;
  completeManifest: (view: InspectManifestView) => void;
  failManifest: (message: string) => void;
  cancelManifest: () => void;
  dismissManifest: () => void;
}

export const useInspectStore = create<InspectStore>((set) => ({
  hashes: {},
  errors: {},
  technicalMetadata: {},
  technicalLoading: {},
  activeJob: null,
  manifestTask: null,
  manifestView: null,
  manifestError: null,
  start: (path, jobId) =>
    set((state) => ({
      activeJob: { path, jobId, progress: 0 },
      errors: { ...state.errors, [path]: "" },
    })),
  updateProgress: (jobId, progress) =>
    set((state) =>
      state.activeJob?.jobId === jobId ? { activeJob: { ...state.activeJob, progress } } : state,
    ),
  complete: (path, hashes) =>
    set((state) => ({
      hashes: { ...state.hashes, [path]: hashes },
      errors: { ...state.errors, [path]: "" },
      activeJob: state.activeJob?.path === path ? null : state.activeJob,
    })),
  fail: (path, message) =>
    set((state) => ({
      errors: { ...state.errors, [path]: message },
      activeJob: state.activeJob?.path === path ? null : state.activeJob,
    })),
  cancel: (path) =>
    set((state) => ({
      activeJob: state.activeJob?.path === path ? null : state.activeJob,
    })),
  startTechnical: (path) =>
    set((state) => ({
      technicalLoading: { ...state.technicalLoading, [path]: true },
    })),
  completeTechnical: (path, metadata) =>
    set((state) => ({
      technicalMetadata: { ...state.technicalMetadata, [path]: metadata },
      technicalLoading: { ...state.technicalLoading, [path]: false },
    })),
  failTechnical: (path) =>
    set((state) => ({
      technicalLoading: { ...state.technicalLoading, [path]: false },
    })),
  startManifest: (action, jobId) =>
    set({
      manifestTask: { action, jobId, progress: 0 },
      manifestView: null,
      manifestError: null,
    }),
  updateManifestProgress: (jobId, progress) =>
    set((state) =>
      state.manifestTask?.jobId === jobId
        ? { manifestTask: { ...state.manifestTask, progress } }
        : state,
    ),
  completeManifest: (view) => set({ manifestTask: null, manifestView: view, manifestError: null }),
  failManifest: (message) =>
    set({ manifestTask: null, manifestView: null, manifestError: message }),
  cancelManifest: () => set({ manifestTask: null }),
  dismissManifest: () => set({ manifestView: null, manifestError: null }),
}));
