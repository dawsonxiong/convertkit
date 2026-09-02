import { useCallback, useEffect, useMemo, useState } from "react";
import { listen } from "@tauri-apps/api/event";
import { downloadDir, join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useAddPaths } from "../hooks/useAddPaths";
import {
  checksumAlgorithmLabel,
  checksumManifestInputs,
  checksumManifestSummary,
  checksumMatches,
  parseExpectedChecksum,
  type ExpectedChecksum,
} from "../lib/checksum";
import { formatFileSize } from "../lib/fileUtils";
import { OPERATIONS } from "../lib/operations";
import {
  cancelConversion,
  computeFileHashes,
  createChecksumManifest,
  exportInspectionReport,
  getTechnicalMetadata,
  isTauriRuntime,
  readFileThumbnail,
  revealInFinder,
  verifyChecksumManifest,
} from "../lib/tauri";
import { useAppStore } from "../store/useAppStore";
import {
  useInspectStore,
  type InspectManifestTask,
  type InspectManifestView,
} from "../store/useInspectStore";
import type { FileHashes, FileInfo, InspectionProgress, TechnicalMetadata } from "../types";

const EMPTY_TECHNICAL_METADATA: TechnicalMetadata = {
  pageCount: null,
  pageSize: null,
  container: null,
  durationSeconds: null,
  bitRate: null,
  videoCodec: null,
  audioCodec: null,
  width: null,
  height: null,
  frameRate: null,
  audioSampleRate: null,
  audioChannels: null,
  audioTracks: [],
  subtitleTracks: [],
};

function formatDate(value: number | null) {
  if (value === null) return "Unavailable";
  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(value));
}

function fileKind(file: FileInfo) {
  if (file.mimeType) return file.mimeType;
  if (file.extension) return `${file.extension.toUpperCase()} file`;
  return "File";
}

function formatDuration(seconds: number) {
  const rounded = Math.max(0, Math.round(seconds));
  const hours = Math.floor(rounded / 3600);
  const minutes = Math.floor((rounded % 3600) / 60);
  const remaining = rounded % 60;
  return hours > 0
    ? `${hours}:${minutes.toString().padStart(2, "0")}:${remaining.toString().padStart(2, "0")}`
    : `${minutes}:${remaining.toString().padStart(2, "0")}`;
}

function formatCodec(value: string) {
  const labels: Record<string, string> = {
    h264: "H.264",
    hevc: "H.265 / HEVC",
    av1: "AV1",
    vp9: "VP9",
    prores: "ProRes",
    mpeg4: "MPEG-4 Part 2",
    aac: "AAC",
    mp3: "MP3",
    flac: "FLAC",
    opus: "Opus",
    pcm_s16le: "PCM 16-bit",
    ass: "ASS",
    mov_text: "Timed text",
    ssa: "SSA",
    srt: "SRT",
    subrip: "SRT",
    webvtt: "WebVTT",
  };
  return labels[value.toLowerCase()] ?? value.toUpperCase();
}

function formatBitRate(bitsPerSecond: number) {
  return bitsPerSecond >= 1_000_000
    ? `${(bitsPerSecond / 1_000_000).toFixed(2).replace(/\.00$/, "")} Mbps`
    : `${Math.round(bitsPerSecond / 1000).toLocaleString()} kbps`;
}

