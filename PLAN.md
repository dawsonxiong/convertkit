# ConvertKit Product and Development Plan

> Living roadmap. Last audited against the codebase on 2026-08-31.

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

| Area              | Tools                                                                | Status              |
| ----------------- | -------------------------------------------------------------------- | ------------------- |
| General           | Cross-format conversion and verified metadata removal                | Shipping foundation |
| Images            | Resize, optimize, and delivery variants                              | Working             |
| Video and audio   | Encode, extract streams, create thumbnails, subtitles, transcription | Working             |
| PDF and documents | Text extraction/OCR, combine, split, page export, and compression    | Working             |
| Organize          | Archives, batch rename, and inspection                               | Working             |
| Automation        | Saved recipes and Finder actions                                      | Working             |

The left tool rail is the long-term navigation model. Only usable tools appear in
the primary list. Future features should not be shipped as disabled buttons.

## Experience principles

- Dark mode only until the dark interface is complete and visually consistent.
- Local-first and offline by default. Earn that trust through predictable behavior,
  permissions, and documentation instead of promotional status cards in the workspace.
- One primary action per screen.
- Stay a file utility, not an image, video, or document editor. Crop, rotate, flip,
  drawing, filters, timelines, and other composition-editing tools are permanent
  non-goals rather than future backlog items.
- File tools may change encoding, dimensions, compression, metadata, packaging, or
  naming, but they should not alter the visual composition of the source.
- Show controls only when they apply to the active tool and selected file.
- Prefer presets and plain language over codec flags and engine terminology.
- Preserve the source by default and make the output destination predictable.
- Use the same loaded, running, success, error, cancel, and reveal states
  for every tool.
- Use one compact button system: primary actions, secondary actions, selectors,
  text actions, and icon actions share fixed sizing, square corners, typography,
  focus treatment, and disabled behavior. Navigation and file rows remain flat rows.
- Exclusive selector groups expose native radio semantics, keep one Tab stop, and support
  wrapping arrow-key plus Home/End navigation without adding visual motion.
- Drag-and-drop, file browsing, paste, Open With, and keyboard operation are
  first-class input paths.
- A new tool must include validation, cancellation, collision-safe output naming,
  and a native-app smoke check before it is considered complete.

## Current codebase audit

### Working today

- Tauri v2 desktop shell with a React, TypeScript, Tailwind, and Zustand frontend.
- Dark-only, resizable workspace with a persistent tool rail and native macOS window
  behavior.
- A compact intent-aware tool finder filters the grouped rail by utility name, file type,
  and format without flattening its information architecture; `Command-K` focuses it from
  anywhere in the workspace.
- App-wide keyboard actions use one tested resolver: command shortcuts remain available,
  while unmodified Enter and Escape never start, cancel, or clear work when a form control,
  selector, link, or button owns keyboard focus.
- ImageMagick, FFmpeg, Pandoc, LibreOffice, resvg, VTracer, Poppler, Ghostscript, and
  bundled macOS Vision OCR routing with exact per-job capability checks.
- Image, video, audio, document, and SVG format detection with common extension
  aliases.
- Multi-file and recursive folder input through browsing or drag-and-drop for every
  visible workflow: Convert, Resize, Optimize, Export images, Remove Metadata,
  Compress Video, Compress Audio, Remove Audio, Extract Audio, Transcribe, Extract Subtitles, Video Thumbnails,
  Extract Text, Recognize Text, Combine to PDF, PDF Split, PDF Page Export, PDF Compression,
  Create Archive, Extract Archive, Batch Rename, and Inspect. Folder scans skip hidden,
  linked, duplicate, and unsupported entries and respect the 100-item queue limit.
- A real in-memory queue with duplicate prevention, item removal, per-file conversion
  targets, sequential batch processing, per-item status/progress, skip, cancel,
  retry, and aggregate completion results.
- Convert queues expose one compact bulk output selector. Choosing a format updates every
  compatible queued row while leaving incompatible files untouched; per-file targets remain
  available for mixed batches and exceptions.
- PNG and JPEG conversion can create one verified, metadata-free, single-page PDF per source
  through a bundled Quartz/ImageIO helper. PDF remains the last picker option and completed rows
  identify the target without presenting container growth as optimization. Combine to PDF also
  accepts an ordered mix of two to 100 PDF, PNG, and JPEG inputs in one transaction. It retains
  every source PDF page, renders each image as one metadata-free aspect-preserving page, and
  verifies the final PDF signature and exact expected page count before committing the output.
- A bounded local recent-jobs list with setup-aware job loading, legacy source reopen, output Reveal in Finder, and
  clear-history actions. The sidebar keeps a three-row preview while its Activity workspace
  exposes all 12 retained jobs, per-job removal, output handoff, and rename undo without
  widening the tool rail. Recording can be paused and resumed independently of current
  history, so no new file paths are retained while paused. Completed, partial, failed,
  skipped, and cancelled outcomes survive restart; only the bounded safe error category is
  retained, never raw engine diagnostics. Transcription participates in the same persisted
  history contract. New entries also retain a bounded versioned snapshot of the exact reusable
  settings plus compatible per-input conversion targets, page/stream selections, and relative
  paths. Loading claims the workspace before native source revalidation, atomically replaces
  only the destination utility session when at least one source survives, and never auto-runs;
  malformed or older setup data degrades to the existing source-only action.
- Independent sessions for every visible workflow retain uploaded files, queue state,
  settings, and finished results while navigating.
- Asynchronous browse, folder scan, Open With, recent-job reopen, and Finder recipe intake is
  pinned to the operation that initiated it. Switching tools while collection is in flight
  cannot leak validated files into a different queue.
- File intake now has one atomic busy-state contract across native drop, Browse, Choose folder,
  paste, `Command-O`, the asynchronous collector, and the store boundary. A picker or recursive
  scan that finishes after its target queue starts running is discarded instead of mutating the
  live job; independent idle tool sessions retain their own state.
- Completed outputs expose one compact action menu in both the live queue and Recent Jobs.
  Completed multi-result queues expose the same menu once in their header, and `Command-R`
  reveals the complete ordered, stable-deduplicated batch rather than only the final result.
  Outputs can be revealed in Finder or staged as a complete batch in any visible utility that
  accepts every file. Handoffs preserve both workspaces, revalidate files
  through normal intake, respect the queue limit, and never run the destination automatically.
  The menu uses native menu semantics, moves through enabled actions with Arrow keys and
  Home/End, closes with Escape or Tab, and restores focus predictably. Rejections use an alert,
  queue outcomes use one stable polite status region, and determinate or indeterminate work is
  exposed as a progressbar without adding visible copy or decorative motion.
