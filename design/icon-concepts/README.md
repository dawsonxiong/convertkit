# ConvertKit app icon concepts

## Selected direction — Circular conversion

The selected icon follows the bold circular-conversion language of the user-supplied
reference while using ConvertKit's own palette and geometry. It was generated with
the bundled ImageGen CLI and `gpt-image-2` at high quality, then normalized locally
to a warm off-white rounded square and periwinkle mark with transparent corners. A
superseded near-black outer frame was removed so the white square is the icon's true
optical boundary.

- Final master: [`convertkit-reference-final.png`](../../output/imagegen/app-icons/convertkit-reference-final.png)
- Installed into the complete Tauri platform icon set under `src-tauri/icons/`

The API key was resolved at runtime through a 1Password secret reference and was
never written to the project.

### Final prompt

```text
Use case: logo-brand
Asset type: final macOS desktop application icon for ConvertKit
Input images: Image 1 is a composition and simplicity reference only; do not copy its exact symbol or proportions.
Primary request: Create an original ConvertKit icon with the same immediate, bold readability as Image 1. Use one near-black rounded-square app tile. Center a large warm off-white rounded square inside it. Inside the white square, place a single heavy pale-periwinkle circular conversion symbol formed from two opposing curved segments with clean diagonal breaks. The two segments should clearly imply files changing from one form to another, but must feel like one custom continuous emblem rather than two generic emoji arrows.
Style/medium: severe flat 2D vector-like mark, Swiss-modern utility software branding, simple enough to redraw as SVG
Composition/framing: straight-on, optically centered, broad filled geometry, generous even margins, no tiny internal details, unmistakable at 16 px
Color palette: near black #0E0F12, warm off-white #F0EDEF, medium periwinkle #8FAAF4; exactly these three solid colors
Constraints: one icon only; no text; no letters; no document pictogram; no extra symbols; no gradients; no shadows; no glow; no gloss; no texture; no bevel; no 3D; no mockup; no watermark
Avoid: copying Image 1 exactly, thin strokes, four arrows, recycling logos, sync-service logos, cloud symbols, decorative details
```

## Asset policy

- `convertkit-reference-final.png` is the only editable 1024px source asset.
- `src-tauri/icons/` contains only the platform outputs generated from that source.
- Regenerate every platform variant with `pnpm icons:generate`.
- The legacy macOS ICNS uses an 824px tile centered on its 1024px source canvas,
  matching the 206px opaque footprint of Apple's rounded-square icons at 256px.
- The macOS ICNS must contain 16, 32, 64, 128, 256, 512, and 1024px representations;
  the Windows ICO must contain 16, 24, 32, 48, 64, and 256px representations.
- Rejected concepts, temporary review renders, and duplicate masters are not kept in
  the repository.
