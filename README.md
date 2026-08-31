# ConvertKit

A private, offline file workspace for macOS. Convert media and documents, resize, optimize,
recognize text in images and scanned PDFs, work with media, archives, metadata, checksums,
and filenames, and keep the whole workflow in one focused dark interface.

Built with Tauri v2, React, and Rust.

## Available Tools

| Tool              | Inputs                                                  | Outputs                                                 |
| ----------------- | ------------------------------------------------------- | ------------------------------------------------------- |
| Convert images    | JPG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC         | Other image formats, SVG, or one-page PDF for PNG/JPEG  |
| Convert SVG       | SVG                                                     | Raster image formats                                    |
| Convert video     | MP4, MOV, WebM, MKV, AVI                                | Video, GIF, or extracted audio                          |
| Compress video    | MP4, MOV, WebM, MKV, AVI, M4V                           | H.264 MP4, H.265 MP4, VP9 WebM, or lossless FFV1 MKV    |
| Remove audio      | MP4, MOV, WebM, MKV, AVI, M4V                           | Silent copy in the original container                   |
| Convert audio     | MP3, WAV, AAC, FLAC, OGG, M4A                           | Other audio formats                                     |
| Compress audio    | MP3, WAV, AAC, FLAC, OGG, M4A                           | A smaller AAC-encoded M4A when compression helps        |
| Extract audio     | MP4, MOV, WebM, MKV, AVI, M4V with embedded audio       | Selected audio track as MP3, M4A, WAV, or FLAC          |
| Transcribe        | Supported audio and video files                         | Local Whisper transcript as TXT, SRT, or WebVTT         |
| Extract subtitles | MP4, MOV, WebM, MKV, AVI, M4V with embedded text tracks | SRT or WebVTT                                           |
| Video thumbnails  | MP4, MOV, WebM, MKV, AVI, M4V                           | Midpoint thumbnail or 4×3 contact sheet as JPEG or PNG  |
| Extract text      | PDF, DOCX, HTML, Markdown, EPUB                         | One plain-text copy per document                        |
| Recognize text    | PDF, JPG, PNG, GIF, TIFF, BMP, HEIC                     | Plain text or one searchable PDF per file               |
| Convert documents | DOCX, HTML, Markdown, EPUB, TXT                         | Supported document formats, including PDF               |
| Resize image      | Raster image formats                                    | A resized copy in the original format                   |
| Optimize image    | Raster image formats                                    | A verified smaller copy, or the unchanged source        |
| Export images     | Raster image formats                                    | Any selected Web, Email, Social, and Preview variants   |
| Remove metadata   | JPEG, PNG, WebP, PDF, supported audio and video         | A metadata-cleaned copy without media re-encoding       |
| Combine to PDF    | Two or more PDF, PNG, or JPEG files                     | One PDF in the chosen order                             |
| Split PDF         | One or more PDF files                                   | One PDF per page or one PDF containing selected pages   |
| Export PDF pages  | One or more PDF files                                   | Selected pages as PNG or JPEG at screen or print size   |
| Compress PDF      | One or more PDF files                                   | Smaller source-preserving copies by quality or file size |
| Create archive    | Files or folders; GZIP accepts one regular file         | One ZIP, 7Z, TAR, TAR.GZ, or single-file GZIP           |
| Extract archive   | ZIP, 7Z, TAR, TAR.GZ, TGZ, or standalone GZIP files     | One output folder per archive, or one decompressed file |
| Batch rename      | Any regular files                                       | In-place filenames with extensions preserved            |
| Inspect files     | Any files or folders                                    | Metadata plus computed or verified checksums            |

