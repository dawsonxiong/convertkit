import type { ConversionError } from "../types";

export const CONVERSION_ERROR_MESSAGES: Record<ConversionError["kind"], string> = {
  MissingDependency: "A required tool is not installed.",
  UnsupportedConversion: "This operation is not supported.",
  InputNotFound: "The input file could not be found.",
  ProcessFailed: "The file could not be processed.",
  Cancelled: "The operation was cancelled.",
  Timeout: "The operation timed out.",
  OutputMissing: "The output file was not created.",
  OutputConflict: "Choose a different output folder or filename suffix.",
  ArchivePasswordRequired: "Enter the password for this encrypted archive.",
  IncorrectArchivePassword: "The archive password is incorrect.",
  TargetSizeUnreachable: "The requested file size could not be reached for this file.",
  OutputNotSmaller: "The compressed file would not be smaller than the original.",
  DiskFull: "There is not enough disk space.",
};
