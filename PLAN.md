# ConvertKit Product and Development Plan

> Living roadmap. Last audited against the codebase on 2026-08-29.

## North star

ConvertKit is a private, offline desktop workspace for everyday file work on macOS.
It should replace a folder full of one-off utilities without turning into a wall of
settings.

The promise is simple:

1. Choose a tool.
2. Add one or more files.
3. Adjust only the relevant options.
4. Get a predictable result without uploading anything.

"All in one" describes the breadth of the toolbox, not the complexity of each
screen. Every tool remains a focused workflow inside one consistent application.

## Product structure

The app is organized by user intent, not by file format or command-line engine.

| Area | Tools | Priority |
| --- | --- | --- |
| Convert | Cross-format image, video, audio, document, and vector conversion | Shipping foundation |
| Images | Resize, optimize, crop, rotate, metadata removal, format presets | Now / next |
| PDF and documents | Merge, split, reorder, rotate, compress, extract pages, OCR | After shared jobs |
| Video and audio | Encode presets, extract audio, normalize, trim, thumbnails, subtitles | Later |
| Inspect | Metadata, dimensions, codecs, page count, hashes, diagnostics | Later |
| Archives and organization | Compress, extract, batch rename, duplicate detection | Later |
| Automation | Finder actions, watched folders, reusable recipes | Last |

The left tool rail is the long-term navigation model. Only usable tools appear in
the primary list. Future features should not be shipped as disabled buttons.

## Experience principles

- Dark mode only until the dark interface is complete and visually consistent.
- Local-first and offline by default. Earn that trust through predictable behavior,
  permissions, and documentation instead of promotional status cards in the workspace.
- One primary action per screen.
- Show controls only when they apply to the active tool and selected file.
- Prefer presets and plain language over codec flags and engine terminology.
- Preserve the source by default and make the output destination predictable.
- Use the same loaded, running, success, error, cancel, and reveal states
  for every tool.
- Drag-and-drop, file browsing, paste, Open With, and keyboard operation are
  first-class input paths.
- A new tool must include validation, cancellation, collision-safe output naming,
  and a native-app smoke check before it is considered complete.

## Current codebase audit

### Working today

- Tauri v2 desktop shell with a React, TypeScript, Tailwind, Zustand, and Framer
  Motion frontend.
- Dark-only, resizable workspace with a persistent tool rail and native macOS window
  behavior.
- ImageMagick, FFmpeg, Pandoc, LibreOffice, resvg, and VTracer routing.
- Image, video, audio, document, and SVG format detection with common extension
  aliases.
- Multi-file browse and drag-and-drop input for Convert, Resize, and Optimize.
- A real in-memory queue with duplicate prevention, item removal, per-file conversion
  targets, sequential batch processing, per-item status/progress, skip, cancel,
  retry, and aggregate completion results.
- Independent Convert, Resize, and Optimize workspace sessions that retain uploaded
  files, queue state, settings, and finished results while navigating between tools.
- Shared output controls for Convert, Resize, and Optimize: source or custom folder,
  per-tool filename suffix, Keep both or atomic Replace existing behavior, and hard
  source-overwrite protection.
- Output folder, suffixes, and collision preference persist locally across launches.
- Determinate FFmpeg progress and indeterminate progress for other engines.
- Cancellation tokens, timeouts, partial-output cleanup, and structured errors.
- Dependency detection that also searches common Homebrew and Cargo locations.
- Drag-and-drop, filtered file browsing, image paste, and Finder Open With.
- Image, video, PDF, and document thumbnails.
- Output size comparison and Reveal in Finder on completion.
- Logging and fallback output to `~/Downloads/ConvertKit` when the source directory
  is read-only.
- Resize with original-dimension probing, ratio locking, percentage presets, and
  batch execution.
- Image optimization with metadata removal by default, an explicit metadata policy,
  multiple candidate encodes, smallest-result selection, before/after size reporting,
  source preservation, cancellation, and collision-safe output naming.
- Optional OxiPNG, jpegoptim, and Gifsicle candidates for lossless format-specific
  optimization, with ImageMagick retained as the always-available fallback.

### In progress

- Visual refinement and density consistency across the shared workspace components.
- Operation-specific file filters and validation across every input route.
- Alignment between the Rust and TypeScript compatibility matrices.
- Native-app review of the new Optimize and expanded queue flows with representative
  real-world files.

### Important gaps