Most tools accept multiple files. Convert, Resize, Optimize, Export images,
Compress video, Compress audio, Remove audio, Extract audio, Transcribe, Extract subtitles, Video
thumbnails, Extract text, Recognize text, Split PDF, Export PDF pages, Compress PDF,
and Extract archive run batch queues;
Combine to PDF, Create archive, and Batch rename process their input lists as one transaction.
GZIP creation is the deliberate exception: it accepts exactly one directly selected regular file.
Each tool keeps its uploaded files, settings, queue status, and results when you switch
sidebar tabs. Inspect accepts files regardless of extension.
Pasting text into settings behaves like a normal form field. Image clipboard data is accepted
only by compatible tools, is bounded to 64 MiB, and is stored in an app-managed temporary
workspace that is cleaned on exit.
Uploaded paths and per-tool settings also survive an app restart in a bounded local draft.
ConvertKit revalidates every restored path through the native input collector, refreshes
its file details, drops missing or unsupported entries, and returns interrupted work to a
pending queue. Startup restoration and job preflight claim the exact workspace before their
asynchronous checks begin, so late results cannot overwrite another tab or start against the
wrong queue. File intake, settings, navigation, and automation wait until that claim settles.
Processes never pretend to resume halfway through; completed output remains
available through Recent Jobs. Completed, partial, failed, skipped, and cancelled batch
outcomes persist there across restarts, including transcription. Failed rows retain only a
safe error category for reopening and retrying the source; raw engine diagnostics are not
stored in history. The sidebar shows the newest three jobs; **View all activity** opens all 12
retained jobs with isolated **Load job**, legacy **Open sources**, output actions, and individual
removal. New jobs retain a bounded versioned snapshot of their output folder, suffix, reusable
processing settings, compatible per-file conversion targets, page selection, embedded-stream
selection, and folder-relative paths. Loading revalidates the surviving sources first, replaces
only that utility's staged session in one atomic commit, and never starts processing. Older
history without a valid setup remains source-only. Use **Pause** in the Recent header to stop
recording new paths without removing existing entries; **Resume** re-enables history and
**Clear** removes current entries.
Output-producing tools can save, apply, and remove operation-specific recipes. A recipe
captures the relevant processing choices together with the destination folder and filename
suffix. Recipes still exclude input-specific decisions such as an embedded stream or a PDF page
range; Activity may restore those only from the exact validated job that recorded them. Existing
output-only presets migrate as compatible recipes. On macOS, the bolt beside an active recipe installs or removes a matching
Finder Quick Action. Running it stages the selected files in an isolated queue with the recipe
restored, leaving the normal preflight and final Run button in control.

## Conversion Format Matrix

The Convert picker is generated from `src/lib/formatMatrix.json`. Rust tests compare
that same matrix against every backend engine route, so an output cannot be added to
the GUI without a real implementation.

| Conversion       | Accepted inputs                                  | Output choices                                                    | Required engine                                                                   |
| ---------------- | ------------------------------------------------ | ----------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Raster image     | JPEG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC | Any other listed raster image format                              | ImageMagick                                                                       |
| Image to PDF     | JPEG, PNG                                        | One single-page PDF per source                                    | Bundled macOS Quartz/ImageIO helper                                               |
| Raster to vector | JPEG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC | SVG                                                               | VTracer                                                                           |
| Vector to raster | SVG                                              | JPEG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC                  | resvg for PNG when available; ImageMagick fallback and other outputs              |
| Video            | MP4, MOV, WebM, MKV, AVI                         | Any other listed video format, GIF, MP3, WAV, AAC, FLAC, OGG, M4A | FFmpeg                                                                            |
| Audio            | MP3, WAV, AAC, FLAC, OGG, M4A                    | Any other listed audio format                                     | FFmpeg                                                                            |
| Document         | DOCX, HTML, Markdown, EPUB, TXT                  | Any other listed document format, including PDF                   | Pandoc; LibreOffice for DOCX to PDF; Tectonic also required for other PDF outputs |
| PDF              | PDF                                              | No general conversion targets                                     | Use Extract Text, Combine to PDF, Split, or Compress PDF                          |

Common aliases such as JPEG/JPG, TIFF/TIF, HEIC/HEIF, HTML/HTM, Markdown/MD,
M4V/MP4, OGA/OGG, and WAV/WAVE normalize to the same routes. ZIP, 7Z, TAR, and GZIP are
handled by the focused archive tools, not presented as conversion formats. GZIP creation accepts
exactly one directly selected regular file. Before a job starts,
ConvertKit checks the exact required executable and shows a dependency message when it
is unavailable. PDF-to-DOCX is intentionally not advertised because the available
conversion is not reliable enough to promise.

PNG and JPEG inputs can create one source-preserving, metadata-free, single-page PDF each.
PDF stays last in the Convert picker so existing defaults do not change. Combine to PDF
accepts an ordered mix of two to 100 PDF, PNG, and JPEG files and creates one document in a
single transaction. Existing PDF pages remain in place, while each image becomes one
metadata-free page with its aspect ratio preserved.

