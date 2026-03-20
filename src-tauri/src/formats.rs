use serde::{Deserialize, Serialize};

/// Every file format the app can handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    // Image
    Jpg,
    Png,
    WebP,
    Tiff,
    Bmp,
    Gif,
    Ico,
    Avif,
    Heic,
    // Video
    Mp4,
    Mov,
    WebM,
    Mkv,
    Avi,
    // Audio
    Mp3,
    Wav,
    Aac,
    Flac,
    Ogg,
    M4a,
    // Document
    Pdf,
    Docx,
    Html,
    Md,
    Epub,
    Txt,
    // Vector
    Svg,
}

/// Broad category a format falls into.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FileCategory {
    Image,
    Video,
    Audio,
    Document,
    Vector,
}

impl Format {
    /// Which broad category this format belongs to.
    pub fn category(&self) -> FileCategory {
        match self {
            Self::Jpg | Self::Png | Self::WebP | Self::Tiff | Self::Bmp | Self::Gif
            | Self::Ico | Self::Avif | Self::Heic => FileCategory::Image,

            Self::Mp4 | Self::Mov | Self::WebM | Self::Mkv | Self::Avi => FileCategory::Video,

            Self::Mp3 | Self::Wav | Self::Aac | Self::Flac | Self::Ogg | Self::M4a => {
                FileCategory::Audio
            }

            Self::Pdf | Self::Docx | Self::Html | Self::Md | Self::Epub | Self::Txt => {
                FileCategory::Document
            }

            Self::Svg => FileCategory::Vector,
        }
    }

    /// Canonical file extension (without leading dot).
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Jpg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
            Self::Bmp => "bmp",
            Self::Gif => "gif",
            Self::Ico => "ico",
            Self::Avif => "avif",
            Self::Heic => "heic",
            Self::Mp4 => "mp4",
            Self::Mov => "mov",
            Self::WebM => "webm",
            Self::Mkv => "mkv",
            Self::Avi => "avi",
            Self::Mp3 => "mp3",
            Self::Wav => "wav",
            Self::Aac => "aac",
            Self::Flac => "flac",
            Self::Ogg => "ogg",
            Self::M4a => "m4a",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
            Self::Html => "html",
            Self::Md => "md",
            Self::Epub => "epub",
            Self::Txt => "txt",
            Self::Svg => "svg",
        }
    }

    /// Parse a file extension (case-insensitive) into a [`Format`].
    /// Handles common aliases such as `jpeg` -> `Jpg`.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Some(Self::Jpg),
            "png" => Some(Self::Png),
            "webp" => Some(Self::WebP),
            "tiff" | "tif" => Some(Self::Tiff),
            "bmp" => Some(Self::Bmp),
            "gif" => Some(Self::Gif),
            "ico" => Some(Self::Ico),
            "avif" => Some(Self::Avif),
            "heic" | "heif" => Some(Self::Heic),
            "mp4" | "m4v" => Some(Self::Mp4),
            "mov" => Some(Self::Mov),
            "webm" => Some(Self::WebM),
            "mkv" => Some(Self::Mkv),
            "avi" => Some(Self::Avi),
            "mp3" => Some(Self::Mp3),
            "wav" => Some(Self::Wav),
            "aac" => Some(Self::Aac),
            "flac" => Some(Self::Flac),
            "ogg" | "oga" => Some(Self::Ogg),
            "m4a" => Some(Self::M4a),
            "pdf" => Some(Self::Pdf),
            "docx" => Some(Self::Docx),
            "html" | "htm" => Some(Self::Html),
            "md" | "markdown" => Some(Self::Md),
            "epub" => Some(Self::Epub),
            "txt" | "text" => Some(Self::Txt),
            "svg" => Some(Self::Svg),
            _ => None,
        }
    }

    /// Formats this format can be converted **to**.
    pub fn compatible_targets(&self) -> Vec<Format> {
        match self.category() {
            FileCategory::Image => {
                let mut targets = vec![
                    Self::Jpg,
                    Self::Png,
                    Self::WebP,
                    Self::Tiff,
                    Self::Bmp,
                    Self::Gif,
                    Self::Ico,
                    Self::Avif,
                ];
                // Remove self from targets
                targets.retain(|f| f != self);
                targets
            }
            FileCategory::Video => {
                let mut targets = vec![
                    Self::Mp4, Self::Mov, Self::WebM, Self::Mkv, Self::Avi,
                    Self::Gif, // video -> GIF via FFmpeg
                    Self::Mp3, Self::Wav, Self::Aac, Self::Flac, Self::Ogg, Self::M4a, // audio extraction
                ];
                targets.retain(|f| f != self);
                targets
            }
            FileCategory::Audio => {
                let mut targets =
                    vec![Self::Mp3, Self::Wav, Self::Aac, Self::Flac, Self::Ogg, Self::M4a];
                targets.retain(|f| f != self);
                targets
            }
            FileCategory::Document => {
                let mut targets = vec![
                    Self::Pdf,
                    Self::Docx,
                    Self::Html,
                    Self::Md,
                    Self::Epub,
                    Self::Txt,
                ];
                targets.retain(|f| f != self);
                targets
            }
            FileCategory::Vector => {
                // SVG can go to raster image formats as well as PDF
                vec![Self::Png, Self::Jpg, Self::WebP, Self::Pdf]
            }
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Jpg => "JPEG Image",
            Self::Png => "PNG Image",
            Self::WebP => "WebP Image",
            Self::Tiff => "TIFF Image",
            Self::Bmp => "BMP Image",
            Self::Gif => "GIF Image",
            Self::Ico => "ICO Icon",
            Self::Avif => "AVIF Image",
            Self::Heic => "HEIC Image",
            Self::Mp4 => "MP4 Video",
            Self::Mov => "MOV Video",
            Self::WebM => "WebM Video",
            Self::Mkv => "MKV Video",
            Self::Avi => "AVI Video",
            Self::Mp3 => "MP3 Audio",
            Self::Wav => "WAV Audio",
            Self::Aac => "AAC Audio",
            Self::Flac => "FLAC Audio",
            Self::Ogg => "OGG Audio",
            Self::M4a => "M4A Audio",
            Self::Pdf => "PDF Document",
            Self::Docx => "Word Document",
            Self::Html => "HTML Document",
            Self::Md => "Markdown Document",
            Self::Epub => "EPUB eBook",
            Self::Txt => "Plain Text",
            Self::Svg => "SVG Vector",
        }
    }
}