- Escape cancels only an active job. It is deliberately inert for loaded, completed, failed, and
  empty workspaces so a stray key cannot discard queue state or results; clearing remains an
  explicit visible action.
- One classified operation registry now separates output-producing jobs from read-only
  workspaces. Workspace recovery, saved-recipe eligibility, and Recent Jobs derive from
  that registry, with invariants preventing a newly added utility from silently losing
  persistence or history support.
- Shipping UI checks now lock the compact sidebar to 26 px rows, 11 px labels, and 14 px
  icons while rejecting gradients, uppercase/wide-tracked interface text, rounded or elevated
  shipping surfaces, and the removed local/private marketing motif before they can drift back
  in. The same contract limits motion to file intake and progress feedback.
- Versioned local workspace drafts retain uploaded paths and per-tool settings across app
  restarts. Every path is revalidated through the native collector, missing or newly
  unsupported files are dropped, metadata is refreshed, and interrupted jobs return as
  pending rows without retaining stale process IDs, errors, or partial results. Draft size,
  path counts, strings, numeric settings, enums, and stream indexes are bounded before use.
- One atomic workspace-admission lifecycle owns asynchronous restoration, capability preflight,
  and execution. It claims the exact operation and token before awaiting, blocks intake,
  settings, navigation, shortcuts, and automation while restoration or preflight is pending,
  and drops stale continuations instead of starting or completing against another queue.
- A typed `JobRequest` command boundary shared by every output-producing tool, with
  an exact capability preflight before execution.
- Keep-both output commits are no-clobber at the filesystem boundary. Files and extracted
  directories use macOS exclusive atomic rename, recompute a numbered name if another process
  wins the path after preparation, and return the path actually committed. Source and hard-link
  aliases are rejected before commit, while multi-output rollback removes only paths owned by
  the current transaction.
- Availability-aware engine routing that falls back to another compatible installed
  engine and checks secondary requirements such as Tectonic for Pandoc PDF output.
- One frontend conversion matrix imported by the format picker and verified in Rust
  against every advertised target, typed compatibility row, and backend engine route.
- ImageMagick conversion inspects source and candidate topology. Static outputs must match the
  requested logical format and source geometry; multi-frame inputs are preserved only in a
  verified single-file-capable target, while incompatible targets are rejected before execution.
  Conversion runs inside a same-parent hidden workspace so numbered ImageMagick sidecars cannot
  leak beside user files.
- A packaged debug app launched with only `/usr/bin:/bin` on `PATH` completed one
  mixed Convert queue covering MP4 to MOV, SVG to JPEG, Markdown to PDF, PNG to JPEG,
  and WAV to MP3. Independent inspection verified the expected media streams and
  durations, 320 × 200 raster dimensions, extracted PDF text, retained sources, and
  no hidden partial outputs.
- Shared output controls for every shipping tool: source or custom folder, per-tool
  filename suffix, automatic keep-both naming, preserved relative folders where
  applicable, and hard source-overwrite protection.
- Output folder and per-tool suffixes persist locally across launches.
- Versioned, operation-specific saved recipes capture the relevant processing choices,
  destination folder, and filename suffix without retaining file-specific stream choices or
  PDF page ranges. Recipes are validated, bounded, removable, and migrate both the earlier
  raw-array and versioned output-only preset shapes. Frontend checks cover round-trip,
  migration, matching, malformed settings, limits, capture, and application.
- Any saved recipe can be deliberately added to or removed from Finder Quick Actions from
  the compact recipe control. The generated macOS service safely transports the selected
  batch, survives cold launches, restores the exact current recipe, and stages an isolated
  queue for the existing explicit preflight and run action. Deleting a recipe also removes
  its Finder workflow.
- Every progress event carries the exact producing job ID from its active-job guard through
  FFmpeg readers, PDF/OCR helpers, archive workers, rename callbacks, and native helpers.
  The frontend accepts progress only for the currently active ID, cancellation fallbacks can
  terminalize only that same job, and success/error/cancel transitions clear all active
  identity and running-row state. Deterministic race checks cover cancelling job A and starting
  job B before A's fallback fires; terminal errors expose retry and reset actions.
- Determinate FFmpeg progress and indeterminate progress for other engines.
- Cancellation tokens, timeouts, partial-output cleanup, and structured errors.
- One bounded subprocess runner now owns quiet-process cancellation, timeouts, complete
  stderr draining with a retained 64 KiB tail, exit-code errors, and partial-output cleanup
  for ImageMagick conversion/resize/optimization/export, resvg, VTracer, Pandoc,
  and LibreOffice.
- Progress-aware FFmpeg video/audio jobs and PDF/text runners reuse the same bounded pipe
  capture, mark children for termination if their owning future is dropped, join pipe
  readers on every exit path, and retain their specialized progress and verification.
- One shared output-verification contract now rejects and removes missing, non-file, and
  zero-byte results before any temporary output can be committed. Exact signature, codec,
  stream, dimensions, cue, topology, and format checks remain layered on top where the
  operation requires stronger proof.
- Dependency detection that also searches common Homebrew and Cargo locations.
- Drag-and-drop, filtered file browsing, image paste, and batch Finder Open With. Launch-time
  paths are buffered until the frontend listener is ready, routed as one set to the current
  or best matching utility, bounded and deduplicated, and deferred during an active job.
  The packaged macOS document types register ZIP, TAR, 7Z, GZIP, and TGZ as Viewer associations
  so Finder can hand every supported archive type to the existing Extract archive workflow.
- Clipboard routing is isolated from form editing: ordinary text paste remains inside inputs,
  only actual image clipboard items reach the workspace handler, eligible operations are an
  explicit typed policy, binary encoding is chunked, and the native command rejects decoded
  images larger than 64 MiB before writing a managed temporary file.
- Unique managed clipboard inputs, stale/exit cleanup, and automatic Downloads output
  routing so pasted-image results do not disappear into a temporary directory.
- Image, video, PDF, and document thumbnails.
- Output size comparison and Reveal in Finder on completion.
- Logging and fallback output to `~/Downloads/ConvertKit` when the source directory
  is read-only.
- A guarded macOS release script accepts only 1Password `op://` references, materializes
  the App Store Connect key in a mode-0600 temporary directory, delegates Developer ID
  signing/notarization/stapling to Tauri, verifies the resulting app and DMG with Apple
  tooling, and removes the temporary key on every exit path.