The packaged macOS app has completed one six-file, 50% Resize queue with JPEG, PNG,
WebP, GIF, HEIC, and TIFF while launched with only `/usr/bin:/bin` on `PATH`; all six
outputs were verified at 320 × 240 and every source remained intact. A separate
14,000 × 14,000 packaged-app job was cancelled while ImageMagick was running, leaving
no output, hidden partial, or child process. Automated coverage repeats the format
checks and enforces the ten-minute timeout and cleanup paths. Resize also probes the source
and candidate before commit: the filename must match the source contents, and the result must
retain its logical format, exact frame/page count, and expected dimensions on every frame.
Real-engine tests cover JPEG, PNG, WebP, GIF, HEIC, TIFF, BMP, AVIF, and ICO, including
animated GIF, multi-page TIFF, and multi-frame ICO inputs.

The same minimal-`PATH` package also completed a mixed Convert queue containing MP4
to MOV, SVG to JPEG, Markdown to PDF, PNG to JPEG, and WAV to MP3. Independent checks
confirmed the expected media streams and durations, 320 × 200 raster outputs, source
text in the PDF, retained input files, and no hidden partial outputs.

Raster conversion inspects both the source and candidate. Static outputs must keep the source
geometry and match the requested logical format. Multi-frame or multi-page images are preserved
only when the target can store the complete topology in one file; otherwise conversion stops
before ImageMagick runs instead of dropping frames or leaking numbered sidecar files.

## Requirements

Primary media and image conversion engines:

```
brew install ffmpeg imagemagick
```

The packaged app includes a target-specific Whisper CLI, GGML backends, native libraries,
and their upstream license files. ConvertKit downloads and verifies only the selected Tiny,
Base, or Small model; audio and transcripts stay on the Mac. Building from source stages
the currently pinned Homebrew runtime and rejects a different version or architecture.

```
brew install whisper-cpp
```

Focused document, vector, PDF, and optimization engines:

```
brew install pandoc resvg tectonic poppler ghostscript
brew install oxipng jpegoptim gifsicle
brew install --cask libreoffice
cargo install vtracer
```

Optimize always has an ImageMagick fallback. When available, OxiPNG, jpegoptim,
and Gifsicle are also tried for their matching formats. Candidates must decode as the
same logical format and preserve the exact frame count and per-frame dimensions. Only the
smallest verified candidate that is strictly smaller than the source is committed; otherwise
the source is left untouched and the queue reports **Already optimized** without creating a copy.
JPEG, WebP, AVIF, and HEIC can instead use a file-size goal from 16 KiB through 102,400 KiB.
The target must be a whole kibibyte smaller than the source. ConvertKit searches descending
lossy quality levels and commits only a verified same-format, same-dimensions, same-frame-count
result at or below the exact byte ceiling; if none qualifies, it returns a typed unreachable-target
error and leaves no output. The packaged debug boundary has independently kept JPEG and WebP
below an exact 256 KiB target under a minimal launch `PATH`, produced a numbered keep-both JPEG,
preserved every source hash, and left no output, hidden partial, temporary workspace, or
ImageMagick child after unreachable and cancelled jobs.

Run that packaged proof after building the debug app:

```sh
scripts/smoke-image-target-size.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

Export images is an explicit delivery recipe rather than an editor. Each source can create
any combination of Web (WebP, maximum 1,920 px), Email (JPEG, maximum 1,600 px), Social
(JPEG, maximum 2,048 px), and Preview (WebP, maximum 640 px) variants. Every variant keeps
the source aspect ratio, never upscales or crops, applies format-aware compression, removes
metadata, and uses a distinct filename suffix. Transparent pixels use a white matte for the
JPEG presets. Animated and multi-page sources are rejected with an explicit error instead of
silently discarding frames. One source is committed as a complete collision-safe output set;
a failure or cancellation removes partial variants, while recent history and Finder reveal
retain every successful output path. Packaged minimal-`PATH` smoke runs verified all four
formats and dimensions, unchanged source content, retained multi-output paths, and numbered
keep-both naming on a repeated export.

Compress audio creates AAC-encoded M4A delivery copies through three focused presets:
High at 256 kbps, Balanced at 160 kbps, and Smallest at 96 kbps. Each result must contain
exactly one AAC audio stream, no video, the source channel count and supported sample rate,
and matching duration. ConvertKit commits the result only when it is strictly smaller than
the source; an already-compact input reports `OutputNotSmaller` and leaves no derivative.
The workflow uses batch and folder input, retained settings, shared output controls, progress,
cancellation, source preservation, Recent Jobs, and Finder reveal without adding audio-editing
controls. The Apple Silicon package gate exercises every preset through the packaged request
boundary under a minimal launch `PATH`, plus keep-both naming, an already-compact source, and
cancellation and hidden-partial cleanup.

Run the packaged proof after building the debug app:

```sh
scripts/smoke-audio-compression.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

