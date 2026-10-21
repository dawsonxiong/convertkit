# Performance: 0.1.0 → 0.2.0 → PR #5

0.2.0 bundled every engine, so nothing needs Homebrew. That made PDF work much
faster, but several image jobs became slower than they were in 0.1.0.
[PR #5](https://github.com/dawsonxiong/convertkit/pull/5) fixes those
regressions and speeds up PDF work further. Every benchmark output is
byte-identical to 0.2.0's.

Measured on Apple Silicon on 21 October 2026.

## Headlines

| Change | Result |
|---|---|
| JPEG to a target size vs 0.2.0 | 4.6× faster (8.76 s → 1.92 s) |
| Optimize JPEG vs 0.2.0 | 2.2× faster (3.35 s → 1.53 s) |
| PDF → PNG, 37 pages, vs 0.1.0 | 12.8× faster (22.96 s → 1.79 s) |
| App binary size | 23% smaller (26 MB → 20 MB) |

## All jobs

Times are the median of 5 runs after a warm-up and include the app's 0.24 s
launch. The three builds ran the same jobs on the same fixtures, alternating
builds on each job. "vs 0.2.0" marks speedups of 10% or more.

| Job | 0.1.0 | 0.2.0 | PR #5 | vs 0.2.0 |
|---|---:|---:|---:|---:|
| JPEG to 1 MiB target | 2.06 s | 8.76 s | 1.92 s | 4.6× faster |
| Optimize JPEG | 1.31 s | 3.35 s | 1.53 s | 2.2× faster |
| PDF → PNG pages (37) | 22.96 s | 4.80 s | 1.79 s | 2.7× faster |
| Compress PDF (balanced) | 0.99 s | 2.66 s | 1.10 s | 2.4× faster |
| JPEG → WebP (12 MP) | 1.21 s | 1.80 s | 1.16 s | 1.5× faster |
| GIF → animated WebP (60 frames) | not supported | 1.31 s | 0.72 s | 1.8× faster |
| PNG → JPEG (3 MP) | 0.34 s | 0.67 s | 0.51 s | 1.3× faster |
| Export images (web + social) | 1.16 s | 1.21 s | 0.89 s | 1.4× faster |
| Optimize animated GIF | 1.58 s | 0.67 s | 0.55 s | 1.2× faster |
| Resize 12 MP JPEG → 1600 px | 0.56 s | 0.61 s | 0.50 s | 1.2× faster |
| Optimize PNG screenshot | 4.99 s | 4.17 s | 4.00 s | same |
| Screenshot PNG → WebP | 0.56 s | 0.61 s | 0.55 s | same |
| JPEG → PNG (12 MP) | 1.42 s | 1.10 s | 1.15 s | same |
| SVG → PNG | 0.50 s | 0.55 s | 0.55 s | same |
| PNG → SVG (trace) | 0.67 s | 0.73 s | 0.72 s | same |
| Merge PDFs | 1.00 s | 0.29 s | 0.29 s | same |
| Split PDF (every page) | 1.16 s | 0.29 s | 0.29 s | same |
| PDF text extraction | 0.29 s | 0.34 s | 0.30 s | same |
| Markdown → DOCX | 0.40 s | 0.39 s | 0.40 s | same |
| Markdown → PDF | 1.37 s | 1.37 s | 1.37 s | same |
| Compress audio (3 min WAV) | 1.86 s | 1.64 s | 1.63 s | same |
| Encode video (20 s 1080p) | 3.93 s | 3.82 s | 3.83 s | same |
| Remove metadata (JPEG) | 0.24 s | 0.24 s | 0.24 s | same |
| App launch only | 0.23 s | 0.24 s | 0.24 s | same |

## What PR #5 changed

- **Raw pixels between the helper and the encoders.** The ImageIO helper used
  to give decoded frames to mozjpeg and libwebp as PNG files. Writing and
  reading that PNG took about 0.86 s for a 12 MP photo. The helper's new
  `pixels` command writes raw RGBA in 0.16 s, and the pixels are identical,
  including semi-transparent edges.
- **Decode once, encode many.** `imageio::PreparedImage` keeps a decoded JPEG
  or WebP source in memory. Target-size jobs reuse it instead of decoding the
  source again for each of up to 14 qualities. They also binary-search the
  qualities, so they need at most 4 encodes. A test checks that the search
  picks the same quality the old step-by-step search did.
- **Optimize runs its candidates at the same time.** The JPEG re-encode and
  the lossless transcode now run together.
- **PDF work runs in parallel.** Page export renders up to 8 pages at once,
  each in its own helper process. Compression decodes, resamples and
  re-encodes the PDF's images across all cores with rayon.
- **Smaller release binary.** Release builds use thin LTO with one codegen
  unit. Job times don't change, because the heavy codecs are written in C.

## Trade-offs that remain

- **PNG → JPEG and Optimize JPEG are still 0.15–0.2 s slower than 0.1.0.**
  mozjpeg's trellis quantisation costs that time and makes smaller files. The
  3 MP test image comes out at 425 KB instead of 808 KB, and Optimize's
  output is 9% smaller.
- **Compress PDF takes 0.1 s longer than in 0.1.0.** Ghostscript barely
  shrank this PDF (18.3 MB → 18.3 MB). The built-in compressor takes it to
  4.5 MB.
- **Optimize PNG still takes about 4 s on a large screenshot.** oxipng at
  level 2 instead of 4 would take about half as long, for a file about 1%
  larger. That's a product choice, so this PR doesn't change it.

## Method

Each build was bundled as an app, compiled with `opt-level=3`, and had the
debug job hook (`CONVERTKIT_DEBUG_SMOKE_REQUEST`) turned on. Each build
received the same job request and was timed from launch to exit. 0.1.0 used
the Homebrew ImageMagick, Poppler, Ghostscript, oxipng, gifsicle and FFmpeg
installed on the test Mac.

The fixtures are synthetic and generated fresh for each benchmark run:

- a 4032×3024 noisy photo, plus a 3 MP version
- a 2880×1800 screenshot-like PNG
- a 60-frame GIF
- a 37-page PDF with text pages and 12 photo pages
- a 20 s 1080p H.264 clip
- a 3-minute WAV
- a 40-section Markdown document

PR #5 was also checked with:

- `cargo test` (284 tests) and `cargo clippy`
- `pnpm package:smoke:apple-silicon`
- six smoke scripts
- 12 extra conversions compared byte for byte with 0.2.0