- A local Apple Silicon package gate builds the explicit `aarch64-apple-darwin` debug app,
  verifies every bundled executable is arm64, applies and validates an ad-hoc signature,
  audits the bundled Whisper runtime, and exercises quality and target-size video, audio
  compression, exact target-size image optimization, image-to-PDF, Combine to PDF, OCR,
  PDF compression and metadata removal, archive creation/extraction, Inspect with checksum
  manifests, and Batch Rename through the packaged request boundary. The credentialed macOS
  release command invokes this gate first. GitHub CI does not claim this local Apple Silicon
  coverage.
- Resize with original-dimension probing, ratio locking, percentage presets, and
  batch execution. Before commit, a bounded shared image probe verifies the source's filename
  matches its contents and the result keeps the same logical format, exact frame/page count,
  and expected dimensions for every frame. Real-engine coverage includes every advertised
  format—JPEG, PNG, WebP, GIF, HEIC, TIFF, BMP, AVIF, and ICO—plus animated GIF, multi-page
  TIFF, and multi-frame ICO topology. A packaged app launched with a minimal `PATH` completed
  one 50% queue containing JPEG, PNG, WebP, GIF, HEIC, and TIFF, with every output verified at
  320 × 240 and every source retained. A separate 14,000 × 14,000 packaged-app job was cancelled
  during processing with no output, hidden partial, or surviving child; automated tests also
  enforce timeout cleanup.
- Image optimization with metadata removal by default, an explicit metadata policy,
  multiple candidate encodes, before/after size reporting, source preservation, cancellation,
  and collision-safe output naming. The source is the baseline: every candidate must remain a
  readable image with the same logical format, exact frame/page count, and per-frame dimensions,
  and only the smallest verified candidate that is strictly smaller can be committed. If none
  qualifies, no derivative is written and the queue reports `Already optimized` against the
  unchanged source instead of claiming zero or negative savings.
- JPEG, WebP, AVIF, and HEIC optimization can instead target a whole-KiB file size from 16 KiB
  through 102,400 KiB, always below the source size. Descending lossy-quality candidates use the
  same typed topology verifier and exact byte ceiling; exhaustion reports `TargetSizeUnreachable`.
  A packaged minimal-`PATH` proof held JPEG and WebP below 256 KiB with unchanged format,
  dimensions, frame count, and source hashes, exercised numbered keep-both output, and proved
  typed unreachable-target and cancellation cleanup without committed or hidden partial output.
- Optional OxiPNG, jpegoptim, and Gifsicle candidates for lossless format-specific
  optimization, with ImageMagick retained as the always-available fallback.
- Explicit batch image export recipes that combine proportional downscaling, format
  conversion, compression, and metadata removal without editor controls. Web, Email,
  Social, and Preview variants can be selected together; each source produces a
  collision-safe output set that is rolled back on failure or cancellation. Every output
  path is retained for Recent Jobs and multi-file Finder reveal. Animated and multi-page
  sources are rejected instead of being flattened implicitly.
- Focused batch video compression through Compatible H.264/AAC MP4, Smaller H.265/AAC MP4,
  Web VP9/Opus WebM, and Lossless FFV1/FLAC MKV presets. Auto, Original, 1080p, and 720p are
  maximum-resolution bounds that never upscale. Quality remains the default goal; the three
  lossy presets can instead target a whole-megabyte size from 1 MiB through 100 GiB. Target
  jobs use measured duration and audio-aware bitrate budgets, verified two-pass FFmpeg
  encoding, and one bounded retry before committing only an output at or below the requested
  size. Quality and target-size results share a typed verifier for container, video codec,
  positive dimensions, exactly one primary audio stream when present with its preset codec,
  and source-duration parity before target-size jobs apply the exact byte ceiling. Lossless
  rejects file-size targeting. The goal and target are retained per tool and across restarts,
  participate in saved recipes, and use the existing batch, cancellation,
  cleanup, Recent Jobs, and Finder handoff workflows.
- Focused batch audio extraction from video to MP3, M4A, WAV, or FLAC through FFmpeg.
  Every embedded audio stream is inspected first, each queued video retains its selected
  language or mix, and the backend revalidates the exact global stream index and codec
  before mapping only that track. Metadata includes title, language, codec, channel layout,
  sample rate, and default disposition. The workflow retains settings, uses exact dependency
  preflight, shared saved recipes, progress, cancellation, recent history, and Finder reveal.
  Native checks cover all four formats, recursive folder filtering, retained tab state,
  cancellation cleanup, and a packaged MP3 job launched with a minimal `PATH`; an automated
  real-media test isolates a non-default track from a two-track MKV and preserves the source.
- Focused batch audio compression to AAC/M4A through High 256 kbps, Balanced 160 kbps, and
  Smallest 96 kbps presets. Outputs retain source duration, channel count, and supported sample
  rate and are committed only when strictly smaller than the source; otherwise the typed
  `OutputNotSmaller` result leaves the source as the useful result. The shared retained queue,
  output, progress, cancellation, Recent Jobs, and Finder handoff contracts apply. The packaged
  Apple Silicon smoke covers every preset, source preservation, keep-both naming, an already-
  compact source, and cancellation and hidden-partial cleanup under a minimal launch `PATH`.
- Focused batch audio removal from video through FFmpeg stream copy, preserving the
  original encoded video while producing a source-container `-silent` copy. It has
  recursive folder input, retained tab state, shared saved recipes, exact FFmpeg and
  FFprobe preflight, progress, cancellation, output stream verification, recent history,
  and Finder reveal. Packaged MP4 and MOV checks confirmed matching video-stream hashes,
  dimensions, codecs, and durations with no output audio; a cancelled 557 MB job left no
  output, hidden partial, or child process.
- Local transcription with separately verified Tiny, Base, and Small models plus a pinned
  Apple Silicon runtime bundled inside the app. The staging gate verifies whisper.cpp 1.9.2,
  GGML 0.22.0, OpenMP 22.1.8, the complete CPU/Metal/BLAS backend set, matching arm64
  architecture, and upstream license files before rewriting every native load path. A
  clean-environment audit loaded only bundle-contained code, and a typed packaged-app job
  produced the expected transcript with an unchanged source hash and no surviving child.
- General batch metadata removal for JPEG, PNG, WebP, PDF, audio, and video, with recursive
  folder input, retained tab state, shared output controls, progress, recent history, and
  Finder reveal. The built-in image path preserves encoded pixels and ICC profiles;
  FFmpeg stream-copy removes media tags and chapters, then verifies identical stream
  topology/codecs and absent sensitive tags. The built-in PDF path performs a full structural
  rewrite rather than rendering pages: it removes document information, XMP, piece-info, old
  IDs, and incremental-history remnants; rejects encrypted or digitally signed inputs; and
  verifies unchanged page geometry and every reachable non-metadata object before commit.