Extract audio inspects every embedded audio stream before enabling the batch. It shows
language, title, codec, channel layout, sample rate, and default-track metadata and lets
each video select its own track before creating MP3, M4A, WAV, or FLAC output. The backend
revalidates the selected global stream index and codec, maps only that audio stream, and
verifies that the result contains one correctly encoded audio stream. Batch and folder
input, source preservation, saved recipes, progress, cancellation, recent history, and
Finder actions use the shared workflows. A real-media test selects the non-default track
from a two-track MKV and verifies that its non-silent audio is isolated without changing
the source.

Extract subtitles inspects every embedded subtitle stream before enabling the batch.
It shows language, title, default, and forced-track metadata, lets each video select its
own track, and exports text-based tracks to SRT or WebVTT. Image-based subtitle streams
remain visible as unsupported because the focused OCR tool accepts images and PDFs rather
than subtitle bitmap streams. The backend
revalidates the selected global stream index and codec before writing, verifies real
UTF-8 timing cues, preserves every source, and uses the shared output, cancellation,
recent-history, and Finder workflows. Real-media tests select a non-default language
from a multi-track MKV and verify isolated SRT and WebVTT output.

Transcribe creates TXT, SRT, or WebVTT from audio and video with Whisper running locally.
ConvertKit preprocesses the primary audio stream to 16-bit mono 16 kHz WAV, supports
automatic or explicit language selection, and manages separately downloaded Tiny, Base,
and Small GGML models with pinned size and SHA-1 verification. Model download progress,
deletion, job cancellation, retained queues and settings, saved recipes, recent history,
and Finder reveal all use the shared workflows. Apple Silicon bundles include pinned
whisper.cpp 1.9.2, GGML 0.22.0, OpenMP 22.1.8, and five CPU/Metal/BLAS backends with every
Homebrew load path rewritten to the signed app. A clean-environment load audit resolved
the executable, libraries, and CPU backend exclusively inside `ConvertKit.app`, and the
typed packaged-app job produced an independently checked transcript with an unchanged
source hash. The existing TXT, SRT, WebVTT, and cancellation smokes continue to cover
format output and cleanup. The current support claim is Apple Silicon only.

Video thumbnails creates one derived image per source video: either a representative
midpoint frame bounded to 1280 × 720 or a fixed 12-frame contact sheet arranged in a
4 × 3 grid. Outputs are JPEG or PNG, videos are processed in batches, and the source is
never changed. FFmpeg output is written atomically and then verified for the expected
codec and dimensions. The tool intentionally has no timeline, scrubber, crop, filter,
annotation, or manual frame-editing surface.

Compress video provides four batch presets: Compatible creates H.264/AAC MP4,
Smaller creates H.265/AAC MP4, Web creates VP9/Opus WebM, and Lossless creates
FFV1/FLAC MKV. The delivery controls provide Auto, Original, 1080p, and 720p maximum
resolutions without upscaling. Quality remains the default compression goal, with
codec-appropriate High, Balanced, and Smallest settings. Compatible, Smaller, and Web can
instead target a whole-megabyte file size from 1 MiB through 100 GiB; Lossless remains fixed
at original dimensions and quality and does not offer file-size targeting. Target-size jobs
budget the primary video and optional audio streams from the measured duration, run verified
two-pass FFmpeg encoding, and make one bounded lower-bitrate retry when the first result is
too large. Both modes use the same typed source/output probe and commit only when the
container, video codec, positive dimensions, exactly one primary audio stream when present,
its preset codec, and source duration are valid; target-size mode then applies its exact byte
ceiling. The selected goal and target survive tool switches and app restarts and are
included in saved recipes. Every source is retained, and cancellation
or failure removes hidden partial files and pass logs. The packaged app completed all four
default-quality preset routes while launched with a minimal system `PATH`; the smoke checks
also verified resolution, primary-audio retention, duration, source retention, and cancellation
cleanup. A separate packaged target-size smoke kept real H.264/AAC, H.265/AAC, and VP9/Opus
outputs below an exact 2 MiB ceiling, decoded every result, preserved the source, produced a
numbered keep-both copy, and left no committed output, hidden partial, or pass-log workspace
after cancellation.

