pub mod ffmpeg;
pub mod image_pdf;
pub mod imagemagick;
pub mod libreoffice;
pub mod pandoc;
pub mod process;
pub mod resvg;
pub mod verification;
pub mod vtracer;

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::Serialize;
use tauri::AppHandle;
use tokio_util::sync::CancellationToken;

use crate::error::ConversionError;
use crate::formats::Format;

/// Resolve CLI tools for both terminal-launched development builds and Finder
/// or Spotlight-launched app bundles, whose inherited PATH is minimal.
pub fn resolve_tool(name: &str) -> Option<PathBuf> {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let resolved = tool_search_directories(Some(inherited), std::env::current_exe().ok())
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file());
    resolved.or_else(|| development_sidecar(name))
}

fn development_sidecar(name: &str) -> Option<PathBuf> {
    if !matches!(
        name,
        "convertkit-image-pdf" | "convertkit-vision-ocr" | "whisper-cli"
    ) {
        return None;
    }
    let target = if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "aarch64-apple-darwin"
    } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
        "x86_64-apple-darwin"
    } else {
        return None;
    };
    let candidate = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("binaries")
        .join(format!("{name}-{target}"));
    candidate.is_file().then_some(candidate)
}

fn tool_search_directories(
    inherited: Option<OsString>,
    current_executable: Option<PathBuf>,
) -> Vec<PathBuf> {
    let mut search_paths = Vec::new();
    if let Some(directory) =
        current_executable.and_then(|path| path.parent().map(Path::to_path_buf))
    {
        search_paths.push(directory);
    }
    if let Some(inherited) = inherited {
        for directory in std::env::split_paths(&inherited) {
            if !search_paths.contains(&directory) {
                search_paths.push(directory);
            }
        }
    }
    for directory in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let path = PathBuf::from(directory);
        if !search_paths.contains(&path) {
            search_paths.push(path);
        }
    }
    search_paths
}

/// Create a subprocess command using the resolved executable path.
pub fn tool_command(name: &str) -> tokio::process::Command {
    let executable = resolve_tool(name).unwrap_or_else(|| PathBuf::from(name));
    let mut command = tokio::process::Command::new(&executable);
    if name == "whisper-cli" {
        if let Some(backends) = bundled_whisper_backends(&executable) {
            command.current_dir(backends);
            command.env_remove("GGML_BACKEND_PATH");
        }
    }
    command
}

fn bundled_whisper_backends(executable: &Path) -> Option<PathBuf> {
    let macos_directory = executable.parent()?;
    if macos_directory.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let backends = macos_directory
        .parent()?
        .join("Resources/whisper-runtime/backends");
    backends.is_dir().then_some(backends)
}

// ---------------------------------------------------------------------------
// Shared types
// ---------------------------------------------------------------------------

/// Everything an engine needs to know to perform a conversion.
pub struct ConversionRequest {
    pub job_id: String,
    pub input_path: PathBuf,
    pub output_path: PathBuf,
    pub input_format: Format,
    pub output_format: Format,
}

/// Returned to the frontend on success.
#[derive(Debug, Clone, Serialize)]
pub struct ConversionResult {
    pub output_path: String,
    pub output_paths: Vec<String>,
    pub output_size: u64,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub undo_manifest: Option<String>,
}

// ---------------------------------------------------------------------------
// Engine trait
// ---------------------------------------------------------------------------

/// A backend that can convert files from one format to another.
///
/// Uses `impl Future` (RPITIT) for `convert`, which means the trait is not
/// dyn-compatible. We use the [`EngineKind`] enum for dynamic dispatch instead.
pub trait ConversionEngine: Send + Sync {
    /// Whether this engine can handle the given `(input, output)` pair.
    fn supports(&self, input: Format, output: Format) -> bool;

    /// Perform the conversion. Implementations should respect cancellation and
    /// emit progress events via the [`AppHandle`].
    fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> impl std::future::Future<Output = Result<ConversionResult, ConversionError>> + Send;

    /// The name of the CLI tool this engine shells out to.
    fn required_tool(&self) -> &'static str;
}

// ---------------------------------------------------------------------------
// Enum dispatch -- avoids dyn-compatibility issues
// ---------------------------------------------------------------------------

