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
pnpm release:doctor
pnpm release:macos
```

## Hard rules

- Sources are never overwritten.
- Convert picker is generated from `src/lib/formatMatrix.json`. Backend tests require a real engine route for every GUI option.
- Signing/notarize credentials stay in 1Password (`CONVERTKIT_*_REF`). Never plaintext keys.
- Missing engines are reported before a job starts. PDF-to-DOCX is not offered.

## Where to look

- Format matrix → `src/lib/formatMatrix.json`
- Tauri backend → `src-tauri/`
- README for brew engine list and release flow