function technicalRows(metadata?: TechnicalMetadata): string[][] {
  if (!metadata) return [];
  return [
    metadata.pageCount ? ["Pages", metadata.pageCount.toLocaleString()] : null,
    metadata.pageSize ? ["Page size", metadata.pageSize] : null,
    metadata.container ? ["Container", metadata.container.toUpperCase()] : null,
    metadata.durationSeconds !== null
      ? ["Duration", formatDuration(metadata.durationSeconds)]
      : null,
    metadata.videoCodec ? ["Video codec", formatCodec(metadata.videoCodec)] : null,
    metadata.width && metadata.height
      ? [
          "Resolution",
          `${metadata.width.toLocaleString()} × ${metadata.height.toLocaleString()} px`,
        ]
      : null,
    metadata.frameRate !== null
      ? ["Frame rate", `${Number(metadata.frameRate.toFixed(2))} fps`]
      : null,
    metadata.audioCodec ? ["Audio codec", formatCodec(metadata.audioCodec)] : null,
    metadata.audioSampleRate
      ? ["Sample rate", `${(metadata.audioSampleRate / 1000).toFixed(1).replace(/\.0$/, "")} kHz`]
      : null,
    metadata.audioChannels
      ? [
          "Channels",
          metadata.audioChannels === 1
            ? "Mono"
            : metadata.audioChannels === 2
              ? "Stereo"
              : `${metadata.audioChannels}`,
        ]
      : null,
    ...metadata.audioTracks.map((track, index) => [
      `Audio ${index + 1}`,
      [
        track.language?.toUpperCase(),
        track.title,
        formatCodec(track.codec),
        track.channelLayout?.replaceAll("_", " ") ??
          (track.channels === 1
            ? "Mono"
            : track.channels === 2
              ? "Stereo"
              : track.channels
                ? `${track.channels} channels`
                : null),
        track.sampleRate ? `${(track.sampleRate / 1000).toFixed(1).replace(/\.0$/, "")} kHz` : null,
        track.isDefault ? "Default" : null,
      ]
        .filter(Boolean)
        .join(", "),
    ]),
    ...metadata.subtitleTracks.map((track, index) => [
      `Subtitle ${index + 1}`,
      [
        track.language?.toUpperCase(),
        track.title,
        formatCodec(track.codec),
        track.isForced ? "Forced" : null,
        track.supported ? null : "Image-based",
      ]
        .filter(Boolean)
        .join(", "),
    ]),
    metadata.bitRate ? ["Bit rate", formatBitRate(metadata.bitRate)] : null,
  ].filter((row): row is string[] => row !== null);
}

function metadataText(file: FileInfo, hashes?: FileHashes, technical?: TechnicalMetadata) {
  const extra = technicalRows(technical).map(([label, value]) => `${label}: ${value}`);
  const lines = [
    `Name: ${file.name}`,
    `Type: ${fileKind(file)}`,
    `Size: ${formatFileSize(file.size)} (${file.size.toLocaleString()} bytes)`,
    file.width && file.height ? `Dimensions: ${file.width} × ${file.height} px` : null,
    ...extra,
    `Created: ${formatDate(file.createdAt)}`,
    `Modified: ${formatDate(file.modifiedAt)}`,
    `Read only: ${file.readOnly ? "Yes" : "No"}`,
    `Path: ${file.path}`,
    hashes ? `MD5: ${hashes.md5}` : null,
    hashes ? `SHA-1: ${hashes.sha1}` : null,
    hashes ? `SHA-256: ${hashes.sha256}` : null,
  ];
  return lines.filter(Boolean).join("\n");
}

function InspectorThumbnail({ file }: { file: FileInfo }) {
  const [thumbnail, setThumbnail] = useState<string | null>(null);

  useEffect(() => {
    let active = true;
    setThumbnail(null);
    readFileThumbnail(file.path)
      .then((value) => {
        if (active) setThumbnail(value);
      })
      .catch(() => undefined);
    return () => {
      active = false;
    };
  }, [file.path]);

  return (
    <div className="grid size-16 shrink-0 place-items-center overflow-hidden bg-[#202024]">
      {thumbnail ? (
        <img src={thumbnail} alt="" className="size-full object-cover" />
      ) : (
        <svg
          className="size-7 text-white/25"
          viewBox="0 0 24 24"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          aria-hidden="true"
        >
          <path d="M6 3h8l4 4v14H6zM14 3v5h4" />
        </svg>
      )}
    </div>
  );
}

function CopyButton({ value, label }: { value: string; label: string }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1200);
    } catch {
      useAppStore.getState().setRejection("Could not copy to the clipboard");
    }
  };

  return (
    <button
      type="button"
      onClick={copy}
      className="text-button shrink-0 px-2"
      aria-label={`Copy ${label}`}
    >
      {copied ? "Copied" : "Copy"}
    </button>
  );
}

function manifestErrorMessage(error: unknown) {
  if (typeof error === "object" && error !== null && "detail" in error) {
    const detail = error.detail;
    if (typeof detail === "object" && detail !== null && "message" in detail) {
      const message = detail.message;
      if (typeof message === "string" && message.trim()) return message;
    }
  }
  return "The checksum manifest could not be processed";
}

