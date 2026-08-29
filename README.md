# ConvertKit

A private, offline file workspace for macOS. Convert media and documents, resize
images, or optimize image file size from one focused dark-mode interface.

Built with Tauri v2, React, and Rust.

## Available Tools

| Tool | Inputs | Outputs |
|----------|---------|---------|
| Convert images | JPG, PNG, WebP, TIFF, BMP, GIF, ICO, AVIF, HEIC | Other image formats or SVG |
| Convert SVG | SVG | Raster image formats |
| Convert video | MP4, MOV, WebM, MKV, AVI | Video, GIF, or extracted audio |
| Convert audio | MP3, WAV, AAC, FLAC, OGG, M4A | Other audio formats |
| Convert documents | DOCX, HTML, Markdown, EPUB, TXT | Supported document formats, including PDF |
| Resize image | Raster image formats | A resized copy in the original format |
| Optimize image | Raster image formats | A smaller copy in the original format |

All three tools support multi-file queues. Each tool keeps its uploaded files,
settings, queue status, and results when you switch to another sidebar tab and back.

Some conversions need one of the optional engines below.

## Requirements

Core tools (required):

```
brew install ffmpeg imagemagick
```

Optional tools (installed on demand):

```
brew install pandoc resvg tectonic
brew install oxipng jpegoptim gifsicle
brew install --cask libreoffice
cargo install vtracer
```

Optimize always has an ImageMagick fallback. When available, OxiPNG, jpegoptim,
and Gifsicle are also tried for their matching formats, and the smallest valid
candidate is kept.

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