- Focused batch text extraction from PDF, DOCX, HTML, Markdown, and EPUB into UTF-8
  plain-text copies. It routes PDFs through Poppler and documents through Pandoc, with
  exact dependency preflight, recursive folder input, relative output folders, retained
  tab state, cancellation, source preservation, recent history, and Finder reveal.
- Local batch OCR for common raster images and scanned PDFs through a bundled macOS Vision
  helper, with retained Text file and Searchable PDF output modes. Searchable output preserves
  the visible scan, page count, page order, and displayed geometry while adding an invisible
  selectable text layer. Both modes use accurate recognition, language correction, automatic
  language detection where supported, a 250-page bound, a 32 MiB text bound, retained tab
  state, saved recipes, cancellation, recent history, source preservation,
  and Finder reveal. Packaged-app checks under a minimal `PATH` verified the legacy text
  default, image and multipage searchable PDFs, extractable text order, pixel-identical scan
  rendering, keep-both naming, and unchanged source hashes; cancelling a 120-page job left no
  output, hidden partial, workspace, renderer, or OCR helper process.
- Ordered Combine to PDF through Poppler with mixed PDF, PNG, and JPEG multi-file/folder input,
  duplicate prevention, retained tab state, exact dependency preflight, cancellation, timeout,
  safe naming, recent-job history, and Reveal in Finder. PNG and JPEG inputs use the bundled
  image-to-PDF helper in an isolated temporary workspace; existing PDF inputs retain all pages.
  Frontend and Rust tests cover mixed intake, retained ordering, unsupported input rejection,
  and image-aware dependency selection. A packaged-app smoke combines a PNG, a two-page PDF,
  and a JPEG; it verifies the four rendered pages in order, unchanged source hashes, removed
  private image comments, numbered keep-both output, and no hidden partial files. The existing
  native smoke also combines representative 1-page and 67-page PDF inputs into one verified
  68-page PDF.
- Batched PDF splitting through Poppler, with one-file-per-page output or ordered page
  selection syntax such as `1-3, 5, 8-10`. It has retained settings, exact dependency
  checks, cancellation, atomic output commits, recent history, and Finder reveal.
- Batched PDF page export through Poppler, with every-page or ordered page selection,
  PNG and JPEG output, and focused 144 DPI Screen or 300 DPI Print resolution presets.
  Each PDF produces a deterministic, rollback-safe output set with retained settings,
  exact dependency checks, progress, cancellation, recent history, and Finder reveal.
- Batched PDF compression through Ghostscript with mutually exclusive Quality and File size
  goals. Quality retains High, Balanced, and Smallest presets and preserves the source bytes
  if recompression would grow the PDF. File size accepts a whole-MiB target from 1 MiB through
  100 GiB, requires it to be below every queued source, and runs a deterministic ten-step
  resolution/quality search. Both goals require a readable candidate whose page count, rotation,
  and effective Media/Crop/Bleed/Trim/Art boxes match the source; File size also requires the
  candidate to remain below the exact ceiling. The selected
  goal survives tool switches and restarts and participates in saved recipes. A minimal-`PATH`
  packaged smoke reduced a 5,780,211-byte, 13-page mixed raster/vector
  PDF to 760,081 bytes under a 1 MiB ceiling, verified every page box, produced a numbered
  keep-both copy, preserved source hashes, and proved cancellation and unreachable targets
  leave no output, hidden partial, or Ghostscript child.
- Built-in ZIP, 7Z, TAR, TAR.GZ, and single-file GZIP creation plus ZIP, 7Z, TAR, TAR.GZ, TGZ, and
  standalone GZIP extraction with multi-file/folder input, preserved relative paths, a retained format choice,
  batch extraction,
  cancellation, progress, collision-safe atomic outputs, recent history, and Finder reveal.
  Extraction rejects traversal, links, duplicate paths, TAR special files, and unsafe 7Z
  anti/link/special entries while bounding entry count and expanded size. ZIP entries also use
  exclusive creation so Unicode-normalization or other filesystem aliases cannot overwrite an
  earlier extracted path. 7Z creation streams archives, optionally encrypts file contents and
  headers with in-process AES-256, preserves relative paths, and verifies its exact manifest and
  fully decoded file bytes before atomic commit. Passwords are bounded, redacted, zeroized after
  the request, retained only in the current in-memory tool session, and excluded from recovery,
  Activity and recipes. GZIP
  creation accepts exactly one directly
  selected regular file, streams with cancellation, preserves its original extension, and fully
  decodes and byte-compares the candidate before atomic commit. Plain `.gz`
  input decompresses to one source-named file while `.tar.gz` remains a folder-producing TAR.GZ
  archive. Native smoke tests create and extract
  nested ZIP and TAR archives and verify every extracted file byte-for-byte. The rebuilt
  packaged app also extracts the TAR fixture with collision-safe naming and identical SHA-256
  hashes. A signed packaged-app smoke also creates and extracts a nested TAR.GZ, byte-compares
  both extracted files to their sources, verifies source hashes, and confirms that no hidden
  partial output remains. The dedicated packaged 7Z smoke extracts nested bytes under a minimal
  `PATH`, verifies keep-both naming and source hashes, and covers traversal, password-required,
  damaged, and cancelled inputs. Its paired creation smoke builds and re-extracts an AES-256
  encrypted nested archive through the packaged writer and reader, rejects missing and incorrect
  passwords, compares every byte, and verifies keep-both, cancellation, source preservation, and
  partial cleanup without a system `7z` command.
  The dedicated standalone GZIP smoke uses embedded streams under a minimal `PATH`, verifies
  exact single-file bytes and source hashes, exercises numbered keep-both output, and proves
  truncated input, a bad CRC, and cancellation leave no finished or hidden partial output.
  The paired creation smoke independently decodes the packaged output and verifies exact bytes,
  extension-preserving and numbered names, invalid multi-file rejection, source hashes,
  cancellation, and partial cleanup.
  The shared reveal boundary accepts verified regular files and output directories while still
  rejecting missing paths, links, special files, duplicates, and over-100 requests.
- Read-only multi-file inspection for arbitrary file types, with native type, byte size,
  image dimensions, creation/modification dates, read-only status, full path, Reveal in Finder,
  and copyable metadata. MD5, SHA-1, and SHA-256 are computed in one cancellable streaming
  pass and checksum output has been compared against macOS system tools. Pasted checksums in
  raw, labeled, BSD, or manifest form can be verified against a fresh hash pass so cached
  results never produce a false match. Bounded optional
  probes add PDF page information and media container, duration, bit rate, codec, video,
  and audio details without blocking the basic inspector. Multi-file reports export as
  versioned JSON or flat CSV through a native save dialog and atomic write. A packaged smoke
  verifies exact file metadata, all three hashes, JSON report fields, source preservation, and
  partial cleanup under a minimal `PATH`.