- No operation-aware job request shared by Convert and Resize.
- Required tools are checked globally at launch rather than for the selected task.
- The current batch queue is in-memory. It has no folder input, recent jobs, retry
  history, or queue recovery after an app restart yet.
- A debug macOS app bundle builds cleanly; interactive release smoke testing remains.
- No clean-machine packaging verification, signing, notarization, or update flow.
- The README lists broad format support but does not distinguish input-only formats,
  optional engines, and tested conversion pairs.
- `ConvertKit` still needs a naming and trademark check before public distribution.
  The bundle identifier is aligned as `com.dropforge.convertkit`.

## Roadmap

### Phase 0 — Make conversion trustworthy

Status: active foundation work. Finish before expanding the toolbox broadly.

- [x] Normalize common extension aliases in Rust and TypeScript.
- [x] Remove advertised PDF input conversions that have no reliable engine.
- [x] Resolve executables consistently in terminal and packaged-app environments.
- [ ] Expose a backend capability query for an exact operation and input/output pair.
- [ ] Show missing optional tools before the user starts a job.
- [x] Drain subprocess output concurrently for long-running engines.
- [ ] Complete a packaged-app smoke pass with a minimal `PATH`.
- [ ] Rewrite format documentation from the verified compatibility matrix.

Exit criteria: every option shown in the UI has a working engine or a clear,
pre-flight dependency message, and representative conversions pass a native-app
smoke check.

### Phase 1 — Workspace shell and Resize

Status: in progress.

- [x] Replace the two-tab layout with a tool rail that can scale to more workflows.
- [x] Refresh the interface with one dark theme, clearer hierarchy, and a larger,
  resizable workspace.
- [x] Filter Resize input to raster images.
- [x] Read original image dimensions and initialize the resize form from them.
- [x] Add locked-ratio editing and 25%, 50%, 75%, and 100% presets.
- [x] Preserve the original and create a collision-safe resized copy.
- [x] Reuse conversion progress, cancellation, result, and Finder flows.
- [ ] Run native resize checks for JPEG, PNG, WebP, GIF, HEIC, and TIFF.
- [ ] Verify cancellation and partial-file cleanup with a large image.

Exit criteria: Resize feels like a complete tool rather than a special conversion
case and works from drag, browse, paste, Open With, keyboard, success, and error
paths.

### Phase 2 — Image workshop

Build the most common local image workflows before moving to another domain.

#### Optimize

- [x] Add a simple Optimize workflow to the tool rail.
- [x] Produce multiple format-aware candidates and keep the smallest valid result.
- [x] Remove metadata by default with a clear Keep option.
- [x] Show before/after file size and percentage saved.
- [x] Preserve the source and use collision-safe output naming.
- [ ] Consider Best, Balanced, and Smallest presets only if real-world measurements
  show that the extra choice is useful.
- [x] Add specialized optimizers as optional candidates without making them required
  dependencies or exposing engine choices in the interface.

#### Transform

- Crop with common aspect presets and a small visual preview.
- Rotate and flip.
- Convert + resize + optimize as one explicit recipe, not hidden side effects.
- Optional Web, Email, Social, and App Icon output presets.

Exit criteria: image tools share one request/result model and can be chained without
overwriting the source.

### Phase 3 — Shared jobs and batch work

This phase is the platform for every later toolbox area.

- Introduce a typed `JobRequest` with operation-specific settings.
- Centralize validation, capability checks, output policy, progress, cancellation,
  errors, and cleanup.
- [x] Add multi-file browse and drag-and-drop input.
- [x] Add a basic in-memory queue with removal and duplicate prevention.
- [x] Add per-file conversion targets and sequential batch execution.
- [x] Add aggregate batch completion results.
- [x] Bound the queue at 100 files and show explicit overflow feedback.
- [ ] Support folders where a tool allows them.
- [x] Add per-item progress, retry, skip, cancel, and aggregate status.
- [x] Retain each tool's uploaded files, queue state, settings, and results while
  switching between sidebar tabs.
- [ ] Persist a small recent-jobs list with reopen and reveal actions.
- [x] Add output folder, suffix, collision policy, and source-overwrite safeguards.
- [x] Persist lightweight output preferences locally across app launches.

Exit criteria: a new tool can plug into one job pipeline without rebuilding lifecycle
logic or UI states.

### Phase 4 — PDF and document tools

Ship narrow, dependable tools rather than a miniature document editor.

- Merge PDFs.
- Split by page range.
- Reorder, rotate, delete, and extract pages with thumbnails.
- Compress with clear quality presets.
- Convert supported documents through Pandoc and LibreOffice.
- Extract text and metadata.
- Add OCR only after language data, download UX, and licensing are settled.

