import { useAppStore } from "../store/useAppStore";
import { useRecentJobs } from "../store/useRecentJobs";
import { useTranscriptionModels } from "../store/useTranscriptionModels";
import type { FileInfo, Operation, RecentJob } from "../types";
import { BYTES_PER_MEBIBYTE } from "../lib/videoCompression";
import { PDF_BYTES_PER_MEBIBYTE } from "../lib/pdfCompression";

export const VISUAL_FIXTURE_OPERATIONS = new Set<Operation>([
  "convert",
  "resize",
  "optimize",
  "exportImages",
  "encodeVideo",
  "compressAudio",
  "removeAudio",
  "extractSubtitles",
  "generateThumbnails",
  "removeMetadata",
  "extractAudio",
  "transcribe",
  "extractText",
  "recognizeText",
  "mergePdf",
  "splitPdf",
  "exportPdfPages",
  "compressPdf",
  "createArchive",
  "extractArchive",
  "rename",
  "inspect",
]);

function imageFixture(): FileInfo {
  return {
    path: "/visual-fixtures/product-photo.png",
    name: "product-photo.png",
    extension: "png",
    size: 2_486_272,
    category: "image",
    format: "png",
    width: 1600,
    height: 1000,
    mimeType: "image/png",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function jpegFixture(): FileInfo {
  return {
    ...imageFixture(),
    path: "/visual-fixtures/interface-capture.jpg",
    name: "interface-capture.jpg",
    extension: "jpg",
    format: "jpg",
    size: 1_184_512,
    mimeType: "image/jpeg",
  };
}

function videoFixture(): FileInfo {
  return {
    path: "/visual-fixtures/product-demo.mp4",
    name: "product-demo.mp4",
    extension: "mp4",
    size: 148_897_792,
    category: "video",
    format: "mp4",
    width: 1920,
    height: 1080,
    mimeType: "video/mp4",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function audioFixture(): FileInfo {
  return {
    path: "/visual-fixtures/interview.wav",
    name: "interview.wav",
    extension: "wav",
    size: 24_682_496,
    category: "audio",
    format: "wav",
    width: null,
    height: null,
    mimeType: "audio/wav",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function pdfFixture(): FileInfo {
  return {
    path: "/visual-fixtures/quarterly-report.pdf",
    name: "quarterly-report.pdf",
    extension: "pdf",
    size: 5_780_211,
    category: "document",
    format: "pdf",
    width: null,
    height: null,
    mimeType: "application/pdf",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function dataFixture(): FileInfo {
  return {
    path: "/visual-fixtures/report.csv",
    name: "report.csv",
    extension: "csv",
    size: 84_612,
    category: "other",
    format: "csv",
    width: null,
    height: null,
    mimeType: "text/csv",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function archiveFixture(): FileInfo {
  return {
    path: "/visual-fixtures/client-files.7z",
    name: "client-files.7z",
    extension: "7z",
    size: 1_248_512,
    category: "other",
    format: "7z",
    width: null,
    height: null,
    mimeType: "application/x-7z-compressed",
    createdAt: null,
    modifiedAt: null,
    readOnly: false,
  };
}

function loadedFilesForOperation(operation: Operation): FileInfo[] {
  switch (operation) {
    case "convert":
      return [imageFixture(), jpegFixture(), videoFixture()];
    case "mergePdf":
      return [pdfFixture(), imageFixture(), jpegFixture()];
    case "rename":
      return [imageFixture(), jpegFixture()];
    case "createArchive":
      return [dataFixture()];
    case "extractArchive":
      return [archiveFixture()];
    case "splitPdf":
    case "exportPdfPages":
    case "compressPdf":
    case "extractText":
      return [pdfFixture()];
    case "compressAudio":
    case "transcribe":
      return [audioFixture()];
    case "encodeVideo":
    case "removeAudio":
    case "extractSubtitles":
    case "generateThumbnails":
    case "extractAudio":
      return [videoFixture()];
    case "optimize":
      return [jpegFixture()];
    default:
      return [imageFixture()];
  }
}

function seedTranscriptionModelFixture() {
  useTranscriptionModels.setState({
    models: [
      {
        model: "tiny",
        label: "Tiny",
        expectedSize: 77_691_713,
        localSize: 77_691_713,
        downloaded: true,
      },
      {
        model: "base",
        label: "Base",
        expectedSize: 147_964_211,
        localSize: 147_964_211,
        downloaded: true,
      },
      {
        model: "small",
        label: "Small",
        expectedSize: 487_601_967,
        localSize: 0,
        downloaded: false,
      },
    ],
    loading: false,
    downloadingModel: null,
    downloadJobId: null,
    progress: null,
    error: null,
    refresh: async () => {},
  });
}

function activityFixtureJobs(): RecentJob[] {
  const now = Date.now();
  const fixtures: Array<{
    operation: Operation;
    inputName: string;
    outputName?: string;
    status?: "completed" | "failed" | "cancelled";
  }> = [
    { operation: "convert", inputName: "homepage-hero.png", outputName: "homepage-hero.webp" },
    { operation: "compressPdf", inputName: "Q3-report.pdf", outputName: "Q3-report-small.pdf" },
    { operation: "encodeVideo", inputName: "product-demo.mov", outputName: "product-demo.mp4" },
    { operation: "resize", inputName: "team-photo.jpg", outputName: "team-photo-resized.jpg" },
    { operation: "extractAudio", inputName: "interview.mp4", outputName: "interview.mp3" },
    { operation: "transcribe", inputName: "meeting.m4a", outputName: "meeting.txt" },
    { operation: "optimize", inputName: "dashboard.png", outputName: "dashboard-optimized.png" },
    { operation: "exportPdfPages", inputName: "brochure.pdf", outputName: "brochure-page-1.png" },
    {
      operation: "removeMetadata",
      inputName: "press-photo.jpg",
      outputName: "press-photo-clean.jpg",
    },
    {
      operation: "extractArchive",
      inputName: "assets.zip",
      outputName: "assets",
      status: "failed",
    },
    { operation: "createArchive", inputName: "client-files", outputName: "client-files.zip" },
    { operation: "extractText", inputName: "research.epub", status: "cancelled" },
  ];

  return fixtures.map((fixture, index) => {
    const inputPath = `/visual-fixtures/${fixture.inputName}`;
    return {
      id: `visual-activity-${index + 1}`,
      operation: fixture.operation,
      completedAt: now - index * 47 * 60_000,
      ...(index === 0
        ? {
            setup: {
              version: 1 as const,
              directory: "/visual-fixtures/exports",
              suffix: "",
              inputs: { [inputPath]: { outputFormat: "webp" } },
            },
          }
        : {}),
      items: [
        {
          inputPath,
          inputName: fixture.inputName,
          ...(fixture.status === "failed"
            ? { status: "failed" as const, errorKind: "ProcessFailed" as const }
            : fixture.status === "cancelled"
              ? { status: "cancelled" as const, errorKind: "Cancelled" as const }
              : {
                  status: "completed" as const,
                  outputPath: `/visual-fixtures/${fixture.outputName}`,
                  outputPaths: [`/visual-fixtures/${fixture.outputName}`],
                }),
        },
      ],
    };
  });
}

export function applyVisualFixture(value: string): boolean {
  if (value === "recentJobs" || value === "activity") {
    useRecentJobs.setState({
      recordingEnabled: true,
      jobs:
        value === "activity"
          ? activityFixtureJobs()
          : [
              {
                id: "visual-recent-job",
                operation: "convert",
                completedAt: Date.now(),
                items: [
                  {
                    inputPath: "/visual-fixtures/product-photo.png",
                    inputName: "product-photo.png",
                    outputPath: "/visual-fixtures/product-photo.webp",
                    outputPaths: ["/visual-fixtures/product-photo.webp"],
                    status: "completed",
                  },
                ],
              },
            ],
    });
    return true;
  }
  if (!VISUAL_FIXTURE_OPERATIONS.has(value as Operation)) return false;

  const operation = value as Operation;
  if (operation === "transcribe") seedTranscriptionModelFixture();
  const store = useAppStore.getState();
  store.setOperation(operation);
  store.reset();
  store.addFiles(loadedFilesForOperation(operation));

  if (operation === "resize") store.setResizeDimensions(1600, 1000);
  if (operation === "optimize") {
    store.setImageOptimizationGoal("fileSize");
    store.setImageTargetSizeBytes(500 * 1024);
  }
  if (operation === "exportImages") {
    store.setImageExportPresets(["web", "email", "social", "preview"]);
  }
  if (operation === "encodeVideo") {
    store.setVideoCompressionGoal("fileSize");
    store.setVideoTargetSizeBytes(25 * BYTES_PER_MEBIBYTE);
  }
  if (operation === "extractAudio") {
    store.setAudioTracks(videoFixture().path, [
      {
        streamIndex: 1,
        codec: "aac",
        language: "eng",
        title: "Main mix",
        channels: 2,
        channelLayout: "stereo",
        sampleRate: 48_000,
        isDefault: true,
      },
    ]);
    store.setAudioOutputFormat("m4a");
  }
  if (operation === "extractSubtitles") {
    store.setSubtitleTracks(videoFixture().path, [
      {
        streamIndex: 2,
        codec: "subrip",
        language: "eng",
        title: "English captions",
        isDefault: true,
        isForced: false,
        supported: true,
      },
    ]);
    store.setSubtitleOutputFormat("vtt");
  }
  if (operation === "generateThumbnails") {
    store.setThumbnailOutputFormat("png");
  }
  if (operation === "splitPdf") {
    store.setPdfSplitMode("extract");
    store.setPdfPageSelection("1-3, 5");
  }
  if (operation === "compressPdf") {
    store.setPdfCompressionGoal("fileSize");
    store.setPdfTargetSizeBytes(PDF_BYTES_PER_MEBIBYTE);
  }
  if (operation === "createArchive") store.setArchiveFormat("gzip");
  if (operation === "rename") {
    store.setRenameSettings({ prefix: "client-", numbering: "suffix", start: 1 });
  }
  if (operation === "recognizeText") store.setOcrOutputFormat("searchablePdf");

  return true;
}