impl std::fmt::Display for FileCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Image => write!(f, "image"),
            Self::Video => write!(f, "video"),
            Self::Audio => write!(f, "audio"),
            Self::Document => write!(f, "document"),
            Self::Vector => write!(f, "vector"),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_common_extensions() {
        assert_eq!(Format::from_extension("jpg"), Some(Format::Jpg));
        assert_eq!(Format::from_extension("JPEG"), Some(Format::Jpg));
        assert_eq!(Format::from_extension("png"), Some(Format::Png));
        assert_eq!(Format::from_extension("webp"), Some(Format::WebP));
        assert_eq!(Format::from_extension("tif"), Some(Format::Tiff));
        assert_eq!(Format::from_extension("TIFF"), Some(Format::Tiff));
    }

    #[test]
    fn parse_aliases() {
        assert_eq!(Format::from_extension("jpeg"), Some(Format::Jpg));
        assert_eq!(Format::from_extension("htm"), Some(Format::Html));
        assert_eq!(Format::from_extension("markdown"), Some(Format::Md));
        assert_eq!(Format::from_extension("heif"), Some(Format::Heic));
        assert_eq!(Format::from_extension("oga"), Some(Format::Ogg));
        assert_eq!(Format::from_extension("m4v"), Some(Format::Mp4));
        assert_eq!(Format::from_extension("text"), Some(Format::Txt));
    }

    #[test]
    fn unknown_extension_returns_none() {
        assert_eq!(Format::from_extension("xyz"), None);
        assert_eq!(Format::from_extension(""), None);
    }

    #[test]
    fn category_is_correct() {
        assert_eq!(Format::Jpg.category(), FileCategory::Image);
        assert_eq!(Format::Mp4.category(), FileCategory::Video);
        assert_eq!(Format::Mp3.category(), FileCategory::Audio);
        assert_eq!(Format::Pdf.category(), FileCategory::Document);
        assert_eq!(Format::Svg.category(), FileCategory::Vector);
    }

    #[test]
    fn compatible_targets_excludes_self() {
        let targets = Format::Png.compatible_targets();
        assert!(!targets.contains(&Format::Png));
        assert!(targets.contains(&Format::Jpg));
    }

    #[test]
    fn extension_roundtrip() {
        let fmt = Format::WebP;
        assert_eq!(Format::from_extension(fmt.extension()), Some(fmt));
    }

    #[test]
    fn svg_targets_include_raster() {
        let targets = Format::Svg.compatible_targets();
        assert!(targets.contains(&Format::Png));
        assert!(targets.contains(&Format::Pdf));
    }

    #[test]
    fn serde_roundtrip() {
        let json = serde_json::to_string(&Format::WebP).unwrap();
        assert_eq!(json, r#""webp""#);
        let back: Format = serde_json::from_str(&json).unwrap();
        assert_eq!(back, Format::WebP);
    }
}
