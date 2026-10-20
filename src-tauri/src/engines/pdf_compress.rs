//! In-process PDF compression in the spirit of Ghostscript's pdfwrite
//! presets: downsample photographic images to a target resolution, re-encode
//! them as JPEG with mozjpeg, compress uncompressed streams, drop unused
//! objects and write object streams. Vector content, text and fonts are
//! copied unchanged, so pages render the same apart from image detail.

use std::collections::{HashMap, HashSet};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId, Stream};
use tokio_util::sync::CancellationToken;

use crate::engines::codecs::{encode_jpeg, RgbaFrame};
use crate::error::ConversionError;

/// Images are left alone unless they are this much sharper than the target,
/// as in Ghostscript's default downsample threshold.
const DOWNSAMPLE_THRESHOLD: f32 = 1.5;
const MAX_FORM_DEPTH: usize = 8;
const MAX_IMAGE_PIXELS: usize = 200_000_000;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    /// Resolution images are downsampled to, in pixels per inch on the page.
    pub resolution: f32,
    pub jpeg_quality: u8,
    /// How much sharper than `resolution` an image must be before it is
    /// downsampled.
    pub threshold: f32,
}

impl Settings {
    /// Ghostscript's /prepress, /ebook and /screen resolutions.
    pub const HIGH: Self = Self {
        resolution: 300.0,
        jpeg_quality: 90,
        threshold: DOWNSAMPLE_THRESHOLD,
    };
    pub const BALANCED: Self = Self {
        resolution: 150.0,
        jpeg_quality: 80,
        threshold: DOWNSAMPLE_THRESHOLD,
    };
    pub const SMALLEST: Self = Self {
        resolution: 72.0,
        jpeg_quality: 60,
        threshold: DOWNSAMPLE_THRESHOLD,
    };

    /// A target-size candidate: downsample anything above `resolution`.
    pub fn exact(resolution: u16, jpeg_quality: u8) -> Self {
        Self {
            resolution: f32::from(resolution),
            jpeg_quality,
            threshold: 1.0,
        }
    }
}

fn pdf_error(message: &str, detail: impl std::fmt::Display) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: detail.to_string(),
        exit_code: None,
    }
}

/// Compress `document` in place.
pub fn compress(
    document: &mut Document,
    settings: Settings,
    cancel_token: &CancellationToken,
) -> Result<(), ConversionError> {
    let placements = image_placements(document);
    for (image_id, scale) in placements {
        if cancel_token.is_cancelled() {
            return Err(ConversionError::Cancelled);
        }
        recompress_image(document, image_id, scale, settings);
    }
    document.prune_objects();
    document.delete_zero_length_streams();
    document.compress();
    Ok(())
}

/// Serialise with object and cross-reference streams, the most compact form
/// readers since PDF 1.5 accept.
pub fn save(document: &mut Document, path: &std::path::Path) -> Result<u64, ConversionError> {
    let mut file = std::io::BufWriter::new(
        std::fs::File::create(path).map_err(|error| pdf_error("The PDF could not be written", error))?,
    );
    if document.version.as_str() < "1.5" {
        document.version = "1.5".into();
    }
    let options = lopdf::SaveOptions::builder()
        .use_object_streams(true)
        .use_xref_streams(true)
        .build();
    document
        .save_with_options(&mut file, options)
        .and_then(|()| std::io::Write::flush(&mut file))
        .map_err(|error| pdf_error("The PDF could not be written", error))?;
    drop(file);
    crate::engines::verification::file_with_signature(path, b"%PDF-")
}