- Standard SHA-256 manifest creation and MD5/SHA-1/SHA-256 manifest verification live inside
  Inspect rather than adding another tool. Creation accepts one same-root queue, writes
  collision-safe `checksums.sha256` output atomically, and revalidates each source after hashing.
  Verification resolves only bounded safe relative regular files and reports match, mismatch,
  and missing outcomes; malformed, oversized, duplicate, absolute, traversal, linked, and special
  entries are rejected. The work is sequential, cancellable, retained across tool switches, and
  covered by a dependency-contained packaged smoke in the Apple Silicon gate.
- Transactional batch rename for arbitrary regular files, with deterministic find/replace,
  prefix, suffix, and numbering previews. Extensions are preserved, conflicts are rejected,
  every batch requires confirmation, and app-managed undo records remain available from
  both the result and recent-jobs views for 30 days. The native smoke test renamed two
  files, verified their SHA-256 hashes were unchanged, retained the plan across navigation,
  and restored both original names through Undo. A packaged smoke additionally proves ordered
  output paths, collision and extension-change rejection, deterministic cancellation and
  staging-failure rollback, undo-record removal, and byte preservation.
- Native 1,100 × 760 visual reviews now cover mixed-file Convert queues and their bulk output
  selector, the focused video controls, Export images, Optimize, Compress PDF, Export PDF
  pages, and Transcribe loaded states. A development-only
  fixture route makes those states deterministic without shipping mock data. The review found
  and fixed WKWebView's rounded glossy fallback for transcription dropdowns; every select now
  opts out of native appearance, and standalone selects share one square dark treatment and
  subdued chevron. All reviewed panels retain their footer action and an independently
  scrollable queue/settings region at the default window size.
- Queue rows are container-responsive rather than tied to the window width. At the default
  1,100-pixel window and the 900-pixel minimum, filenames keep their own readable row while
  format and action controls wrap below without horizontal overflow; Convert and ordered
  Merge PDF fixtures were measured at both widths.
- The shared workspace shell now exposes a thin scrollbar when the grouped tool rail exceeds
  the window height, keeps operation-setting headings at one visual tier, uses the destructive
  action role for model deletion, and stacks saved-recipe naming actions so the input remains
  usable at the minimum queue width. Unused alternate progress and result components were
  removed so those states cannot drift from the live queue implementation.

### Legacy-state cleanup

App Icon Package, Normalize Audio, and Duplicate Finder are removed from the product and runtime.
Legacy workspace drafts, recipes, and Recent Jobs are filtered through the centralized active-
operation policy. A retry-safe startup migration also removes the Finder Quick Action previously
managed by retired recipes before committing sanitized state.

### Current product checkpoint

- The planned file-utility surface and shared dark dashboard are feature-complete for this
  checkpoint. Loaded-state visual fixtures now cover all 22 active tools and are checked
  against the centralized sidebar registry so a newly exposed utility cannot silently miss
  visual QA coverage. A browser review at 1,280 × 720 confirmed the compact density, square
  controls, readable queue rows, shared dot treatment, and scroll-safe workspace layout across
  General, Images, Video & audio, PDF & documents, and Organize.

### Deferred, non-blocking distribution work

- Public universal distribution is deferred. The current verified support claim remains Apple
  Silicon only.

### Important gaps

- Workspace inputs and settings recover after restart, but an active subprocess cannot
  resume from its previous byte or frame. Interrupted rows restart as new jobs. Completed,
  partial, failed, skipped, and cancelled outcomes remain in Recent Jobs, but interrupted
  work restarts from the beginning rather than resuming at a previous byte or frame.
- A debug macOS app bundle builds cleanly; interactive release smoke testing remains.
- The signed/notarized release path is scripted but still needs a real credentialed run
  and clean-machine Gatekeeper check. Secure update delivery is intentionally not enabled
  until a dedicated Tauri updater keypair exists: the public key must be compiled into the
  app and the private key retained through 1Password-backed release tooling before signed
  updater artifacts and GitHub `latest.json` can be published.
