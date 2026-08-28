export type FileCategory = "image" | "video" | "audio" | "document" | "vector";

export interface FileInfo {
  path: string;
  name: string;
  extension: string;
  size: number;
  category: FileCategory;
  format: string;
  width: number | null;
  height: number | null;
}

export interface ConversionResult {
  output_path: string;
  output_size: number;
  duration_ms: number;
}

export interface ProgressPayload {
  percent: number;
  stage: string;
}

export interface ConversionError {
  kind:
    | "MissingDependency"
    | "UnsupportedConversion"
    | "InputNotFound"
    | "ProcessFailed"
    | "Cancelled"
    | "Timeout"
    | "OutputMissing"
    | "DiskFull";
  detail: Record<string, string>;
}

export interface DependencyStatus {
  name: string;
  installed: boolean;
  version: string | null;
  required: boolean;
}

export type AppState = "empty" | "loaded" | "converting" | "done" | "error";

export type Operation = "convert" | "resize";
