# convertkit

Private, offline file workspace for macOS. Convert media and documents, resize and optimize images, transcribe locally, work with PDFs and archives, inspect files. Nothing is uploaded.

Tauri v2 + React + Rust. Apple Silicon only. **Not Next.js.** Do not apply `next16-app`. Frontend is Vite + Tailwind v4. Backend is `src-tauri` — follow `rust-house`.

## Commands

```sh
pnpm install
pnpm tauri dev
pnpm tauri build
pnpm test
pnpm check
pnpm lint
pnpm format
pnpm native:helpers
pnpm package:smoke:apple-silicon
pnpm release:doctor
pnpm release:check
pnpm release:macos
```

Rust: `cargo test` and `cargo clippy --all-targets -- -D warnings` in `src-tauri/`. Most engine tests skip when a helper is not built; run `pnpm native:helpers` first so they run.

## Hard rules

- Sources are never overwritten.
- Convert picker is generated from `src/lib/formatMatrix.json`. Backend tests require a real engine route for every GUI option.
- Signing/notarize credentials stay in 1Password (`CONVERTKIT_*_REF`). Never plaintext keys.
- Missing engines are reported before a job starts. PDF-to-DOCX is not offered.
- Every runtime engine ships inside the app. Never add a Homebrew or other system install as a requirement: bundle the tool from a pinned, checksummed download or source build under `scripts/`, or do the work in process. ImageMagick and LibreOffice are optional fallbacks only.
- Bundled executables are listed in `is_bundled_executable` (`src-tauri/src/engines/mod.rs`) and `bundle.externalBin` (`src-tauri/tauri.conf.json`), and checked by `scripts/check-apple-silicon-package.sh`. Keep all three in step.
- FFmpeg and Pandoc are GPL-2.0-or-later. Keep FFmpeg's build GPLv2 (no `--enable-version3` or `--enable-nonfree`), keep every licence and NOTICE in the bundle, and attach `ConvertKit_<version>_third-party-source.tar.gz` (from `scripts/package-third-party-sources.sh`) to every release that ships a DMG.

## Where to look

- Format matrix → `src/lib/formatMatrix.json`
- Tauri backend → `src-tauri/`; engine routing and tool resolution → `src-tauri/src/engines/mod.rs`
- Images → `engines/imageio.rs` (drives the bundled ImageIO helper, `src-tauri/native/imageio.m`) and `engines/codecs.rs` (mozjpeg, libwebp, ICO, GIF and JPEG optimizers)
- PDFs → the bundled PDFKit helper (`src-tauri/native/pdf.m`) answers `pdfinfo`, `pdftoppm` and `pdftotext`; split, merge and compress are `engines/pdf_pages.rs` and `engines/pdf_compress.rs`
- Pinned third-party builds → `scripts/build-ffmpeg.sh`, `scripts/build-whisper.sh`, `scripts/stage-document-tools.sh` (Pandoc, Tectonic)
- Release flow, licences and setup → `README.md`; roadmap and shipped history → `PLAN.md`