Run the packaged quality and target-size proofs after building the debug app:

```sh
scripts/smoke-video-quality.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
scripts/smoke-video-target-size.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

Remove audio creates a silent copy in the source container using FFmpeg stream copy,
so the encoded video is not recompressed. It accepts batch and recursive folder input,
keeps every source, rejects videos that are already silent, and verifies that each output
contains video but no audio. Packaged-app MP4 and MOV checks retained codec, dimensions,
and duration and produced byte-identical encoded video streams; cancelling a 557 MB job
left no output, hidden partial, or FFmpeg child process.

Remove metadata works in batches without re-encoding image pixels or encoded media
streams. Its built-in image path removes EXIF (apart from display orientation), XMP,
comments, and PNG text/time chunks from JPEG, PNG, and WebP while preserving ICC color
profiles. PDFs are structurally rewritten without rasterizing pages: document information,
XMP, piece information, old IDs, and incremental-history remnants are removed, while page
geometry and all reachable non-metadata structure must match before commit. Encrypted and
digitally signed PDFs are rejected. Audio and video use FFmpeg stream-copy to remove global
and per-stream private tags plus chapters; the result is rejected unless every stream codec
is preserved and sensitive tags are absent. This utility does not redact visible text, form
values, comments, or attachment contents. Sources are kept and cleaned copies use the
`-clean` suffix.

Extract text saves one UTF-8 `.txt` copy per PDF or document. PDF extraction uses
Poppler's `pdftotext`; DOCX, HTML, Markdown, and EPUB extraction use Pandoc. It accepts
batch and recursive folder input, preserves relative folders and source files, and uses
the shared suffix, saved-recipe, cancellation, recent-job, and Finder workflows.

Recognize text creates either one UTF-8 `.txt` copy or one searchable `.pdf` per common raster
image or scanned PDF. Searchable PDF keeps the visible scan and page order while adding an
invisible selectable text layer; an image becomes one page and a PDF remains one output with
the same number of pages.
Recognition uses Apple's Vision framework entirely on the Mac, with accurate recognition,
language correction, and automatic language detection where the installed macOS version
supports it. The native helper is compiled for the current Mac target and bundled inside the
app; no model download, API key, or network service is involved. Plain-text PDF input is
rendered one page at a time with Poppler. Searchable PDFs are composed by the bundled native
helper without an additional runtime dependency. Both modes are bounded at 250 pages and use
the same batch, folder, recipe, cancellation, source-preservation, recent-job, and Finder
workflows as Extract text. A packaged-app smoke pass under a minimal `PATH` verified the legacy
text default, one-page image PDF, multipage scanned PDF page order and visible rendering,
extractable text, keep-both naming, and unchanged source hashes. A cancelled 120-page job left
no output, hidden partial, workspace, renderer, or OCR helper process.

PDF tools use Poppler's `pdfinfo`, `pdfseparate`, `pdfunite`, and `pdftoppm`. Combine to PDF
keeps the visible input order, converts PNG and JPEG inputs through the bundled image-to-PDF
helper in a temporary workspace, and verifies that the final page count matches every source
PDF page plus one page per image. Mixed-input frontend and Rust tests cover retained ordering,
input rejection, and exact dependency selection. A packaged-app smoke combines a PNG, a
two-page PDF, and a JPEG, verifies all four rendered pages in order, preserves every source
hash, strips private image comments, and confirms numbered keep-both output. Page export
creates PNG or JPEG files at 144 or 300 DPI from every page or an explicit page selection.
Packaged minimal-`PATH` smoke runs verified selected-page 144-DPI PNG and every-page 300-DPI
JPEG output from a two-page A4 source, including exact pixel dimensions and source retention.
Before any job starts,
the app checks the exact operation and settings and explains which optional tool is
missing when necessary. PDF compression uses Ghostscript with separate Quality and File size
goals. Quality offers High quality, Balanced, and Smallest presets and keeps the original
bytes when recompression would make a PDF larger. File size accepts a whole-MiB target from
1 MiB through 100 GiB, requires it to be smaller than every queued source, and tries a bounded
ten-step quality/resolution ladder. Both goals save a result only when it is a structurally
readable PDF whose page count, rotation, and effective Media/Crop/Bleed/Trim/Art boxes all
match the source; File size must also remain below the exact ceiling. The setting is retained
across tool switches and restarts and is included
in saved recipes. A packaged minimal-`PATH` smoke reduced a
5,780,211-byte, 13-page mixed raster/vector PDF to 760,081 bytes under an exact 1 MiB ceiling,
created a numbered keep-both result, preserved every source hash and page box, and verified
that cancellation and an unreachable target leave no output, hidden partial, or Ghostscript
child.

Progress and cancellation are scoped to the exact job that produced them. A late progress
event or cancellation fallback from an older job is ignored after another job starts, and
every success, failure, or cancellation clears the active identity and running rows before a
retry can begin. The frontend race suite explicitly cancels job A, starts job B, and proves
that A can no longer change B. The packaged OCR cancellation smoke also confirms that the
rebuilt app leaves no committed output, hidden partial, temporary workspace, renderer, or
native helper behind.

ZIP, 7Z, TAR, TAR.GZ, and single-file GZIP creation plus ZIP, 7Z, TAR, TAR.GZ, TGZ, and standalone
GZIP extraction are built in and do not require another command-line tool. 7Z creation streams
an archive, optionally encrypts both file contents and headers with AES-256, preserves relative
paths, and verifies its manifest and every decoded byte against the selected files before commit.
Passwords remain request-only memory, are redacted from diagnostics, and are never written to
workspace recovery, Activity, or recipes. GZIP creation accepts
exactly one direct regular file, preserves its original extension in names such as
`report-archive.csv.gz`, and verifies the complete decoded byte stream before commit. A plain
`.gz` decompresses to one source-named file, while `.tar.gz` remains a TAR.GZ archive that
unpacks into a folder.
The packaged macOS app registers ZIP, TAR, 7Z, GZIP, and TGZ as Viewer document types so these
supported archives are available through Finder's Open With flow.
Extraction rejects traversal paths, links, duplicate paths, filesystem-equivalent ZIP path
collisions, TAR special files, and unsafe 7Z anti/link/special entries; limits entry count and
expanded size; and commits to the visible output folder only after the archive finishes
successfully.
Finder reveal accepts created or decompressed files and extracted output folders; the native
boundary revalidates each item and rejects missing paths, links, and special filesystem entries.

Run the packaged archive smoke after building the debug app:

```bash
scripts/smoke-archive.sh src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
scripts/smoke-7z-creation.sh src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
scripts/smoke-gzip-creation.sh src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
scripts/smoke-7z-extraction.sh src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
scripts/smoke-gzip-extraction.sh src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