/// For every image drawn on a page, the largest pixels-per-point ratio it is
/// shown at (pixels per inch is this times 72). Images reached any other way
/// (patterns, annotations) are absent and so never downsampled.
fn image_placements(document: &Document) -> HashMap<ObjectId, f32> {
    let mut placements = HashMap::new();
    for page_id in document.get_pages().into_values() {
        let Ok(content) = document.get_and_decode_page_content(page_id) else {
            continue;
        };
        let resources = page_xobjects(document, page_id);
        walk_content(
            document,
            &content.operations,
            &resources,
            [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
            0,
            &mut placements,
            &mut HashSet::new(),
        );
    }
    placements
}

fn page_xobjects(document: &Document, page_id: ObjectId) -> Dictionary {
    let mut xobjects = Dictionary::new();
    let Ok((direct, inherited)) = document.get_page_resources(page_id) else {
        return xobjects;
    };
    let resources = direct
        .into_iter()
        .cloned()
        .chain(inherited.into_iter().filter_map(|id| document.get_dictionary(id).ok().cloned()));
    for dictionary in resources {
        if let Ok(entries) = dictionary
            .get(b"XObject")
            .and_then(|value| resolve(document, value).as_dict().cloned())
        {
            for (name, value) in entries.iter() {
                if !xobjects.has(name) {
                    xobjects.set(name.clone(), value.clone());
                }
            }
        }
    }
    xobjects
}

fn resolve<'a>(document: &'a Document, object: &'a Object) -> &'a Object {
    match object {
        Object::Reference(id) => document.get_object(*id).unwrap_or(object),
        other => other,
    }
}

type Matrix = [f32; 6];

fn multiply(a: Matrix, b: Matrix) -> Matrix {
    [
        a[0] * b[0] + a[1] * b[2],
        a[0] * b[1] + a[1] * b[3],
        a[2] * b[0] + a[3] * b[2],
        a[2] * b[1] + a[3] * b[3],
        a[4] * b[0] + a[5] * b[2] + b[4],
        a[4] * b[1] + a[5] * b[3] + b[5],
    ]
}

fn numbers(operands: &[Object]) -> Option<Matrix> {
    let values = operands
        .iter()
        .map(|value| value.as_float().ok())
        .collect::<Option<Vec<_>>>()?;
    values.try_into().ok()
}

#[allow(clippy::too_many_arguments)]
fn walk_content(
    document: &Document,
    operations: &[lopdf::content::Operation],
    xobjects: &Dictionary,
    base: Matrix,
    depth: usize,
    placements: &mut HashMap<ObjectId, f32>,
    visiting: &mut HashSet<ObjectId>,
) {
    let mut stack = Vec::new();
    let mut matrix = base;
    for operation in operations {
        match operation.operator.as_str() {
            "q" => stack.push(matrix),
            "Q" => matrix = stack.pop().unwrap_or(base),
            "cm" => {
                if let Some(transform) = numbers(&operation.operands) {
                    matrix = multiply(transform, matrix);
                }
            }
            "Do" => {
                let Some(name) = operation.operands.first().and_then(|name| name.as_name().ok())
                else {
                    continue;
                };
                let Ok(Object::Reference(id)) = xobjects.get(name).cloned() else {
                    continue;
                };
                let Ok(Object::Stream(stream)) = document.get_object(id) else {
                    continue;
                };
                match stream.dict.get(b"Subtype").and_then(Object::as_name) {
                    Ok(b"Image") => record_image(stream, id, matrix, placements),
                    Ok(b"Form") if depth < MAX_FORM_DEPTH && visiting.insert(id) => {
                        let form_matrix = stream
                            .dict
                            .get(b"Matrix")
                            .ok()
                            .and_then(|value| value.as_array().ok())
                            .and_then(|values| numbers(values))
                            .unwrap_or([1.0, 0.0, 0.0, 1.0, 0.0, 0.0]);
                        let form_xobjects = stream
                            .dict
                            .get(b"Resources")
                            .ok()
                            .and_then(|value| resolve(document, value).as_dict().ok())
                            .and_then(|resources| resources.get(b"XObject").ok())
                            .and_then(|value| resolve(document, value).as_dict().ok())
                            .cloned()
                            .unwrap_or_else(|| xobjects.clone());
                        if let Ok(content) = stream
                            .decompressed_content()
                            .map_err(|_| ())
                            .and_then(|bytes| Content::decode(&bytes).map_err(|_| ()))
                        {
                            walk_content(
                                document,
                                &content.operations,
                                &form_xobjects,
                                multiply(form_matrix, matrix),
                                depth + 1,
                                placements,
                                visiting,
                            );
                        }
                        visiting.remove(&id);
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

fn record_image(stream: &Stream, id: ObjectId, matrix: Matrix, placements: &mut HashMap<ObjectId, f32>) {
    let (Ok(width), Ok(height)) = (
        stream.dict.get(b"Width").and_then(Object::as_i64),
        stream.dict.get(b"Height").and_then(Object::as_i64),
    ) else {
        return;
    };
    // The image fills the unit square, so the matrix columns are its drawn
    // size in points.
    let drawn_width = matrix[0].hypot(matrix[1]);
    let drawn_height = matrix[2].hypot(matrix[3]);
    if drawn_width < 0.01 || drawn_height < 0.01 {
        return;
    }
    let scale = (width as f32 / drawn_width).max(height as f32 / drawn_height);
    let entry = placements.entry(id).or_insert(0.0);
    *entry = entry.max(scale);
}

enum Channels {
    Gray,
    Rgb,
}

/// Replace one image with a downsampled JPEG when that is smaller. Images
/// this cannot decode exactly are left untouched.
fn recompress_image(document: &mut Document, id: ObjectId, pixels_per_point: f32, settings: Settings) {
    let Ok(Object::Stream(stream)) = document.get_object(id) else {
        return;
    };
    let Some(decoded) = decode_image(document, stream) else {
        return;
    };
    let (channels, width, height, samples) = decoded;
    let ppi = pixels_per_point * 72.0;
    let factor = if ppi > settings.resolution * settings.threshold {
        settings.resolution / ppi
    } else {
        1.0
    };
    let new_width = ((width as f32 * factor).round() as usize).max(1);
    let new_height = ((height as f32 * factor).round() as usize).max(1);
    let samples = if (new_width, new_height) != (width, height) {
        resample(&samples, width, height, new_width, new_height, channel_count(&channels))
    } else {
        samples
    };

    // Grayscale stays single-channel so the image's colour space stays valid.
    let jpeg = match channels {
        Channels::Gray => encode_gray_jpeg(&samples, new_width, new_height, settings.jpeg_quality),
        Channels::Rgb => {
            let frame = RgbaFrame {
                pixels: to_rgba(&samples),
                width: new_width as u32,
                height: new_height as u32,
            };
            encode_jpeg(&frame, settings.jpeg_quality, &[]).ok()
        }
    };
    let Some(jpeg) = jpeg.filter(|jpeg| jpeg.len() < stream.content.len()) else {
        return;
    };

    let mut dictionary = stream.dict.clone();
    dictionary.set("Filter", Object::Name(b"DCTDecode".to_vec()));
    dictionary.remove(b"DecodeParms");
    dictionary.set("Width", Object::Integer(new_width as i64));
    dictionary.set("Height", Object::Integer(new_height as i64));
    dictionary.set("BitsPerComponent", Object::Integer(8));
    let soft_mask = dictionary.get(b"SMask").and_then(Object::as_reference).ok();
    let mut replacement = Stream::new(dictionary, jpeg);
    replacement.allows_compression = false;
    document.objects.insert(id, Object::Stream(replacement));

    // A soft mask has its own dimensions; resample it by the same factor and
    // keep it lossless.
    if let Some(mask_id) = soft_mask.filter(|_| factor < 1.0) {
        resample_mask(document, mask_id, factor);
    }
}

fn channel_count(channels: &Channels) -> usize {
    match channels {
        Channels::Gray => 1,
        Channels::Rgb => 3,
    }
}

fn to_rgba(samples: &[u8]) -> Vec<u8> {
    samples
        .as_chunks::<3>()
        .0
        .iter()
        .flat_map(|pixel| [pixel[0], pixel[1], pixel[2], 255])
        .collect()
}

fn encode_gray_jpeg(samples: &[u8], width: usize, height: usize, quality: u8) -> Option<Vec<u8>> {
    let encode = || -> std::io::Result<Vec<u8>> {
        let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
        compress.set_size(width, height);
        compress.set_quality(f32::from(quality));
        let mut started = compress.start_compress(Vec::new())?;
        started.write_scanlines(samples)?;
        started.finish()
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(encode))
        .ok()?
        .ok()
}

/// The colour space when it is one this can re-encode as JPEG without
/// changing colours: DeviceGray, DeviceRGB, or an ICC profile with one or
/// three components.
fn image_channels(document: &Document, dictionary: &Dictionary) -> Option<Channels> {
    let space = resolve(document, dictionary.get(b"ColorSpace").ok()?);
    match space {
        Object::Name(name) if name == b"DeviceGray" => Some(Channels::Gray),
        Object::Name(name) if name == b"DeviceRGB" => Some(Channels::Rgb),
        Object::Array(items) if items.first().and_then(|item| item.as_name().ok()) == Some(b"ICCBased") => {
            let profile = items.get(1).map(|item| resolve(document, item))?;
            match profile.as_stream().ok()?.dict.get(b"N").and_then(Object::as_i64).ok()? {
                1 => Some(Channels::Gray),
                3 => Some(Channels::Rgb),
                _ => None,
            }
        }
        _ => None,
    }
}

fn decode_image(document: &Document, stream: &Stream) -> Option<(Channels, usize, usize, Vec<u8>)> {
    let dictionary = &stream.dict;
    if dictionary.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Image")
        || dictionary.get(b"ImageMask").and_then(Object::as_bool).unwrap_or(false)
        || dictionary.has(b"Decode")
        || dictionary.has(b"Mask")
        || dictionary.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8)
    {
        return None;
    }
    let channels = image_channels(document, dictionary)?;
    let width = usize::try_from(dictionary.get(b"Width").and_then(Object::as_i64).ok()?).ok()?;
    let height = usize::try_from(dictionary.get(b"Height").and_then(Object::as_i64).ok()?).ok()?;
    if width == 0 || height == 0 || width.saturating_mul(height) > MAX_IMAGE_PIXELS {
        return None;
    }
    let expected = width * height * channel_count(&channels);
    let filters = match dictionary.get(b"Filter").ok() {
        None => Vec::new(),
        Some(Object::Name(name)) => vec![name.clone()],
        Some(Object::Array(items)) => items
            .iter()
            .map(|item| item.as_name().map(<[u8]>::to_vec).ok())
            .collect::<Option<Vec<_>>>()?,
        _ => return None,
    };
    let samples = match filters.as_slice() {
        [filter] if filter == b"DCTDecode" => decode_jpeg(&stream.content, &channels)?,
        [] => stream.content.clone(),
        [filter] if filter == b"FlateDecode" => stream.decompressed_content().ok()?,
        _ => return None,
    };
    (samples.len() == expected).then_some((channels, width, height, samples))
}

fn decode_jpeg(data: &[u8], channels: &Channels) -> Option<Vec<u8>> {
    let decode = || -> std::io::Result<Vec<u8>> {
        let decompress = mozjpeg::Decompress::new_mem(data)?;
        // CMYK and YCCK JPEGs would need Adobe inversion handling; skip them.
        if matches!(
            decompress.color_space(),
            mozjpeg::ColorSpace::JCS_CMYK | mozjpeg::ColorSpace::JCS_YCCK
        ) {
            return Err(std::io::Error::other("CMYK JPEG"));
        }
        let mut started = match channels {
            Channels::Gray => decompress.grayscale()?,
            Channels::Rgb => decompress.rgb()?,
        };
        let samples = started.read_scanlines::<u8>()?;
        started.finish()?;
        Ok(samples)
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(decode))
        .ok()?
        .ok()
}

fn resample_mask(document: &mut Document, mask_id: ObjectId, factor: f32) {
    let Ok(Object::Stream(mask)) = document.get_object(mask_id) else {
        return;
    };
    let dictionary = &mask.dict;
    let (Ok(width), Ok(height)) = (
        dictionary.get(b"Width").and_then(Object::as_i64),
        dictionary.get(b"Height").and_then(Object::as_i64),
    ) else {
        return;
    };
    let (width, height) = (width as usize, height as usize);
    if dictionary.get(b"BitsPerComponent").and_then(Object::as_i64).ok() != Some(8)
        || dictionary.has(b"Decode")
    {
        return;
    }
    let Ok(samples) = (if dictionary.has(b"Filter") {
        mask.decompressed_content()
    } else {
        Ok(mask.content.clone())
    }) else {
        return;
    };
    if samples.len() != width * height {
        return;
    }
    let new_width = ((width as f32 * factor).round() as usize).max(1);
    let new_height = ((height as f32 * factor).round() as usize).max(1);
    let resampled = resample(&samples, width, height, new_width, new_height, 1);
    let mut dictionary = dictionary.clone();
    dictionary.remove(b"Filter");
    dictionary.remove(b"DecodeParms");
    dictionary.set("Width", Object::Integer(new_width as i64));
    dictionary.set("Height", Object::Integer(new_height as i64));
    let mut replacement = Stream::new(dictionary, resampled);
    let _ = replacement.compress();
    document.objects.insert(mask_id, Object::Stream(replacement));
}

/// Area-average downsampling: each output sample is the mean of the source
/// samples it covers, which avoids aliasing at any reduction factor.
fn resample(
    samples: &[u8],
    width: usize,
    height: usize,
    new_width: usize,
    new_height: usize,
    channels: usize,
) -> Vec<u8> {
    let mut output = vec![0u8; new_width * new_height * channels];
    let x_ratio = width as f32 / new_width as f32;
    let y_ratio = height as f32 / new_height as f32;
    for y in 0..new_height {
        let top = (y as f32 * y_ratio) as usize;
        let bottom = (((y + 1) as f32 * y_ratio).ceil() as usize).clamp(top + 1, height);
        for x in 0..new_width {
            let left = (x as f32 * x_ratio) as usize;
            let right = (((x + 1) as f32 * x_ratio).ceil() as usize).clamp(left + 1, width);
            let count = ((bottom - top) * (right - left)) as u32;
            for channel in 0..channels {
                let mut sum = 0u32;
                for row in top..bottom {
                    let start = (row * width + left) * channels + channel;
                    for column in 0..(right - left) {
                        sum += u32::from(samples[start + column * channels]);
                    }
                }
                output[(y * new_width + x) * channels + channel] = ((sum + count / 2) / count) as u8;
            }
        }
    }
    output
}

#[cfg(test)]
pub(crate) mod fixtures {
    use lopdf::{dictionary, Document, Object, Stream};
    use std::path::Path;

    /// One page with a 1200x900 noisy RGB image drawn 4x3 inches (300 ppi),
    /// stored uncompressed, plus a line of text.
    pub fn photo_pdf(path: &Path) {
        let (width, height) = (1200usize, 900usize);
        let mut state = 0x1234_5678_u32;
        let pixels = (0..width * height * 3)
            .map(|index| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                let gradient = ((index / 3) % width * 255 / width) as u32;
                ((gradient + (state & 0x1f)) & 0xff) as u8
            })
            .collect::<Vec<_>>();
        let mut document = Document::with_version("1.7");
        let pages = document.new_object_id();
        let mut image = Stream::new(
            dictionary! {
                "Type" => "XObject", "Subtype" => "Image",
                "Width" => width as i64, "Height" => height as i64,
                "ColorSpace" => "DeviceRGB", "BitsPerComponent" => 8,
            },
            pixels,
        );
        image.allows_compression = false;
        let image = document.add_object(image);
        let font = document.add_object(dictionary! {
            "Type" => "Font", "Subtype" => "Type1", "BaseFont" => "Helvetica",
        });
        let content = b"q 288 0 0 216 72 400 cm /Im1 Do Q BT /F1 18 Tf 72 360 Td (Caption) Tj ET".to_vec();
        let content = document.add_object(Stream::new(dictionary! {}, content));
        let page = document.add_object(dictionary! {
            "Type" => "Page", "Parent" => pages,
            "MediaBox" => vec![0.into(), 0.into(), 612.into(), 792.into()],
            "Contents" => content,
            "Resources" => dictionary! {
                "XObject" => dictionary! { "Im1" => image },
                "Font" => dictionary! { "F1" => font },
            },
        });
        document.objects.insert(
            pages,
            Object::Dictionary(dictionary! {
                "Type" => "Pages", "Kids" => vec![page.into()], "Count" => 1,
            }),
        );
        let catalog = document.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages });
        document.trailer.set("Root", catalog);
        document.save(path).expect("save photo fixture");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn image_size(document: &Document) -> (i64, i64, Vec<u8>) {
        document
            .objects
            .values()
            .filter_map(|object| object.as_stream().ok())
            .find(|stream| stream.dict.get(b"Subtype").and_then(Object::as_name).ok() == Some(b"Image"))
            .map(|stream| {
                (
                    stream.dict.get(b"Width").and_then(Object::as_i64).unwrap(),
                    stream.dict.get(b"Height").and_then(Object::as_i64).unwrap(),
                    stream.dict.get(b"Filter").and_then(Object::as_name).unwrap_or(b"").to_vec(),
                )
            })
            .expect("image")
    }

    #[test]
    fn finds_drawn_image_resolution_through_transforms() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.pdf");
        fixtures::photo_pdf(&input);
        let document = Document::load(&input).expect("load");
        let placements = image_placements(&document);
        assert_eq!(placements.len(), 1);
        let ppi = placements.values().next().copied().unwrap() * 72.0;
        assert!((ppi - 300.0).abs() < 0.5, "{ppi}");
    }

    #[test]
    fn downsamples_to_each_preset_and_keeps_text() {
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.pdf");
        fixtures::photo_pdf(&input);
        let source_size = std::fs::metadata(&input).expect("size").len();

        let mut previous = u64::MAX;
        for (settings, expected) in [
            (Settings::HIGH, (1200, 900)),
            (Settings::BALANCED, (600, 450)),
            (Settings::SMALLEST, (288, 216)),
        ] {
            let mut document = Document::load(&input).expect("load");
            compress(&mut document, settings, &CancellationToken::new()).expect("compress");
            let output = directory.path().join("out.pdf");
            let size = save(&mut document, &output).expect("save");
            assert!(size < source_size, "{settings:?} shrank the file");
            assert!(size < previous, "{settings:?} is smaller than the previous preset");
            previous = size;
            let reloaded = Document::load(&output).expect("reload");
            let (width, height, filter) = image_size(&reloaded);
            assert_eq!((width, height), expected, "{settings:?}");
            assert_eq!(filter, b"DCTDecode");
            assert!(reloaded.extract_text(&[1]).unwrap_or_default().contains("Caption"));
        }
    }

    #[test]
    fn area_average_resampling_preserves_means() {
        let samples = [0u8, 255, 0, 255, 100, 100, 100, 100];
        // Each half averages 0, 255, 100 and 100.
        assert_eq!(resample(&samples, 4, 2, 2, 1, 1), vec![114, 114]);
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let directory = tempfile::tempdir().expect("tempdir");
        let input = directory.path().join("photo.pdf");
        fixtures::photo_pdf(&input);
        let mut document = Document::load(&input).expect("load");
        assert!(matches!(
            compress(&mut document, Settings::BALANCED, &cancelled),
            Err(ConversionError::Cancelled)
        ));
    }
}