/// Concrete enum that wraps every engine so we can return a single type from
/// the router without requiring trait objects.
pub enum EngineKind {
    Resvg(resvg::ResvgEngine),
    VTracer(vtracer::VTracerEngine),
    LibreOffice(libreoffice::LibreOfficeEngine),
    ImagePdf(image_pdf::ImagePdfEngine),
    ImageMagick(imagemagick::ImageMagickEngine),
    Ffmpeg(ffmpeg::FfmpegEngine),
    Pandoc(pandoc::PandocEngine),
}

impl EngineKind {
    pub fn supports(&self, input: Format, output: Format) -> bool {
        match self {
            Self::Resvg(e) => e.supports(input, output),
            Self::VTracer(e) => e.supports(input, output),
            Self::LibreOffice(e) => e.supports(input, output),
            Self::ImagePdf(e) => e.supports(input, output),
            Self::ImageMagick(e) => e.supports(input, output),
            Self::Ffmpeg(e) => e.supports(input, output),
            Self::Pandoc(e) => e.supports(input, output),
        }
    }

    pub fn required_tool(&self) -> &'static str {
        match self {
            Self::Resvg(e) => e.required_tool(),
            Self::VTracer(e) => e.required_tool(),
            Self::LibreOffice(e) => e.required_tool(),
            Self::ImagePdf(e) => e.required_tool(),
            Self::ImageMagick(e) => e.required_tool(),
            Self::Ffmpeg(e) => e.required_tool(),
            Self::Pandoc(e) => e.required_tool(),
        }
    }

    /// Every executable required for this exact conversion. Most engines need
    /// one binary; Pandoc PDF output also needs a LaTeX engine.
    pub fn required_tools(&self, output: Format) -> Vec<&'static str> {
        match self {
            Self::Pandoc(_) if output == Format::Pdf => vec!["pandoc", "tectonic"],
            _ => vec![self.required_tool()],
        }
    }

    pub fn is_available_for(&self, output: Format) -> bool {
        self.required_tools(output)
            .into_iter()
            .all(|tool| resolve_tool(tool).is_some())
    }

    pub async fn convert(
        &self,
        request: ConversionRequest,
        app: AppHandle,
        cancel_token: CancellationToken,
    ) -> Result<ConversionResult, ConversionError> {
        match self {
            Self::Resvg(e) => e.convert(request, app, cancel_token).await,
            Self::VTracer(e) => e.convert(request, app, cancel_token).await,
            Self::LibreOffice(e) => e.convert(request, app, cancel_token).await,
            Self::ImagePdf(e) => e.convert(request, app, cancel_token).await,
            Self::ImageMagick(e) => e.convert(request, app, cancel_token).await,
            Self::Ffmpeg(e) => e.convert(request, app, cancel_token).await,
            Self::Pandoc(e) => e.convert(request, app, cancel_token).await,
        }
    }
}

// ---------------------------------------------------------------------------
// Router
// ---------------------------------------------------------------------------

/// Return every engine that supports a pair in preference order.
pub fn get_engines(input: Format, output: Format) -> Vec<EngineKind> {
    // Priority-ordered list. We construct lightweight structs on the fly.
    let candidates: Vec<EngineKind> = vec![
        EngineKind::Resvg(resvg::ResvgEngine),
        EngineKind::VTracer(vtracer::VTracerEngine),
        EngineKind::LibreOffice(libreoffice::LibreOfficeEngine),
        EngineKind::ImagePdf(image_pdf::ImagePdfEngine),
        EngineKind::ImageMagick(imagemagick::ImageMagickEngine),
        EngineKind::Ffmpeg(ffmpeg::FfmpegEngine),
        EngineKind::Pandoc(pandoc::PandocEngine),
    ];

    candidates
        .into_iter()
        .filter(|engine| engine.supports(input, output))
        .collect()
}

/// Return the best installed engine for a format pair. This deliberately
/// falls through to another compatible engine when the preferred executable
/// is unavailable.
pub fn get_engine(input: Format, output: Format) -> Option<EngineKind> {
    get_engines(input, output)
        .into_iter()
        .find(|engine| engine.is_available_for(output))
}

/// Missing tool groups for each compatible engine. Each inner vector is an
/// AND requirement; outer entries are alternatives.
pub fn missing_tool_groups(input: Format, output: Format) -> Vec<Vec<&'static str>> {
    get_engines(input, output)
        .into_iter()
        .filter_map(|engine| {
            let missing = engine
                .required_tools(output)
                .into_iter()
                .filter(|tool| resolve_tool(tool).is_none())
                .collect::<Vec<_>>();
            (!missing.is_empty()).then_some(missing)
        })
        .collect()
}

