# ConvertKit

A minimal desktop file converter for macOS. Drop a file in, pick an output format, click convert.

Built with Tauri v2, React, and Rust.

## Supported Formats

| Category | Formats |
|----------|---------|
| Image | JPG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC |
| Video | MP4, MOV, WebM, MKV, AVI |
| Audio | MP3, WAV, AAC, FLAC, OGG, M4A |
| Document | PDF, DOCX, HTML, Markdown, EPUB, TXT |
| Vector | SVG |

Video files can also be converted to GIF or have audio extracted.

## Requirements

Core tools (required):

```
brew install ffmpeg imagemagick
```

Optional tools (installed on demand):

```
brew install pandoc resvg tectonic
brew install --cask libreoffice
cargo install vtracer
```

The app checks for missing tools on launch and prompts you to install them.

## Development

```
pnpm install
pnpm tauri dev
```

### Scripts

| Command | Description |
|---------|-------------|
| `pnpm tauri dev` | Run in development mode |
| `pnpm tauri build` | Build production DMG |
| `pnpm check` | Type-check with tsgo |
| `pnpm lint` | Lint with oxlint |
| `pnpm format` | Format with oxfmt |

### Tech Stack

- **Framework**: Tauri v2 (Rust backend, web frontend)
- **Frontend**: React 18, Tailwind CSS v4, Zustand, Framer Motion
- **Engines**: FFmpeg, ImageMagick, Pandoc, resvg, VTracer, LibreOffice

## Build

```
pnpm tauri build
```

Output: `src-tauri/target/release/bundle/dmg/ConvertKit.dmg`

## Keyboard Shortcuts

| Shortcut | Action |
|----------|--------|
| `Cmd+O` | Open file browser |
| `Enter` | Start conversion |
| `Esc` | Cancel or reset |
| `Cmd+R` | Reveal output in Finder |

## License

MIT