It creates and extracts a nested TAR.GZ through the packaged backend, compares the extracted
bytes to both sources, verifies source hashes, and checks that no partial output remains. The 7Z
creation smoke builds and re-extracts a nested archive entirely through the packaged app,
requires the correct password for its encrypted headers and contents, byte-compares both files,
exercises keep-both naming and mid-stream cancellation, and proves source and partial cleanup
invariants. The 7Z extraction smoke uses a deterministic nested
fixture without an installed `7z` command, verifies
byte content, keep-both naming, and source preservation, then checks traversal, password-required,
damaged-archive, and cancellation failures and cleanup.
The GZIP creation smoke independently decodes the packaged app's output, checks exact bytes,
extension-preserving and keep-both names, invalid multi-file input, source hashes, cancellation,
and partial cleanup. The standalone GZIP extraction gate uses embedded streams instead of a
system `gzip`, checks exact single-file bytes and naming, then covers keep-both numbering,
truncated and CRC-corrupt input, source preservation, cancellation, and cleanup under a minimal
`PATH`.

Batch rename is also built in. It previews find/replace, prefix, suffix, and optional
numbering changes before enabling the action. Extensions are always preserved, name
collisions are rejected, and the entire batch must be confirmed before any filename
changes. Successful batches can be undone from the result or recent-jobs list; unused
undo records expire after 30 days. Its packaged smoke verifies ordered output paths, extension
and byte preservation, occupied-target and extension-change rejection, deterministic
cancellation and failure rollback, undo, and managed-record cleanup.