/// Remove a partially-written output file, if it exists.
/// Shared by all engines to avoid duplication.
pub fn cleanup_partial(path: &Path) {
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    #[test]
    fn bundled_tools_are_searched_before_path_and_homebrew() {
        let inherited = std::env::join_paths(["/usr/bin", "/bin"]).expect("valid search path");
        let directories = tool_search_directories(
            Some(inherited),
            Some(PathBuf::from(
                "/Applications/ConvertKit.app/Contents/MacOS/convertkit",
            )),
        );
        assert_eq!(
            directories,
            vec![
                PathBuf::from("/Applications/ConvertKit.app/Contents/MacOS"),
                PathBuf::from("/usr/bin"),
                PathBuf::from("/bin"),
                PathBuf::from("/opt/homebrew/bin"),
                PathBuf::from("/usr/local/bin"),
            ]
        );
    }

    #[test]
    fn whisper_backend_path_is_limited_to_a_real_app_bundle() {
        let fixture = tempfile::tempdir().expect("temporary bundle");
        let macos = fixture.path().join("ConvertKit.app/Contents/MacOS");
        let backends = fixture
            .path()
            .join("ConvertKit.app/Contents/Resources/whisper-runtime/backends");
        std::fs::create_dir_all(&macos).expect("MacOS directory");
        std::fs::create_dir_all(&backends).expect("backend directory");

        assert_eq!(
            bundled_whisper_backends(&macos.join("whisper-cli")),
            Some(backends)
        );
        assert_eq!(
            bundled_whisper_backends(Path::new("/usr/local/bin/whisper-cli")),
            None
        );
    }

    #[test]
    fn svg_png_has_resvg_and_imagemagick_routes() {
        let tools = get_engines(Format::Svg, Format::Png)
            .into_iter()
            .map(|engine| engine.required_tool())
            .collect::<Vec<_>>();
        assert_eq!(tools, vec!["resvg", "magick"]);
    }

    #[test]
    fn png_and_jpeg_pdf_use_only_the_bundled_single_page_route() {
        for input in [Format::Png, Format::Jpg] {
            let tools = get_engines(input, Format::Pdf)
                .into_iter()
                .map(|engine| engine.required_tool())
                .collect::<Vec<_>>();
            assert_eq!(tools, vec!["convertkit-image-pdf"]);
        }
        assert!(get_engines(Format::Gif, Format::Pdf).is_empty());
        assert!(get_engines(Format::Svg, Format::Pdf).is_empty());
    }

    #[test]
    fn pandoc_pdf_requires_a_pdf_engine() {
        let engines = get_engines(Format::Md, Format::Pdf);
        assert_eq!(engines.len(), 1);
        assert_eq!(
            engines[0].required_tools(Format::Pdf),
            vec!["pandoc", "tectonic"]
        );
    }

    #[test]
    fn frontend_matrix_matches_the_engine_router() {
        let frontend: BTreeMap<String, Vec<String>> =
            serde_json::from_str(include_str!("../../../src/lib/formatMatrix.json"))
                .expect("frontend format matrix should be valid JSON");

        assert_eq!(frontend.len(), Format::ALL.len() + 2);
        assert_eq!(frontend.get("zip"), Some(&Vec::new()));
        assert_eq!(frontend.get("tar"), Some(&Vec::new()));

        for input in Format::ALL {
            let frontend_targets = frontend
                .get(input.extension())
                .unwrap_or_else(|| panic!("missing frontend row for {}", input.extension()));
            let rust_targets = input
                .compatible_targets()
                .into_iter()
                .map(|format| format.extension())
                .collect::<BTreeSet<_>>();
            let frontend_target_set = frontend_targets
                .iter()
                .map(String::as_str)
                .collect::<BTreeSet<_>>();

            assert_eq!(
                frontend_target_set,
                rust_targets,
                "frontend and Rust disagree for {}",
                input.extension()
            );

            for output in Format::ALL {
                let advertised = frontend_targets
                    .iter()
                    .any(|target| target == output.extension());
                let routed = !get_engines(input, output).is_empty();
                assert_eq!(
                    advertised,
                    routed,
                    "{} → {} is advertised={advertised}, routed={routed}",
                    input.extension(),
                    output.extension()
                );
            }
        }
    }
}