function isCancelledError(error: unknown) {
  return (
    typeof error === "object" && error !== null && "kind" in error && error.kind === "Cancelled"
  );
}

function ManifestResultView({ view, onClose }: { view: InspectManifestView; onClose: () => void }) {
  if (view.kind === "created") {
    const { outputPath, entryCount } = view.result;
    return (
      <div className="grid h-full min-h-[360px] place-items-center p-8 text-center">
        <div className="max-w-md">
          <svg
            className="mx-auto size-10 text-emerald-300"
            viewBox="0 0 24 24"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            aria-hidden="true"
          >
            <path d="m5 12 4 4L19 6" />
          </svg>
          <h2 className="mt-4 text-xl font-semibold text-[#e5e1e4]">SHA-256 manifest created</h2>
          <p className="mt-2 text-sm text-white/45">
            {entryCount} {entryCount === 1 ? "file" : "files"} recorded in{" "}
            <span className="text-white/70">{outputPath.split("/").pop()}</span>
          </p>
          <div className="mt-5 flex justify-center gap-2">
            <button
              type="button"
              className="primary-button"
              onClick={() => void revealInFinder(outputPath)}
            >
              Reveal in Finder
            </button>
            <button type="button" className="secondary-button" onClick={onClose}>
              Done
            </button>
          </div>
        </div>
      </div>
    );
  }

  const { result } = view;
  const counts = checksumManifestSummary(result.entries);
  const label = result.algorithm === "sha256" ? "SHA-256" : result.algorithm.toUpperCase();
  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="flex shrink-0 items-center justify-between border-b border-[#3b3d46] bg-[#1b1b1d] p-4">
        <div className="min-w-0">
          <h2 className="text-lg font-semibold text-white/90">Manifest verification</h2>
          <p className="mt-1 truncate text-[11px] text-white/40">
            {label}: {counts.match} matched, {counts.mismatch} mismatched, {counts.missing} missing
          </p>
        </div>
        <div className="flex gap-2">
          <button
            type="button"
            className="secondary-button"
            onClick={() => void revealInFinder(result.manifestPath)}
          >
            Reveal
          </button>
          <button type="button" className="text-button" onClick={onClose}>
            Close
          </button>
        </div>
      </header>
      <div className="queue-scroll min-h-0 flex-1 overflow-y-auto p-3">
        <div className="space-y-1">
          {result.entries.map((entry) => (
            <div
              key={`${entry.relativePath}:${entry.expectedDigest}`}
              className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-4 bg-[#0e0e10] px-3 py-2.5"
            >
              <div className="min-w-0">
                <p className="truncate text-[12px] font-medium text-white/80">
                  {entry.relativePath}
                </p>
                <p className="mt-0.5 truncate font-mono text-[9px] text-white/30">
                  {entry.actualDigest ?? entry.expectedDigest}
                </p>
              </div>
              <span
                className={`text-[10px] font-semibold ${
                  entry.status === "match"
                    ? "text-emerald-300"
                    : entry.status === "missing"
                      ? "text-amber-300"
                      : "text-red-300"
                }`}
              >
                {entry.status === "match"
                  ? "Matched"
                  : entry.status === "missing"
                    ? "Missing"
                    : "Mismatch"}
              </span>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function ManifestProgressView({
  task,
  onCancel,
}: {
  task: InspectManifestTask;
  onCancel: () => void;
}) {
  return (
    <div className="grid h-full min-h-[360px] place-items-center p-8 text-center">
      <div className="w-full max-w-xs">
        <h2 className="text-xl font-semibold text-[#e5e1e4]">
          {task.action === "create" ? "Creating SHA-256 manifest" : "Verifying manifest"}
        </h2>
        <p className="mt-2 text-sm text-white/45">{task.progress}% complete</p>
        <div className="mt-4 h-1 bg-white/10">
          <div className="h-full bg-[#b0c6ff]" style={{ width: `${task.progress}%` }} />
        </div>
        <button type="button" className="secondary-button mt-5" onClick={onCancel}>
          Cancel
        </button>
      </div>
    </div>
  );
}

export function InspectorWorkspace({ isDragging }: { isDragging: boolean }) {
  const [exportState, setExportState] = useState<"idle" | "working" | "done">("idle");
  const [reportFormat, setReportFormat] = useState<"json" | "csv">("json");
  const [expectedChecksum, setExpectedChecksum] = useState("");
  const [verification, setVerification] = useState<{
    path: string;
    expected: ExpectedChecksum;
    matches: boolean;
  } | null>(null);
  const files = useAppStore((state) => state.files);
  const selected = useAppStore((state) => state.file);
  const selectFile = useAppStore((state) => state.selectFile);
  const removeFile = useAppStore((state) => state.removeFile);
  const reset = useAppStore((state) => state.reset);
  const addPaths = useAddPaths();
  const hashes = useInspectStore((state) => state.hashes);
  const errors = useInspectStore((state) => state.errors);
  const activeJob = useInspectStore((state) => state.activeJob);
  const technicalMetadata = useInspectStore((state) => state.technicalMetadata);
  const technicalLoading = useInspectStore((state) => state.technicalLoading);
  const manifestTask = useInspectStore((state) => state.manifestTask);
  const manifestView = useInspectStore((state) => state.manifestView);
  const manifestError = useInspectStore((state) => state.manifestError);
  const selectedHashes = selected ? hashes[selected.path] : undefined;
  const selectedError = selected ? errors[selected.path] : undefined;
  const selectedTechnical = selected ? technicalMetadata[selected.path] : undefined;
  const isTechnicalLoading = selected ? Boolean(technicalLoading[selected.path]) : false;
  const parsedExpectedChecksum = useMemo(
    () => parseExpectedChecksum(expectedChecksum),
    [expectedChecksum],
  );

  useEffect(() => {
    setExpectedChecksum("");
    setVerification(null);
  }, [selected?.path]);

  useEffect(() => {
    if (!isTauriRuntime()) return;
    let dispose: (() => void) | undefined;
    void listen<InspectionProgress>("inspection-progress", (event) => {
      useInspectStore.getState().updateProgress(event.payload.jobId, event.payload.percent);
      useInspectStore.getState().updateManifestProgress(event.payload.jobId, event.payload.percent);
    }).then((unlisten) => {
      dispose = unlisten;
    });
    return () => dispose?.();
  }, []);

  useEffect(() => {
    if (
      !isTauriRuntime() ||
      !selected ||
      selected.path in technicalMetadata ||
      technicalLoading[selected.path]
    )
      return;
    const path = selected.path;
    useInspectStore.getState().startTechnical(path);
    getTechnicalMetadata(path)
      .then((metadata) => useInspectStore.getState().completeTechnical(path, metadata))
      .catch(() => useInspectStore.getState().failTechnical(path));
  }, [selected, technicalLoading, technicalMetadata]);

  const browseFiles = useCallback(async () => {
    const value = await open({ multiple: true, title: "Choose files to inspect" });
    if (!value) return;
    await addPaths(Array.isArray(value) ? value : [value], "inspect");
  }, [addPaths]);

  const browseFolder = useCallback(async () => {
    const value = await open({ directory: true, multiple: false, title: "Choose a folder" });
    if (typeof value === "string") await addPaths([value], "inspect");
  }, [addPaths]);

  const computeSelectedHashes = useCallback(async (): Promise<FileHashes | null> => {
    if (!selected || useInspectStore.getState().activeJob) return null;
    const jobId = crypto.randomUUID();
    useInspectStore.getState().start(selected.path, jobId);
    try {
      const result = await computeFileHashes(selected.path, jobId);
      useInspectStore.getState().complete(selected.path, result);
      return result;
    } catch (error) {
      const cancelled =
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        error.kind === "Cancelled";
      if (cancelled) useInspectStore.getState().cancel(selected.path);
      else useInspectStore.getState().fail(selected.path, "Checksums could not be computed");
      return null;
    }
  }, [selected]);

  const startHashes = useCallback(() => {
    setVerification(null);
    void computeSelectedHashes();
  }, [computeSelectedHashes]);

  const verifyExpectedChecksum = useCallback(async () => {
    if (!selected || !parsedExpectedChecksum) return;
    setVerification(null);
    const result = await computeSelectedHashes();
    if (!result) return;
    setVerification({
      path: selected.path,
      expected: parsedExpectedChecksum,
      matches: checksumMatches(parsedExpectedChecksum, result),
    });
  }, [computeSelectedHashes, parsedExpectedChecksum, selected]);

  const cancelHashes = useCallback(async () => {
    if (!activeJob) return;
    await cancelConversion(activeJob.jobId);
  }, [activeJob]);

  const createManifest = useCallback(async () => {
    if (files.length === 0 || manifestTask || activeJob) return;
    const jobId = crypto.randomUUID();
    useInspectStore.getState().startManifest("create", jobId);
    try {
      const result = await createChecksumManifest(checksumManifestInputs(files), jobId);
      useInspectStore.getState().completeManifest({ kind: "created", result });
    } catch (error) {
      if (isCancelledError(error)) useInspectStore.getState().cancelManifest();
      else useInspectStore.getState().failManifest(manifestErrorMessage(error));
    }
  }, [activeJob, files, manifestTask]);

  const verifyManifest = useCallback(async () => {
    if (manifestTask || activeJob) return;
    const manifestPath = await open({
      multiple: false,
      title: "Choose a checksum manifest",
      filters: [{ name: "Checksum manifests", extensions: ["sha256", "sha1", "md5"] }],
    });
    if (typeof manifestPath !== "string") return;
    const jobId = crypto.randomUUID();
    useInspectStore.getState().startManifest("verify", jobId);
    try {
      const result = await verifyChecksumManifest(manifestPath, jobId);
      useInspectStore.getState().completeManifest({ kind: "verified", result });
    } catch (error) {
      if (isCancelledError(error)) useInspectStore.getState().cancelManifest();
      else useInspectStore.getState().failManifest(manifestErrorMessage(error));
    }
  }, [activeJob, manifestTask]);

  const cancelManifest = useCallback(async () => {
    if (manifestTask) await cancelConversion(manifestTask.jobId);
  }, [manifestTask]);

  const clearWorkspace = useCallback(() => {
    useInspectStore.getState().dismissManifest();
    reset();
  }, [reset]);

  const exportReport = useCallback(async () => {
    if (files.length === 0 || exportState === "working") return;
    try {
      const defaultPath = isTauriRuntime()
        ? await join(await downloadDir(), `ConvertKit inspection.${reportFormat}`)
        : `ConvertKit inspection.${reportFormat}`;
      const chosenPath = await save({
        title: "Export inspection report",
        defaultPath,
        filters: [
          {
            name: reportFormat === "json" ? "JSON report" : "CSV report",
            extensions: [reportFormat],
          },
        ],
      });
      if (!chosenPath) return;

      setExportState("working");
      const outputPath = `${chosenPath.replace(/\.(?:json|csv)$/i, "")}.${reportFormat}`;
      const metadata = { ...useInspectStore.getState().technicalMetadata };
      const pending = files.filter((file) => !metadata[file.path]);
      let nextIndex = 0;
      const worker = async () => {
        while (nextIndex < pending.length) {
          const file = pending[nextIndex++];
          try {
            const value = await getTechnicalMetadata(file.path);
            metadata[file.path] = value;
            useInspectStore.getState().completeTechnical(file.path, value);
          } catch {
            metadata[file.path] = EMPTY_TECHNICAL_METADATA;
            useInspectStore.getState().failTechnical(file.path);
          }
        }
      };
      await Promise.all(Array.from({ length: Math.min(3, pending.length) }, () => worker()));

      const currentHashes = useInspectStore.getState().hashes;
      await exportInspectionReport(
        outputPath,
        reportFormat,
        files.map((file) => ({
          file,
          technicalMetadata: metadata[file.path] ?? null,
          checksums: currentHashes[file.path] ?? null,
        })),
      );
      setExportState("done");
      window.setTimeout(() => setExportState("idle"), 1400);
    } catch (error) {
      console.error("Failed to export inspection report:", error);
      setExportState("idle");
      useAppStore.getState().setRejection("The inspection report could not be exported");
    }
  }, [exportState, files, reportFormat]);

  const rows = useMemo(() => {
    if (!selected) return [];
    return [
      ["Type", fileKind(selected)],
      ["Size", `${formatFileSize(selected.size)} (${selected.size.toLocaleString()} bytes)`],
      ...(selected.width && selected.height
        ? [
            [
              "Dimensions",
              `${selected.width.toLocaleString()} × ${selected.height.toLocaleString()} px`,
            ],
          ]
        : []),
      ...technicalRows(selectedTechnical),
      ["Created", formatDate(selected.createdAt)],
      ["Modified", formatDate(selected.modifiedAt)],
      ["Read only", selected.readOnly ? "Yes" : "No"],
      ["Path", selected.path],
    ];
  }, [selected, selectedTechnical]);

  return (
    <div className="flex h-full min-h-0 flex-col">
      <header className="shrink-0">
        <h1 className="text-2xl font-semibold tracking-tight text-[#e5e1e4]">Inspect files</h1>
        <p className="mt-1 text-sm text-[#a8a8b1]">{OPERATIONS.inspect.description}</p>
      </header>

      <div className="mt-5 grid min-h-0 flex-1 grid-cols-[minmax(220px,4fr)_minmax(0,8fr)] gap-4">
        <section className="flex min-h-0 flex-col border border-[#3b3d46] bg-[#131315]">
          <header className="flex h-11 shrink-0 items-center justify-between border-b border-[#3b3d46] bg-[#1b1b1d] px-3">
            <h2 className="text-[13px] font-semibold text-white/85">Files ({files.length})</h2>
            {files.length > 0 && (
              <div className="flex items-center gap-3">
                <span className="relative flex items-center">
                  <select
                    value={reportFormat}
                    onChange={(event) => setReportFormat(event.target.value as "json" | "csv")}
                    disabled={exportState === "working"}
                    className="h-7 cursor-pointer appearance-none bg-transparent pr-3 text-[10px] font-medium text-white/45 outline-none hover:text-white/75 focus-visible:text-[#b0c6ff] disabled:opacity-40"
                    aria-label="Inspection report format"
                  >
                    <option value="json">JSON</option>
                    <option value="csv">CSV</option>
                  </select>
                  <svg
                    className="pointer-events-none absolute right-0 size-2.5 text-white/30"
                    viewBox="0 0 12 12"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    aria-hidden="true"
                  >
                    <path d="m3 4.5 3 3 3-3" />
                  </svg>
                </span>
                <button
                  type="button"
                  onClick={exportReport}
                  disabled={exportState === "working" || Boolean(manifestTask)}
                  className="text-button text-button-accent text-button-large"
                >
                  {exportState === "working"
                    ? "Exporting…"
                    : exportState === "done"
                      ? "Exported"
                      : "Export"}
                </button>
                <button
                  type="button"
                  onClick={clearWorkspace}
                  disabled={Boolean(manifestTask)}
                  className="text-button text-button-large"
                >
                  Clear
                </button>
              </div>
            )}
          </header>

          <div className="queue-scroll min-h-0 flex-1 overflow-y-auto p-2">
            {files.length === 0 ? (
              <div className="grid h-full min-h-40 place-items-center text-sm text-white/40">
                No files added
              </div>
            ) : (
              <div className="flex flex-col gap-1">
                {files.map((file) => {
                  const active = file.path === selected?.path;
                  return (
                    <div
                      key={file.path}
                      className={`group flex min-w-0 items-center ${active ? "bg-[#353437]" : "hover:bg-[#201f22]"}`}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          useInspectStore.getState().dismissManifest();
                          selectFile(file.path);
                        }}
                        className="min-w-0 flex-1 px-2 py-2 text-left"
                      >
                        <p
                          className={`truncate text-[12px] font-medium ${active ? "text-[#b0c6ff]" : "text-white/80"}`}
                        >
                          {file.name}
                        </p>
                        <p className="mt-0.5 truncate text-[10px] text-white/35">
                          {formatFileSize(file.size)}
                        </p>
                      </button>
                      <button
                        type="button"
                        onClick={() => removeFile(file.path)}
                        className="icon-button opacity-0 group-hover:opacity-100 focus:opacity-100"
                        aria-label={`Remove ${file.name}`}
                      >
                        <svg
                          className="size-3"
                          viewBox="0 0 24 24"
                          fill="none"
                          stroke="currentColor"
                          strokeWidth="2"
                        >
                          <path d="m6 6 12 12M18 6 6 18" />
                        </svg>
                      </button>
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          <footer className="grid shrink-0 grid-cols-2 gap-2 border-t border-[#3b3d46] bg-[#1b1b1d] p-2">
            <button
              type="button"
              onClick={() => void createManifest()}
              disabled={files.length === 0 || Boolean(manifestTask) || Boolean(activeJob)}
              className="secondary-button"
            >
              Create SHA-256
            </button>
            <button
              type="button"
              onClick={() => void verifyManifest()}
              disabled={Boolean(manifestTask) || Boolean(activeJob)}
              className="secondary-button"
            >
              Verify manifest
            </button>
            <button
              type="button"
              onClick={browseFiles}
              disabled={Boolean(manifestTask)}
              className="secondary-button flex-1"
            >
              Add files
            </button>
            <button
              type="button"
              onClick={browseFolder}
              disabled={Boolean(manifestTask)}
              className="secondary-button flex-1"
            >
              Add folder
            </button>
          </footer>
        </section>

        <section
          className={`drop-grid min-h-0 overflow-hidden border bg-[#131315] ${isDragging ? "border-[#b0c6ff]" : "border-[#3b3d46]"}`}
        >
          {manifestTask ? (
            <ManifestProgressView task={manifestTask} onCancel={() => void cancelManifest()} />
          ) : manifestError ? (
            <div className="grid h-full min-h-[360px] place-items-center p-8 text-center">
              <div className="max-w-md">
                <h2 className="text-xl font-semibold text-red-200">Manifest not processed</h2>
                <p className="mt-2 text-sm leading-6 text-white/45">{manifestError}</p>
                <button
                  type="button"
                  className="secondary-button mt-5"
                  onClick={() => useInspectStore.getState().dismissManifest()}
                >
                  Close
                </button>
              </div>
            </div>
          ) : manifestView ? (
            <ManifestResultView
              view={manifestView}
              onClose={() => useInspectStore.getState().dismissManifest()}
            />
          ) : !selected ? (
            <div className="grid h-full min-h-[360px] place-items-center p-8 text-center">
              <div>
                <svg
                  className="mx-auto size-10 text-white"
                  viewBox="0 0 24 24"
                  fill="none"
                  stroke="currentColor"
                  strokeWidth="1.35"
                  aria-hidden="true"
                >
                  <path d="M7.5 3.5h6.75L18.5 7.75V20.5h-11z" />
                  <path d="M14 3.75V8h4.25M13 16V9.5m0 0-2.5 2.5m2.5-2.5 2.5 2.5" />
                </svg>
                <p className="mt-5 text-xl font-semibold text-[#e5e1e4]">
                  {isDragging ? "Release to add files or folders" : "Drop files here"}
                </p>
                <p className="mt-2 text-sm text-[#92939d]">
                  Inspect any local file without changing it.
                </p>
                <button type="button" onClick={browseFiles} className="secondary-button mt-5">
                  Browse files
                </button>
              </div>
            </div>
          ) : (
            <div className="queue-scroll h-full overflow-y-auto">
              <header className="flex items-center gap-4 border-b border-[#3b3d46] bg-[#1b1b1d] p-4">
                <InspectorThumbnail file={selected} />
                <div className="min-w-0 flex-1">
                  <p className="truncate text-lg font-semibold text-white/90">{selected.name}</p>
                  <p className="mt-1 truncate text-[11px] text-white/40">{fileKind(selected)}</p>
                </div>
                <button
                  type="button"
                  onClick={() => void revealInFinder(selected.path)}
                  className="secondary-button"
                >
                  Reveal
                </button>
                <CopyButton
                  value={metadataText(selected, selectedHashes, selectedTechnical)}
                  label="all details"
                />
              </header>

              <div className="p-4">
                <div className="flex items-center gap-2">
                  <h2 className="text-[13px] font-semibold text-white/85">File details</h2>
                  {isTechnicalLoading && (
                    <span className="text-[9px] text-white/30">Reading technical details…</span>
                  )}
                </div>
                <dl className="mt-3 grid grid-cols-[110px_minmax(0,1fr)] gap-x-5 gap-y-3 text-[11px]">
                  {rows.map(([label, value]) => (
                    <div key={label} className="contents">
                      <dt className="text-white/35">{label}</dt>
                      <dd
                        className={`min-w-0 text-white/75 ${label === "Path" ? "select-text break-all" : "truncate"}`}
                      >
                        {value}
                      </dd>
                    </div>
                  ))}
                </dl>

                <div className="mt-5 border-t border-[#303139] pt-4">
                  <div className="flex items-center justify-between gap-3">
                    <div>
                      <h2 className="text-[13px] font-semibold text-white/85">Checksums</h2>
                      <p className="mt-0.5 text-[10px] text-white/35">
                        Verify file identity with MD5, SHA-1, and SHA-256.
                      </p>
                    </div>
                    {activeJob?.path === selected.path ? (
                      <button type="button" onClick={cancelHashes} className="secondary-button">
                        Cancel {activeJob.progress}%
                      </button>
                    ) : (
                      <button
                        type="button"
                        onClick={startHashes}
                        disabled={Boolean(activeJob)}
                        className="primary-button"
                      >
                        {selectedHashes ? "Recompute" : "Compute checksums"}
                      </button>
                    )}
                  </div>

                  {selectedError && (
                    <p className="mt-3 text-[10px] text-red-300">{selectedError}</p>
                  )}
                  <div className="mt-4 flex min-w-0 gap-2">
                    <input
                      type="text"
                      value={expectedChecksum}
                      onChange={(event) => {
                        setExpectedChecksum(event.target.value);
                        setVerification(null);
                      }}
                      disabled={Boolean(activeJob)}
                      placeholder="Paste MD5, SHA-1, or SHA-256"
                      aria-label="Expected checksum"
                      spellCheck={false}
                      autoCapitalize="none"
                      autoCorrect="off"
                      className="h-8 min-w-0 flex-1 border border-[#44464f] bg-[#0e0e10] px-2.5 font-mono text-[10px] text-white/75 outline-none placeholder:text-white/25 hover:border-white/15 focus:border-[#b0c6ff] disabled:opacity-40"
                    />
                    <button
                      type="button"
                      onClick={() => void verifyExpectedChecksum()}
                      disabled={!parsedExpectedChecksum || Boolean(activeJob)}
                      className="secondary-button"
                    >
                      Verify
                    </button>
                  </div>
                  {expectedChecksum.trim() && !parsedExpectedChecksum && (
                    <p className="mt-2 text-[10px] text-red-300/75">
                      Enter one valid MD5, SHA-1, or SHA-256 value
                    </p>
                  )}
                  {verification?.path === selected.path && (
                    <p
                      role="status"
                      className={`mt-2 text-[10px] font-medium ${verification.matches ? "text-emerald-300" : "text-red-300"}`}
                    >
                      {checksumAlgorithmLabel(verification.expected.algorithm)}{" "}
                      {verification.matches ? "matches this file" : "does not match this file"}
                    </p>
                  )}
                  {selectedHashes && (
                    <dl className="mt-4 space-y-2">
                      {(["md5", "sha1", "sha256"] as const).map((key) => (
                        <div
                          key={key}
                          className="grid grid-cols-[52px_minmax(0,1fr)_auto] items-center gap-2 bg-[#0e0e10] px-3 py-2"
                        >
                          <dt className="text-[9px] font-semibold text-white/35">
                            {key === "sha1" ? "SHA-1" : key === "sha256" ? "SHA-256" : "MD5"}
                          </dt>
                          <dd
                            className="select-text truncate font-mono text-[10px] text-white/70"
                            title={selectedHashes[key]}
                          >
                            {selectedHashes[key]}
                          </dd>
                          <CopyButton value={selectedHashes[key]} label={key} />
                        </div>
                      ))}
                    </dl>
                  )}
                </div>
              </div>
            </div>
          )}
        </section>
      </div>
    </div>
  );
}