PDF-to-DOCX must not be advertised until a reliable implementation is chosen and
tested. Conversion and page manipulation are separate tools.

### Phase 5 — Video and audio tools

- Video presets: H.264, H.265, Web, Small file, and Archive.
- Resolution, frame-rate, quality, and VideoToolbox acceleration controls.
- Extract audio, mute/strip audio, normalize loudness, and convert audio.
- Trim without a timeline editor.
- Extract frames and generate thumbnails/contact sheets.
- Subtitle extraction and burn-in after track inspection exists.

Avoid recreating HandBrake. Advanced flags belong behind an explicit disclosure and
must map to typed settings.

### Phase 6 — Inspect, archives, and organization

- Unified metadata inspector for images, media, PDFs, and documents.
- Copy/export technical metadata and compute common hashes.
- Create and extract ZIP/TAR archives with safe path handling.
- Batch rename with preview and undo manifest.
- Duplicate detection by size then hash, with review before deletion.

Destructive actions require previews, explicit confirmation, and a recoverable path.

### Phase 7 — Transcription and automation

#### Local transcription

- FFmpeg preprocessing to 16-bit mono 16 kHz WAV.
- Locally managed Whisper executable and separately downloaded models.
- Tiny, Base, and Small models first.
- Language auto-detection plus explicit selection.
- TXT, SRT, and VTT output.
- Download size/state, model deletion, elapsed time, and cancellation.

#### Automation

- Saved recipes.
- Finder Quick Actions and Open With routes.
- Watched folders with explicit output and conflict policies.
- Optional command-line entry point backed by the same job model.

Automation comes last because it amplifies both good and bad behavior. It should only
run tools whose single and batch flows are already dependable.

## Architecture direction

### Frontend

- Keep the tool registry centralized with label, description, accepted inputs,
  dependency requirements, and route/component metadata.
- Keep shared lifecycle state separate from operation settings.
- Continue moving from the monolithic store toward shared job state plus small
  operation-specific slices as more tools are introduced.
- Do not model Optimize, Transcribe, Inspect, or PDF actions as output formats.
- Keep the shared queue runner and session model operation-agnostic as the toolbox
  expands.

### Rust

- Keep subprocess construction and execution in Rust.
- Create shared process helpers for executable resolution, concurrent pipe draining,
  timeouts, cancellation, output verification, and cleanup.
- Keep engine flags inside typed request structs.
- Treat capabilities as `(operation, input, settings, output)` rather than "tool is
  installed."
- Use safe temporary directories for multi-step jobs and move the verified result to
  its final destination atomically where possible.
- Never build shell command strings from user-controlled paths.

### Data and privacy

- No analytics, uploads, or cloud dependency by default.
- Recent jobs store paths and settings locally and can be disabled/cleared.
- Model downloads and optional binaries are separate from user outputs.
- Temporary clipboard files need an explicit cleanup policy.
- Logging must avoid full file contents and sanitize noisy engine output.

## Quality gates

Every tool must pass these gates before it appears in the navigation:

1. Input validation and unsupported-type messaging.
2. Capability/dependency preflight.
3. Collision-safe output and source preservation.
4. Progress where measurable and an honest indeterminate state otherwise.
5. Cancellation, timeout, and partial-output cleanup.
6. Success, error, retry, reveal, and start-another flows.
7. Keyboard and drag-and-drop operation.
8. A representative native-app smoke check.
9. A packaged-app check, not only terminal development mode.
10. README support claims updated from verified behavior.

## Current checkpoint

This repository is at a useful checkpoint for pushing. Convert, Resize, and Optimize
all use the shared workspace and real multi-file queue. Per-tool sessions preserve
uploaded files, settings, item status, and results when the user changes tabs.

The next platform work is capability preflight, a typed `JobRequest`, folder input,
and a small recent-jobs list with reopen and reveal actions. The next image-tool
decision is whether to build crop/rotate or reusable output presets first.

For video, keep the product narrower than HandBrake: clear presets for dimensions,
codec, and file-size/quality reduction, followed by focused trim, audio, thumbnail,
and subtitle utilities. PDF, video, image, and general file utilities should be
prioritized together after this checkpoint is reviewed.

The reference-led circular conversion icon is selected and integrated into the
complete Tauri platform icon set. Its sole 1024px source master and generation brief
are documented in `design/icon-concepts/README.md`.