- `ConvertKit` is a high-risk public name collision, not a cleared release name. Kit, Inc.
  still identifies its active software business as [formerly ConvertKit](https://kit.com/),
  and the [USPTO recommends a comprehensive clearance search](https://www.uspto.gov/TrademarkBasicsToolkit)
  for confusingly similar software marks. Choose a new name or obtain qualified clearance
  before public distribution; the current `com.dropforge.convertkit` identifier is provisional.
## Roadmap

### Phase 0 — Make conversion trustworthy

Status: complete for the current compatibility matrix. Re-open this phase whenever a
new advertised conversion route is added.

- [x] Normalize common extension aliases in Rust and TypeScript.
- [x] Remove advertised PDF input conversions that have no reliable engine.
- [x] Resolve executables consistently in terminal and packaged-app environments.
- [x] Expose a backend capability query for an exact operation and input/output pair.
- [x] Show missing optional tools before the user starts a job.
- [x] Drain subprocess output concurrently for long-running engines.
- [x] Complete a packaged-app mixed-format smoke pass with a minimal `PATH`, covering
      image, vector, video, audio, and document routes with independent output checks.
- [x] Rewrite format documentation from the verified compatibility matrix.
- [x] Add source-preserving PNG/JPEG to single-page PDF conversion with a bundled macOS helper,
      exact type and page validation, metadata stripping, output handoff, and packaged smoke.

Exit criteria: every option shown in the UI has a working engine or a clear,
pre-flight dependency message, and representative conversions pass a native-app
smoke check.

### Phase 1 — Workspace shell and Resize

Status: complete for the current workspace shell and Resize scope.

- [x] Replace the two-tab layout with a tool rail that can scale to more workflows.
- [x] Centralize grouped navigation and verify that every registered operation appears
      exactly once in the visible tool rail or the explicit hidden-operation list.
- [x] Add a compact `Command-K` tool finder that filters the grouped rail by utility intent,
      category, or format while preserving section and operation order.
- [x] Standardize primary, secondary, selector, text, and icon controls through shared
      square-cornered button roles instead of component-specific styling.
- [x] Give every exclusive selector one shared keyboard contract with roving Tab focus,
      arrow-key wrapping, Home/End navigation, and visible focus treatment.
- [x] Guard app-wide Enter and Escape actions from interactive controls so form submission,
      selector use, and tool search cannot accidentally run or clear a queued job.
- [x] Refresh the interface with one dark theme, clearer hierarchy, and a larger,
      resizable workspace.
- [x] Encode the approved visual exclusions and compact sidebar measurements as automated
      design-system checks instead of relying on repeated manual corrections.
- [x] Filter Resize input to raster images.
- [x] Read original image dimensions and initialize the resize form from them.
- [x] Add locked-ratio editing and 25%, 50%, 75%, and 100% presets.
- [x] Preserve the original and create a collision-safe resized copy.
- [x] Reuse conversion progress, cancellation, result, and Finder flows.
- [x] Verify the source content and every Resize output before commit: same logical format,
      exact frame/page count, and exact requested or aspect-fit dimensions for every frame.
- [x] Exercise every advertised raster format plus animated GIF, multi-page TIFF, and
      multi-frame ICO with the real image engine.
- [x] Run a packaged-app Resize queue for JPEG, PNG, WebP, GIF, HEIC, and TIFF under
      a minimal `PATH`, then verify exact output dimensions and retained sources.
- [x] Cancel a 14,000 × 14,000 packaged-app resize during processing and verify no
      output, hidden partial, or surviving ImageMagick process; cover timeout cleanup too.

Exit criteria: Resize feels like a complete tool rather than a special conversion
case and works from drag, browse, paste, Open With, keyboard, success, and error
paths.

### Phase 2 — Image file utilities

Build the most common local image workflows before moving to another domain.

#### Optimize

- [x] Add a simple Optimize workflow to the tool rail.
- [x] Produce multiple format-aware candidates and keep the smallest valid result.
- [x] Remove metadata by default with a clear Keep option.
- [x] Show before/after file size and percentage saved.
- [x] Preserve the source and use collision-safe output naming.
- [x] Measure Best, Balanced, and Smallest-style lossy quality levels against photo,
      interface, and icon fixtures. Keep Optimize as one high-quality action: lower JPEG
      and WebP levels saved substantially more on photographs but degraded interface text
      and hard icon edges disproportionately, while PNG, GIF, and TIFF have no equivalent
      quality tier. Delivery-specific tradeoffs remain in Export images instead of a
      misleading cross-format selector.
- [x] Add specialized optimizers as optional candidates without making them required
      dependencies or exposing engine choices in the interface.
- [x] Verify logical format, exact frame/page count, and per-frame dimensions before commit;
      keep the source as the baseline, require a strictly smaller winner, and surface an honest
      `Already optimized` result without creating a duplicate when no candidate qualifies.
- [x] Add a mutually exclusive exact file-size goal for JPEG, WebP, AVIF, and HEIC, bounded to
      whole-KiB targets from 16 KiB through 102,400 KiB and below every source. Verify every
      candidate with the shared image-structure probe, return a typed unreachable-target error,
      retain state in drafts, recipes, and Activity setup, and package-test JPEG,
      WebP, keep-both naming, source preservation, cancellation, and partial cleanup through the
      debug request boundary under a minimal `PATH`.

#### File-oriented image workflows

- [x] Combine conversion, proportional resizing, optimization, and metadata removal in one
      explicit Export images recipe rather than hiding those changes in another tool.
- [x] Add practical Web, Email, Social, and Preview output variants with fixed,
      inspectable format, dimension, and quality behavior.
- [x] Batch metadata removal for images, PDFs, audio, and video without exposing editor-style
      controls. PDF cleanup preserves document content and interactive structure and is not
      presented as redaction of visible text, form values, comments, or attachments.
- [x] Batch metadata inspection and JSON/CSV export through Inspect Files.
- [x] Create multiple selected variants from each source as one rollback-safe output set.

Crop, rotate, flip, filters, drawing, and other pixel-editing features are
intentionally out of scope. They belong in an image editor, not this file utility.
Resize remains in scope because it changes an output file's dimensions for practical
delivery needs; it must stay a form-based batch utility and never grow into a canvas,
viewport, or composition-editing surface.

Exit criteria: image tools share one request/result model and can be chained without
overwriting the source.

### Phase 3 — Shared jobs and batch work

This phase is the platform for every later toolbox area.

- [x] Introduce a typed `JobRequest` with operation-specific settings.
- [x] Centralize validation, capability checks, output policy, progress, cancellation,
      errors, and cleanup.
- [x] Centralize active-job registration and guaranteed removal across current tools.
- [x] Centralize the lifecycle of quiet subprocesses so image and document tools cannot
      diverge on cancellation, timeout, stderr draining, or partial-output cleanup.
- [x] Add multi-file browse and drag-and-drop input.
- [x] Keep text paste local to form controls and bound workspace image-paste payloads across
      both the frontend selection policy and native temporary-file boundary.
- [x] Add a basic in-memory queue with removal and duplicate prevention.
- [x] Add per-file conversion targets and sequential batch execution.
- [x] Add aggregate batch completion results.
- [x] Bound the queue at 100 files and show explicit overflow feedback.
- [x] Support recursive folder browse and drop where a tool allows them, with hidden
      and unsupported filtering, deduplication, queue bounds, and relative output paths.
- [x] Add per-item progress, retry, skip, cancel, and aggregate status.
- [x] Scope progress, cancellation fallbacks, and terminal cleanup to the exact job identity so
      stale work from a cancelled job cannot mutate a subsequently started job.
- [x] Retain each tool's uploaded files, queue state, settings, and results while
      switching between sidebar tabs.
- [x] Persist a bounded, versioned workspace draft across app restarts, revalidate every
      restored path natively, and normalize interrupted work back to a safe pending queue.
- [x] Claim restoration and job preflight with an operation-scoped token before awaiting so
      input, settings, navigation, automation, or stale continuations cannot mutate or start
      work in the wrong retained session.
- [x] Persist a small recent-jobs list with reopen, reveal, and clear actions, including
      completed, partial, failed, skipped, and cancelled outcomes without retaining raw
      engine diagnostics.
- [x] Add output folder, suffix, automatic keep-both naming, and source-overwrite
      safeguards.
- [x] Persist lightweight output preferences locally across app launches.
- [x] Add reusable output presets for destination folders and filename suffixes, scoped to
      the active tool and stored in a validated versioned schema. This foundation now
      migrates into full saved recipes without losing existing user data.

Exit criteria: a new tool can plug into one job pipeline without rebuilding lifecycle
logic or UI states.

### Phase 4 — PDF and document tools

Ship narrow, dependable tools rather than a miniature document editor.

- [x] Combine ordered PDF, PNG, and JPEG inputs into one verified PDF through the shared job
      workflow, with one metadata-free page per image and all existing PDF pages retained.
- [x] Split every page into its own PDF or extract selected pages into a new PDF.
- [x] Export every or selected PDF pages to PNG or JPEG with focused screen and print
      resolution presets.
- [x] Compress PDFs in batches with three quality presets, never keep a larger result, and
      verify page count, rotation, and every effective page box before commit.
- [x] Add a mutually exclusive exact file-size goal to Compress PDF with bounded attempts,
      page-topology verification, a clear unreachable-target error, and no committed or hidden
      partial output on failure or cancellation.
- [x] Convert supported documents through Pandoc and LibreOffice.
- [x] Extract readable text from PDF, DOCX, HTML, Markdown, and EPUB in retained batch
      queues with exact Poppler/Pandoc preflight and source-preserving outputs.
- [x] Inspect and export document metadata through the shared Inspect Files workflow.
- [x] Recognize text in common raster images and scanned PDFs through bundled macOS Vision,
      with automatic language detection where supported and no downloadable model or API.
- [x] Add retained Text file and Searchable PDF OCR outputs. Preserve each scan's visible
      appearance and page order, add a geometry-aware invisible text layer, migrate legacy
      recipes to Text, and carry the selected mode through batches, drafts,
      Finder recipes, Recent Jobs, and output handoff.

PDF-to-DOCX must not be advertised until a reliable implementation is chosen and
tested. Conversion and page manipulation are separate tools.

### Phase 5 — Video and audio tools

- [x] Extract audio from video in batches to MP3, M4A, WAV, or FLAC, with per-file
      embedded-track discovery, retained selection, and exact stream validation.
- [x] Compress audio in batches to verified AAC/M4A with High, Balanced, and Smallest presets,
      preserve duration, channel count, and supported sample rate, and commit only a strictly
      smaller result. The Apple Silicon package gate covers all presets, keep-both naming,
      `OutputNotSmaller`, source preservation, and cancellation cleanup.
- [x] Add focused batch encoding presets: Compatible H.264/AAC MP4, Smaller
      H.265/AAC MP4 up to 1080p, Web VP9/Opus WebM up to 1080p, and lossless FFV1/FLAC MKV.
- [x] Add focused maximum-resolution controls (Auto, Original, 1080p, and 720p) plus
      codec-appropriate High, Balanced, and Smallest quality settings without upscaling.
      A packaged minimal-`PATH` quality smoke covers all four presets, duration/audio/container
      verification, source preservation, keep-both output, and cancellation cleanup.
- [x] Add a mutually exclusive file-size goal for Compatible, Smaller, and Web, bounded from
      1 MiB through 100 GiB, with two-pass encoding, one bounded overshoot retry, exact output
      verification, retained state, and saved recipes. Lossless stays
      on its fixed quality path. Packaged H.264, H.265, and VP9 target-size jobs all stayed
      below an exact 2 MiB ceiling; keep-both, source preservation, full decode, and cancellation
      cleanup were also verified.
- Consider frame-rate and VideoToolbox acceleration controls only after representative
  delivery measurements show that they solve a real file workflow.
- [x] Remove audio without re-encoding the video stream.
- [x] Remove private audio/video tags and chapters through verified stream-copy without
      changing encoded streams or creating a separate metadata tool.
- Add focused audio-format utilities beyond Convert only when they solve a distinct
  file workflow.
- [x] Generate one midpoint thumbnail or one fixed 4 × 3 contact sheet per video as
      JPEG or PNG, with batch and folder input. Arbitrary frame picking stays deferred so
      the tool does not grow into a timeline or editor.
- [x] Extract embedded text subtitle tracks to SRT or WebVTT after stream inspection,
      with per-file track selection and language, title, default, and forced metadata.
      Image-based tracks are reported but not offered as text output. Subtitle burn-in is
      intentionally out of scope because it changes the video's visual composition rather
      than producing a focused derived file.

Avoid recreating HandBrake. Advanced flags belong behind an explicit disclosure and
must map to typed settings. Do not add rotate, flip, crop, filters, or timeline tools;
video work stays focused on encoding, streams, metadata, and derived file outputs.

### Phase 6 — Inspect, archives, and organization

- [x] Add a unified, read-only file inspector with filesystem metadata and image
      dimensions for arbitrary file types.
- [x] Copy technical metadata and compute MD5, SHA-1, and SHA-256 in one cancellable
      streaming pass.
- [x] Verify pasted MD5, SHA-1, or SHA-256 values and common checksum-manifest lines
      against a fresh cancellable hash pass.
- [x] Create standard SHA-256 manifests for same-root Inspect queues and batch-verify imported
      MD5, SHA-1, or SHA-256 manifests with safe relative path resolution, per-entry outcomes,
      retained progress/results, cancellation, source preservation, and atomic keep-both output.
- [x] Add bounded format-specific details for media codecs/duration and PDF page count.
- [x] Export multi-file inspection reports as JSON or CSV, including technical details
      and any checksums already computed.
- [x] Create ZIP, 7Z, TAR, or TAR.GZ archives from files or recursively collected folders while
      preserving relative paths; verify 7Z manifests and decoded bytes before commit; create a
      verified standalone GZIP stream from exactly one directly selected regular file. Support
      request-only AES-256 passwords for 7Z content and headers without persisting secrets.
- [x] Extract ZIP, 7Z, TAR, TAR.GZ, TGZ, or standalone GZIP input with traversal/link rejection,
      expansion limits, atomic folder or file commits, collision-safe naming, and exclusive ZIP
      entry creation that rejects filesystem-equivalent aliases before they can overwrite data.
- [x] Keep plain GZIP decompression distinct from gzip-compressed TAR routing: `.gz` produces
      one source-named file while `.tar.gz` uses the TAR path, link, special-entry,
      duplicate-path, entry-count, and expansion safety contract.
- [x] Batch rename with deterministic preview, explicit confirmation, collision-safe
      staging, and an expiring undo manifest.

Destructive actions require previews, explicit confirmation, and a recoverable path.

### Phase 7 — Transcription and automation

#### Local transcription

- [x] FFmpeg preprocessing to 16-bit mono 16 kHz WAV.
- [x] Resolve a locally installed `whisper-cli` with exact capability feedback.
- [x] Bundle and load-audit a pinned, licensed arm64 Whisper/GGML/OpenMP runtime, with all
      native dependencies and backends resolved inside the signed app bundle.
- [x] Manage separately downloaded, size- and SHA-1-verified Tiny, Base, and Small models.
- [x] Language auto-detection plus explicit selection.
- [x] TXT, SRT, and VTT output.
- [x] Download size/state, model deletion, retained settings, saved recipes, elapsed time,
      output reveal, and cancellation.
- [x] Complete representative packaged-app Tiny-model TXT, SRT, and VTT transcription plus
      cancellation smoke passes under a minimal `PATH`, with independent content, cue,
      source-integrity, partial-output, and child-process checks.

#### Automation

- [x] Saved recipes that restore validated operation settings, destination, and naming while
      deliberately excluding input-specific page and stream selections.
- [x] Batch Finder Open With with launch-safe buffering and intent-aware tool routing.
- [x] Finder Quick Actions for deliberately chosen saved recipes, with launch-safe request
      buffering and an explicit in-app preflight/run step.
- Optional command-line entry point backed by the same job model.

Automation comes last because it amplifies both good and bad behavior. It should only
run tools whose single and batch flows are already dependable.

## Architecture direction

### Frontend

- Keep the tool registry centralized with label, description, accepted inputs,
  dependency requirements, and route/component metadata.
- Classify every registered operation exactly once as an output job or a read-only
  workspace. Derive persistence and history capability from that classification; keep
  navigation and Open With lists explicit because they represent narrower
  product-policy subsets.
- Keep shared lifecycle state separate from operation settings.
- Continue moving from the monolithic store toward shared job state plus small
  operation-specific slices as more tools are introduced.
- Do not model Optimize, Transcribe, Inspect, or PDF actions as output formats.
- Keep the shared queue runner and session model operation-agnostic as the toolbox
  expands.

### Rust

- Keep subprocess construction and execution in Rust.
- Keep executable resolution and quiet-process lifecycle in shared helpers. Extend the
  same primitives through specialized progress-aware runners without discarding useful
  progress reporting or format-specific verification.
- Keep engine flags inside typed request structs.
- Treat capabilities as `(operation, input, settings, output)` rather than "tool is
  installed."
- Keep native helpers target-specific, bundled, bounded, cancellable, and behind the same
  typed capability and output-verification boundaries as external engines.
- Use safe temporary directories for multi-step jobs and move the verified result to
  its final destination atomically where possible.
- Never build shell command strings from user-controlled paths.

### Data and privacy

- No analytics, uploads, or cloud dependency by default.
- Recent jobs store paths and settings locally and can be disabled/cleared.
- Model downloads and optional binaries are separate from user outputs.
- Managed clipboard files are cleared on normal exit and crash leftovers older than
  24 hours are removed on the next launch.
- Batch-rename undo manifests stay inside app data and expire after 30 days.
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

This repository is at a useful checkpoint for pushing. Convert, Resize, Optimize, Export
images, Remove Metadata, Compress Video, Compress Audio, Remove Audio, Extract Audio, Transcribe, Extract
Subtitles, Video Thumbnails, Extract Text, Recognize Text, Combine to PDF, PDF Split, PDF Page
Export, PDF Compression, Create Archive, Extract Archive, Batch Rename, and Inspect use the
shared workspace and retained-session model. Per-tool sessions preserve uploaded files,
settings, item status, and results when the user changes tabs. Removed operation identifiers
remain covered only by bounded legacy-state migration so they cannot reappear in the product.

Quiet image/document jobs and specialized media/PDF subprocesses now share bounded pipe
draining and consistent child cleanup. Output verification also has one shared non-empty-file
and signature baseline at the atomic commit boundary, while each format retains its exact
codec, stream, dimensions, cue, topology, or format validation requirements.
Saved recipes now extend the existing compact output control without adding another settings
surface; legacy presets migrate automatically and current processing choices participate in
exact custom-state matching. An opt-in Finder button beside a selected recipe installs or
removes its Quick Action; invoking it restores that recipe and stages the Finder selection in
an isolated queue rather than silently running stale or input-specific choices.
Recent Jobs now retains transcription and unsuccessful batch outcomes across restarts. Its
three-row sidebar preview opens a full Activity workspace where current entries can load a
validated, isolated copy of their staged sources and settings, legacy entries can reopen their
sources, and every entry can reuse or reveal completed outputs or be removed individually.
Failed rows store only a bounded error category for a safe, useful retry hint; raw subprocess
details remain confined to the active session.
Optimize now separates its existing quality action from an exact file-size goal for JPEG, WebP,
AVIF, and HEIC. Whole-KiB targets are verified against the source topology and byte ceiling before
commit. The packaged Apple Silicon boundary kept JPEG and WebP beneath 256 KiB, preserved source
hashes, created a numbered keep-both result, returned the dedicated unreachable-target error, and
cleaned cancellation without a committed output or hidden partial.
The focused video delivery controls now distinguish maximum resolution from compression goal.
Quality preserves the existing preset defaults, while Compatible, Smaller, and Web can target
1 MiB through 100 GiB with verified two-pass encoding; Lossless cannot target a byte size. The
packaged backend has now completed and decoded exact-ceiling H.264, H.265, and VP9 jobs, including
keep-both and cancellation cleanup.
Compress Audio provides a separate file-size workflow from general format conversion. Its three
AAC/M4A presets share the retained batch and atomic output model, and the backend rejects any
candidate that is not strictly smaller with the dedicated `OutputNotSmaller` outcome.
Recognize Text now offers a backward-compatible Text file mode and a native Searchable PDF
mode without adding another sidebar tool. Its selected output survives tool switches and app
restarts, participates in recipes, and is shown accurately in queue and
completion states. The packaged app has produced verified one-page and multipage searchable
PDFs with unchanged visible scans, extractable text in page order, source preservation,
collision-safe naming, and typed cancellation cleanup under a minimal launch `PATH`.
Product decisions should remain narrow, high-frequency file workflows rather than editor
features. PNG and JPEG now have one-page PDF
output inside Universal Convert, and the existing PDF merge workflow has become Combine to PDF
for an ordered PDF, PNG, and JPEG mix. Together they cover both one-output-per-image and one
combined-document workflows without adding editor controls.

Apple Silicon transcription no longer depends on a system `whisper-cli`. The native-helper
build stages pinned versions, rejects architecture drift without deleting the last valid
runtime, bundles license notices, removes compiled Homebrew backend paths, and verifies every
remaining Mach-O dependency. The packaged request boundary and a clean-environment dyld audit
both resolve the runtime entirely inside `ConvertKit.app`.

For video, keep the product narrower than HandBrake: clear presets for dimensions,
codec, and file-size/quality reduction, followed by focused audio-stream and delivery
utilities. PDF, video, image, and general file utilities should be
prioritized together after this checkpoint is reviewed.

The reference-led circular conversion icon is selected and integrated into the
macOS bundle and app shell. Its sole 1024px source master and generation brief
are documented in `design/icon-concepts/README.md`.