File inspection is also built in. It shows type, byte size, image dimensions when
available, creation and modification dates, read-only status, and the full path. PDF
files include page count and page size; audio and video include container, duration,
bit rate, codecs, resolution, frame rate, and per-stream audio and subtitle metadata when
available.
Checksums are computed locally in one streaming pass and can be cancelled on large files.
Paste a raw, labeled, BSD-style, or checksum-manifest MD5, SHA-1, or SHA-256 value to
verify it against a freshly computed result.
Inspect can also create a standard `checksums.sha256` beside a same-folder or folder-derived
queue, using numbered keep-both names without changing any source. Importing a `.sha256`,
`.sha1`, or `.md5` manifest verifies its safe relative regular-file entries sequentially and
reports each as matched, mismatched, or missing. Absolute paths, traversal, duplicates, links,
special files, malformed or oversized manifests, and more than 100 entries are rejected before
hashing.
The full inspection queue can be exported to a versioned JSON report or a flat CSV in
the user's chosen location. Its packaged smoke verifies exact metadata, all three checksums,
the JSON report schema and fields, source preservation, and partial cleanup.

The dependency-contained checksum-manifest smoke exercises standard creation, keep-both naming,
all verification outcomes, traversal rejection, progress-triggered cancellation, and cleanup:

```sh
scripts/smoke-checksum-manifest.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

## Legacy-state cleanup

App Icon Package, Normalize Audio, and Duplicate Finder have been removed. Persisted workspace,
recipe, and activity data is sanitized so their legacy operation identifiers cannot reactivate
them. ConvertKit also removes the Finder Quick Action previously managed by retired recipes.

## Development

```
pnpm install
pnpm tauri dev
```

Loaded-state visual fixtures are available only in Vite development mode. Run `pnpm dev`,
then point a Tauri development window at one of `convert`, `resize`, `optimize`, `exportImages`,
`encodeVideo`, `transcribe`, `exportPdfPages`, `compressPdf`, `inspect`, the `recentJobs`
sidebar fixture, or the 12-row `activity` fixture:

```sh
pnpm tauri dev --no-watch --config \
  '{"build":{"beforeDevCommand":"","devUrl":"http://127.0.0.1:1420/?fixture=encodeVideo"}}'
```

These fixtures exercise native layout and scrolling only; they use deliberately unavailable
paths so dependency preflight remains honest and no mock file can be processed.

Completed queue rows, completed batch headers, Recent Jobs, and Activity share one output-actions
menu. It can reveal one result or the complete stable-deduplicated batch, or stage that output set
in another utility compatible with every file, without clearing either workspace or starting the
next job automatically. `Cmd+R` also reveals every output from the completed batch. Use the
`recentJobs` and `activity` fixtures to review
the compact sidebar placement, full history list, and portal menu at the default window size.
The menu supports Arrow keys, Home/End, Escape, and focus restoration. Queue progress and
terminal outcomes are announced through one stable polite status region, while rejection and
download/progress feedback expose the appropriate alert and progressbar semantics.

### Scripts

| Command                            | Description                                                  |
| ---------------------------------- | ------------------------------------------------------------ |
| `pnpm tauri dev`                   | Run in development mode                                      |
| `pnpm tauri build`                 | Build production DMG                                         |
| `pnpm check`                       | Type-check with tsgo                                         |
| `pnpm test`                        | Run frontend persistence and behavior tests                  |
| `pnpm lint`                        | Lint with oxlint                                             |
| `pnpm format`                      | Format with oxfmt                                            |
| `pnpm icons:generate`              | Regenerate the retained macOS and app-shell icon assets      |
| `pnpm native:helpers`              | Build native OCR/PDF helpers and stage the Whisper runtime   |
| `pnpm package:smoke:apple-silicon` | Build and smoke-test the ad-hoc arm64 app                    |
| `pnpm release:doctor`              | Check macOS release tooling and reference setup              |
| `pnpm release:check`               | Resolve release references and validate the signing identity |
| `pnpm release:macos`               | Gate, sign, notarize, staple, and verify the release DMG     |

`pnpm package:smoke:apple-silicon` is the local package gate for the supported architecture.
It refuses Rosetta and non-macOS hosts, builds the explicit `aarch64-apple-darwin` debug app,
applies and verifies an ad-hoc signature, audits the packaged Whisper runtime, and runs the
quality-video, target-size-video, audio-compression, target-size-image, image-to-PDF,
Combine-to-PDF, OCR/searchable-PDF, PDF-compression, PDF-metadata,
dependency-contained archive creation/extraction, Inspect, checksum-manifest, and Batch Rename
smokes. The media gates require the same full FFmpeg installation used by the visible compression
tools.
`pnpm release:macos` runs this
gate before it begins the credentialed release build.

This repository does not configure a self-hosted Apple Silicon Actions runner, so GitHub CI
does not claim to execute the arm64 package gate. Run the command on a native Apple Silicon Mac.
Audit an already packaged runtime without downloading a model:

```sh
CARGO_BUILD_TARGET=aarch64-apple-darwin \
  scripts/smoke-whisper-runtime.sh --verify-only \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

