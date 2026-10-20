pub mod codecs;
pub mod ffmpeg;
pub mod image_pdf;
pub mod imageio;
pub mod imagemagick;
pub mod pdf_compress;
pub mod pdf_pages;
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
    if pdf_helper_command(name) {
        if let Some(helper) = resolve_executable(PDF_HELPER) {
            return Some(helper);
        }
    }
    resolve_executable(name)
}

/// The bundled PDF helper answers these Poppler commands with the same
/// arguments and output, so Poppler is only a fallback when the helper is
/// missing from a development checkout.
const PDF_HELPER: &str = "convertkit-pdf";

fn pdf_helper_command(name: &str) -> bool {
    matches!(name, "pdfinfo" | "pdftoppm" | "pdftotext")
}

fn resolve_executable(name: &str) -> Option<PathBuf> {
    let inherited = std::env::var_os("PATH").unwrap_or_default();
    let resolved = tool_search_directories(Some(inherited), std::env::current_exe().ok())
        .into_iter()
        .map(|directory| directory.join(name))
        .find(|path| path.is_file());
    resolved.or_else(|| development_sidecar(name))
}

/// Executables that ship inside the app bundle, including the Poppler
/// commands the bundled PDF helper answers.
pub fn is_bundled_tool(name: &str) -> bool {
    pdf_helper_command(name) || is_bundled_executable(name)
}

fn is_bundled_executable(name: &str) -> bool {
    matches!(
        name,
        "convertkit-image-pdf"
            | "convertkit-imageio"
            | "convertkit-pdf"
            | "convertkit-vision-ocr"
            | "ffmpeg"
            | "ffprobe"
            | "pandoc"
            | "tectonic"
            | "whisper-cli"
    )
}

fn development_sidecar(name: &str) -> Option<PathBuf> {
    if !is_bundled_executable(name) {
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
    if pdf_helper_command(name) && executable.file_name() != Some(std::ffi::OsStr::new(name)) {
        command.arg(name);
    }
    if name == "whisper-cli" {
        // The bundled whisper-cli has its backends built in, but GGML still scans
        // the working directory and GGML_BACKEND_PATH for plug-ins. Start it in its
        // own runtime folder, which holds none, so nothing else gets loaded.
        if let Some(runtime) = bundled_whisper_runtime(&executable) {
            command.current_dir(runtime);
            command.env_remove("GGML_BACKEND_PATH");
        }
    }
    command
}

fn bundled_whisper_runtime(executable: &Path) -> Option<PathBuf> {
    let macos_directory = executable.parent()?;
    if macos_directory.file_name()?.to_str()? != "MacOS" {
        return None;
    }
    let runtime = macos_directory.parent()?.join("Resources/whisper-runtime");
    runtime.is_dir().then_some(runtime)
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
    ImageIo(imageio::ImageIoEngine),
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
            Self::ImageIo(e) => e.supports(input, output),
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
            Self::ImageIo(e) => e.required_tool(),
            Self::ImageMagick(e) => e.required_tool(),
            Self::Ffmpeg(e) => e.required_tool(),
            Self::Pandoc(e) => e.required_tool(),
        }
    }

    /// Every executable required for this exact conversion. Most engines need
    /// one binary, in-process engines none, and Pandoc PDF output also needs
    /// a LaTeX engine.
    pub fn required_tools(&self, output: Format) -> Vec<&'static str> {
        match self {
            // resvg and vtracer are linked into the app; vtracer reads its
            // input through the bundled ImageIO helper.
            Self::Resvg(_) => Vec::new(),
            Self::VTracer(_) => vec![imageio::HELPER],
            Self::Pandoc(_) if output == Format::Pdf => vec!["pandoc", "tectonic"],
            _ => vec![self.required_tool()],
        }
    }

    pub fn is_available_for(&self, input: Format, output: Format) -> bool {
        let installed = self
            .required_tools(output)
            .into_iter()
            .all(|tool| resolve_tool(tool).is_some());
        match self {
            Self::ImageIo(e) => installed && e.is_available_for(input, output),
            _ => installed,
        }
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
            Self::ImageIo(e) => e.convert(request, app, cancel_token).await,
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
        EngineKind::ImageIo(imageio::ImageIoEngine),
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
        .find(|engine| engine.is_available_for(input, output))
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
    fn whisper_runtime_path_is_limited_to_a_real_app_bundle() {
        let fixture = tempfile::tempdir().expect("temporary bundle");
        let macos = fixture.path().join("ConvertKit.app/Contents/MacOS");
        let runtime = fixture
            .path()
            .join("ConvertKit.app/Contents/Resources/whisper-runtime");
        std::fs::create_dir_all(&macos).expect("MacOS directory");
        std::fs::create_dir_all(&runtime).expect("runtime directory");

        assert_eq!(
            bundled_whisper_runtime(&macos.join("whisper-cli")),
            Some(runtime)
        );
        assert_eq!(
            bundled_whisper_runtime(Path::new("/usr/local/bin/whisper-cli")),
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
    fn raster_pairs_prefer_the_bundled_imageio_helper() {
        let tools = get_engines(Format::Png, Format::WebP)
            .into_iter()
            .map(|engine| engine.required_tool())
            .collect::<Vec<_>>();
        assert_eq!(tools, vec!["convertkit-imageio", "magick"]);
    }

    #[test]
    fn svg_rasters_need_no_installed_tool() {
        for output in [Format::Png, Format::Jpg, Format::WebP, Format::Ico] {
            let engines = get_engines(Format::Svg, output);
            assert!(engines[0].required_tools(output).is_empty());
            assert!(engines[0].is_available_for(Format::Svg, output));
        }
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
    fn docx_pdf_prefers_libreoffice_then_falls_back_to_pandoc() {
        let tools = get_engines(Format::Docx, Format::Pdf)
            .into_iter()
            .map(|engine| engine.required_tool())
            .collect::<Vec<_>>();
        assert_eq!(tools, vec!["soffice", "pandoc"]);
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