Omit `--verify-only` to run generated-speech transcription. The script uses the same pinned
Tiny-model size and checksum as the app; set `CONVERTKIT_WHISPER_MODEL_PATH` to a previously
verified local model to avoid downloading it again.

Run the packaged PDF compression smoke on a Mac with Ghostscript installed:

```sh
scripts/smoke-pdf-compression.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

Verify the bundled image-to-PDF helper and its rendered one-page output:

```sh
scripts/smoke-image-pdf.sh \
  src-tauri/target/aarch64-apple-darwin/debug/bundle/macos/ConvertKit.app
```

### Tech Stack

- **Framework**: Tauri v2 (Rust backend, web frontend)
- **Frontend**: React 18, Tailwind CSS v4, Zustand
- **Engines**: FFmpeg, ImageMagick, bundled whisper.cpp/GGML, macOS Vision and Quartz/ImageIO, Pandoc, resvg, VTracer, LibreOffice, Poppler, Ghostscript

## Build

```
pnpm tauri build
```

Output: `src-tauri/target/release/bundle/dmg/ConvertKit_<version>_<arch>.dmg`

## Signed macOS release

Release credentials are read directly from 1Password. Store the Developer ID identity,
App Store Connect issuer ID, key ID, and `.p8` private key in 1Password, then export only
their secret references:

```sh
export CONVERTKIT_SIGNING_IDENTITY_REF='op://VAULT/ITEM/signing identity'
export CONVERTKIT_APPLE_API_ISSUER_REF='op://VAULT/ITEM/issuer id'
export CONVERTKIT_APPLE_API_KEY_ID_REF='op://VAULT/ITEM/key id'
export CONVERTKIT_APPLE_API_PRIVATE_KEY_REF='op://VAULT/ITEM/private key'
```

Run `pnpm release:doctor` to check the local tools, `pnpm release:check` to resolve and
validate the references without building, then `pnpm release:macos` for the release. The
script writes the private key to a mode-0600 temporary directory only for the build, lets
Tauri sign/notarize/staple the app and DMG, runs `codesign`, `stapler`, and Gatekeeper
verification, and removes the temporary key on every exit path. Plain credentials and
private-key files must never be stored in this repository.

## Keyboard Shortcuts

| Shortcut   | Action                                   |
| ---------- | ---------------------------------------- |
| `Cmd+K`    | Find a tool                              |
| `Cmd+O`    | Open file browser                        |
| `Enter`    | Run the active tool                      |
| `Esc`      | Cancel active processing                 |
| `Cmd+R`    | Reveal all completed outputs in Finder   |
| `←/→/↑/↓`  | Change a focused selector option         |
| `Home/End` | Choose the first or last selector option |

Enter runs a loaded workspace only when focus is outside a form control or button. Escape cancels
active processing, but never clears loaded, completed, or failed work. Inputs, selectors, recipe
editors, and the tool finder retain their normal keyboard behavior.

## License

MIT
