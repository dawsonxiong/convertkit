//! In-process encoders and container writers for the formats ImageIO cannot
//! write well: WebP (still and animated), JPEG with 4:2:0 chroma, and ICO.
//! Everything here works on decoded RGBA pixels or raw container bytes, so it
//! is testable without the native helper.

use std::io::{BufReader, Read};
use std::path::Path;

use crate::error::ConversionError;

pub const WEBP_MAX_DIMENSION: u32 = 16_383;
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";
const PIXELS_MAGIC: &[u8] = b"CKPIXEL1";

/// One decoded frame of 8-bit RGBA pixels.
pub struct RgbaFrame {
    pub pixels: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

fn codec_error(message: &str, detail: impl std::fmt::Display) -> ConversionError {
    ConversionError::ProcessFailed {
        message: message.into(),
        stderr: detail.to_string(),
        exit_code: None,
    }
}

/// The frames the image helper's `pixels` command wrote: each one is
/// `CKPIXEL1`, little-endian u32 width and height, then straight RGBA rows.
pub fn read_pixels(path: &Path) -> Result<Vec<RgbaFrame>, ConversionError> {
    const MESSAGE: &str = "An intermediate image could not be decoded";
    let file = std::fs::File::open(path).map_err(|error| codec_error(MESSAGE, error))?;
    let mut remaining = file
        .metadata()
        .map_err(|error| codec_error(MESSAGE, error))?
        .len();
    let mut reader = BufReader::new(file);
    let mut frames = Vec::new();
    while remaining > 0 {
        let mut header = [0; 16];
        reader
            .read_exact(&mut header)
            .map_err(|error| codec_error(MESSAGE, error))?;
        if &header[..8] != PIXELS_MAGIC {
            return Err(codec_error(MESSAGE, "unexpected frame header"));
        }
        let width = u32::from_le_bytes([header[8], header[9], header[10], header[11]]);
        let height = u32::from_le_bytes([header[12], header[13], header[14], header[15]]);
        let length = u64::from(width) * u64::from(height) * 4;
        remaining = remaining
            .checked_sub(16 + length)
            .filter(|_| width > 0 && height > 0)
            .ok_or_else(|| codec_error(MESSAGE, "truncated frame"))?;
        let mut pixels = vec![0; length as usize];
        reader
            .read_exact(&mut pixels)
            .map_err(|error| codec_error(MESSAGE, error))?;
        frames.push(RgbaFrame {
            pixels,
            width,
            height,
        });
    }
    if frames.is_empty() {
        return Err(codec_error(MESSAGE, "no frames"));
    }
    Ok(frames)
}

pub fn png_dimensions(data: &[u8]) -> Option<(u32, u32)> {
    let header = data
        .get(16..24)
        .filter(|_| data.starts_with(PNG_SIGNATURE))?;
    Some((
        u32::from_be_bytes(header[..4].try_into().ok()?),
        u32::from_be_bytes(header[4..].try_into().ok()?),
    ))
}

// ---------------------------------------------------------------------------
// WebP
// ---------------------------------------------------------------------------

fn webp_config(quality: Option<u8>, method: u8) -> Result<webp::WebPConfig, ConversionError> {
    let mut config = webp::WebPConfig::new()
        .map_err(|()| codec_error("The WebP encoder could not start", "invalid config"))?;
    match quality {
        Some(quality) => config.quality = f32::from(quality.clamp(1, 100)),
        None => {
            config.lossless = 1;
            config.quality = 100.0;
        }
    }
    config.method = i32::from(method.min(6));
    Ok(config)
}

fn check_webp_size(width: u32, height: u32) -> Result<(), ConversionError> {
    if width > WEBP_MAX_DIMENSION || height > WEBP_MAX_DIMENSION {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("This image is {width} × {height} px"),
            output: format!("WebP, which allows at most {WEBP_MAX_DIMENSION} px per side"),
        });
    }
    Ok(())
}

/// Lossy at `quality`, or lossless when `quality` is `None`.
pub fn encode_webp(
    frame: &RgbaFrame,
    quality: Option<u8>,
    method: u8,
) -> Result<Vec<u8>, ConversionError> {
    check_webp_size(frame.width, frame.height)?;
    let config = webp_config(quality, method)?;
    let encoded = webp::Encoder::from_rgba(&frame.pixels, frame.width, frame.height)
        .encode_advanced(&config)
        .map_err(|error| {
            codec_error(
                "The WebP encoder could not encode this image",
                format!("{error:?}"),
            )
        })?;
    Ok(encoded.to_vec())
}

/// An animated WebP. Every frame must share the canvas size, and
/// `durations_ms` gives each frame's display time.
pub fn encode_webp_animation(
    frames: &[RgbaFrame],
    durations_ms: &[u32],
    loop_count: i32,
    quality: Option<u8>,
    method: u8,
) -> Result<Vec<u8>, ConversionError> {
    use libwebp_sys as sys;

    let first = frames
        .first()
        .ok_or_else(|| codec_error("The WebP encoder received no frames", ""))?;
    let (width, height) = (first.width, first.height);
    check_webp_size(width, height)?;
    if frames.len() != durations_ms.len()
        || frames
            .iter()
            .any(|frame| frame.width != width || frame.height != height)
    {
        return Err(ConversionError::UnsupportedConversion {
            input: "This image has frames of different sizes".into(),
            output: "an animated WebP, whose frames share one canvas".into(),
        });
    }
    let config = webp_config(quality, method)?;
    let fail =
        |message: &str| codec_error("The WebP encoder could not encode this animation", message);
    let abi = sys::WEBP_MUX_ABI_VERSION as i32;

    struct Encoder(*mut sys::WebPAnimEncoder);
    impl Drop for Encoder {
        fn drop(&mut self) {
            // SAFETY: the pointer came from WebPAnimEncoderNewInternal and is
            // deleted exactly once.
            unsafe { sys::WebPAnimEncoderDelete(self.0) }
        }
    }

    // SAFETY: every libwebp structure is initialised by its own init call
    // before use, pixel slices outlive the calls that read them, and each
    // picture and the assembled data are freed by libwebp's own functions.
    unsafe {
        let mut options = std::mem::MaybeUninit::<sys::WebPAnimEncoderOptions>::zeroed();
        if sys::WebPAnimEncoderOptionsInitInternal(options.as_mut_ptr(), abi) == 0 {
            return Err(fail("options"));
        }
        let mut options = options.assume_init();
        options.anim_params.loop_count = loop_count.max(0);
        let canvas_width = i32::try_from(width).map_err(|_| fail("width"))?;
        let canvas_height = i32::try_from(height).map_err(|_| fail("height"))?;
        let encoder = Encoder(sys::WebPAnimEncoderNewInternal(
            canvas_width,
            canvas_height,
            &options,
            abi,
        ));
        if encoder.0.is_null() {
            return Err(fail("encoder"));
        }

        let mut timestamp: i32 = 0;
        for (frame, duration) in frames.iter().zip(durations_ms) {
            let mut picture = std::mem::MaybeUninit::<sys::WebPPicture>::zeroed();
            if !sys::WebPPictureInit(picture.as_mut_ptr()) {
                return Err(fail("picture"));
            }
            let mut picture = picture.assume_init();
            picture.use_argb = 1;
            picture.width = canvas_width;
            picture.height = canvas_height;
            let imported =
                sys::WebPPictureImportRGBA(&mut picture, frame.pixels.as_ptr(), canvas_width * 4);
            let added = imported != 0
                && sys::WebPAnimEncoderAdd(encoder.0, &mut picture, timestamp, &config) != 0;
            sys::WebPPictureFree(&mut picture);
            if !added {
                return Err(fail(&anim_error(encoder.0)));
            }
            timestamp = timestamp.saturating_add(i32::try_from(*duration).unwrap_or(i32::MAX));
        }
        // A final empty frame fixes the last frame's duration.
        if sys::WebPAnimEncoderAdd(encoder.0, std::ptr::null_mut(), timestamp, std::ptr::null())
            == 0
        {
            return Err(fail(&anim_error(encoder.0)));
        }
        let mut data = std::mem::MaybeUninit::<sys::WebPData>::zeroed();
        if sys::WebPAnimEncoderAssemble(encoder.0, data.as_mut_ptr()) == 0 {
            return Err(fail(&anim_error(encoder.0)));
        }
        let mut data = data.assume_init();
        let bytes = std::slice::from_raw_parts(data.bytes, data.size).to_vec();
        sys::WebPDataClear(&mut data);
        Ok(bytes)
    }
}

/// # Safety
/// `encoder` must be a live libwebp animation encoder.
unsafe fn anim_error(encoder: *mut libwebp_sys::WebPAnimEncoder) -> String {
    let message = unsafe { libwebp_sys::WebPAnimEncoderGetError(encoder) };
    if message.is_null() {
        return "unknown error".into();
    }
    unsafe { std::ffi::CStr::from_ptr(message) }
        .to_string_lossy()
        .into_owned()
}

fn riff_chunks(data: &[u8]) -> Option<Vec<([u8; 4], &[u8])>> {
    if data.get(..4)? != b"RIFF" || data.get(8..12)? != b"WEBP" {
        return None;
    }
    let mut chunks = Vec::new();
    let mut offset = 12;
    while offset + 8 <= data.len() {
        let fourcc: [u8; 4] = data[offset..offset + 4].try_into().ok()?;
        let length = u32::from_le_bytes(data[offset + 4..offset + 8].try_into().ok()?) as usize;
        let body = data.get(offset + 8..offset + 8 + length)?;
        chunks.push((fourcc, body));
        offset += 8 + length + (length & 1);
    }
    Some(chunks)
}

/// Copy EXIF and XMP from `source` (a WebP) into `encoded`. Colour profiles
/// are not copied because the encoded pixels have been converted to sRGB.
pub fn copy_webp_metadata(encoded: &[u8], source: &[u8], width: u32, height: u32) -> Vec<u8> {
    let Some(source_chunks) = riff_chunks(source) else {
        return encoded.to_vec();
    };
    let metadata = source_chunks
        .into_iter()
        .filter(|(fourcc, _)| fourcc == b"EXIF" || fourcc == b"XMP ")
        .collect::<Vec<_>>();
    let Some(chunks) = riff_chunks(encoded).filter(|_| !metadata.is_empty()) else {
        return encoded.to_vec();
    };

    let has_exif = metadata.iter().any(|(fourcc, _)| fourcc == b"EXIF");
    let has_xmp = metadata.iter().any(|(fourcc, _)| fourcc == b"XMP ");
    let mut extended = match chunks.first() {
        Some((fourcc, body)) if fourcc == b"VP8X" && body.len() >= 10 => body.to_vec(),
        _ => {
            let alpha = chunks.iter().any(|(fourcc, body)| {
                fourcc == b"ALPH"
                    || (fourcc == b"VP8L"
                        && body.len() >= 5
                        && (u32::from_le_bytes([body[1], body[2], body[3], body[4]]) >> 28) & 1
                            == 1)
            });
            let mut header = vec![if alpha { 0x10 } else { 0 }, 0, 0, 0];
            header.extend(&(width.saturating_sub(1)).to_le_bytes()[..3]);
            header.extend(&(height.saturating_sub(1)).to_le_bytes()[..3]);
            header
        }
    };
    extended[0] |= if has_exif { 0x08 } else { 0 } | if has_xmp { 0x04 } else { 0 };

    let mut body = Vec::new();
    let mut push = |fourcc: &[u8; 4], data: &[u8]| {
        body.extend(fourcc);
        body.extend((data.len() as u32).to_le_bytes());
        body.extend(data);
        if data.len() & 1 == 1 {
            body.push(0);
        }
    };
    push(b"VP8X", &extended);
    for (fourcc, data) in &chunks {
        if fourcc != b"VP8X" && fourcc != b"EXIF" && fourcc != b"XMP " {
            push(fourcc, data);
        }
    }
    for (fourcc, data) in &metadata {
        push(fourcc, data);
    }
    let mut output = b"RIFF".to_vec();
    output.extend(((body.len() + 4) as u32).to_le_bytes());
    output.extend(b"WEBP");
    output.extend(body);
    output
}

// ---------------------------------------------------------------------------
// JPEG
// ---------------------------------------------------------------------------

/// An APPn segment to write after the JFIF header.
pub struct AppSegment {
    pub marker: u8,
    pub data: Vec<u8>,
}

/// Metadata segments worth keeping from a source JPEG: EXIF and XMP (APP1)
/// and IPTC (APP13). JFIF, ICC and Adobe colour segments describe the old
/// encoding and are rebuilt by the encoder instead.
pub fn jpeg_metadata_segments(source: &[u8]) -> Vec<AppSegment> {
    let mut segments = Vec::new();
    if source.get(..2) != Some(&[0xff, 0xd8]) {
        return segments;
    }
    let mut offset = 2;
    while offset + 4 <= source.len() && source[offset] == 0xff {
        let marker = source[offset + 1];
        if marker == 0xda || marker == 0xd9 {
            break;
        }
        let length = usize::from(u16::from_be_bytes([source[offset + 2], source[offset + 3]]));
        let Some(data) = source
            .get(offset + 4..offset + 2 + length)
            .filter(|_| length >= 2)
        else {
            break;
        };
        if marker == 0xe1 || marker == 0xed {
            segments.push(AppSegment {
                marker: marker - 0xe0,
                data: data.to_vec(),
            });
        }
        offset += 2 + length;
    }
    segments
}

/// A minimal EXIF APP1 payload carrying only the orientation tag.
pub fn orientation_exif(orientation: u8) -> Vec<u8> {
    let mut exif = b"Exif\0\0II*\0".to_vec();
    exif.extend(8u32.to_le_bytes()); // first IFD offset
    exif.extend(1u16.to_le_bytes()); // one entry
    exif.extend(0x0112u16.to_le_bytes()); // Orientation
    exif.extend(3u16.to_le_bytes()); // SHORT
    exif.extend(1u32.to_le_bytes()); // count
    exif.extend(u32::from(orientation).to_le_bytes());
    exif.extend(0u32.to_le_bytes()); // no next IFD
    exif
}

/// Byte offset of the orientation value inside an EXIF APP1 payload, and
/// whether the payload is little-endian.
fn orientation_offset(exif: &[u8]) -> Option<(usize, bool)> {
    let tiff = exif.strip_prefix(b"Exif\0\0")?;
    let little = match tiff.get(..2)? {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let read16 = |offset: usize| -> Option<u16> {
        let pair: [u8; 2] = tiff.get(offset..offset + 2)?.try_into().ok()?;
        Some(if little {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        })
    };
    let quad: [u8; 4] = tiff.get(4..8)?.try_into().ok()?;
    let ifd = if little {
        u32::from_le_bytes(quad)
    } else {
        u32::from_be_bytes(quad)
    } as usize;
    for index in 0..usize::from(read16(ifd)?) {
        let entry = ifd + 2 + index * 12;
        if read16(entry)? == 0x0112 {
            tiff.get(entry + 8..entry + 10)?;
            return Some((6 + entry + 8, little));
        }
    }
    None
}

/// The orientation recorded in a JPEG's EXIF, if any.
pub fn jpeg_orientation(data: &[u8]) -> Option<u8> {
    jpeg_metadata_segments(data).iter().find_map(|segment| {
        let (offset, little) = orientation_offset(&segment.data)?;
        let pair = [segment.data[offset], segment.data[offset + 1]];
        let value = if little {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
        };
        u8::try_from(value)
            .ok()
            .filter(|value| (1..=8).contains(value))
    })
}

/// Rewrite the orientation tag in an EXIF APP1 payload in place. Used after
/// pixels have been rotated so a kept EXIF block cannot rotate them again.
pub fn set_exif_orientation(exif: &mut [u8], orientation: u16) {
    if let Some((offset, little)) = orientation_offset(exif) {
        exif[offset..offset + 2].copy_from_slice(&if little {
            orientation.to_le_bytes()
        } else {
            orientation.to_be_bytes()
        });
    }
}

fn panic_message(panic: Box<dyn std::any::Any + Send>) -> String {
    panic
        .downcast::<String>()
        .map(|message| *message)
        .or_else(|panic| {
            panic
                .downcast::<&str>()
                .map(|message| (*message).to_owned())
        })
        .unwrap_or_else(|_| "libjpeg stopped unexpectedly".into())
}

/// Encode with mozjpeg: 4:2:0 chroma, trellis quantisation, optimised
/// Huffman tables and progressive scans.
pub fn encode_jpeg(
    frame: &RgbaFrame,
    quality: u8,
    segments: &[AppSegment],
) -> Result<Vec<u8>, ConversionError> {
    if frame.width > 65_535 || frame.height > 65_535 {
        return Err(ConversionError::UnsupportedConversion {
            input: format!("This image is {} × {} px", frame.width, frame.height),
            output: "JPEG, which allows at most 65,535 px per side".into(),
        });
    }
    let encode = || -> std::io::Result<Vec<u8>> {
        let mut compress = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_EXT_RGBA);
        compress.set_size(frame.width as usize, frame.height as usize);
        compress.set_quality(f32::from(quality.clamp(1, 100)));
        compress.set_chroma_sampling_pixel_sizes((2, 2), (2, 2));
        let mut started = compress.start_compress(Vec::new())?;
        for segment in segments {
            started.write_marker(mozjpeg::Marker::APP(segment.marker), &segment.data);
        }
        started.write_scanlines(&frame.pixels)?;
        started.finish()
    };
    // libjpeg reports fatal errors by unwinding through the error manager.
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(encode)) {
        Ok(Ok(encoded)) => Ok(encoded),
        Ok(Err(error)) => Err(codec_error(
            "The JPEG encoder could not encode this image",
            error,
        )),
        Err(panic) => Err(codec_error(
            "The JPEG encoder could not encode this image",
            panic_message(panic),
        )),
    }
}

/// Losslessly re-encode a JPEG's existing DCT coefficients with optimised
/// Huffman tables and progressive scans, the transcode jpegtran and jpegoptim
/// perform. Pixels are bit-identical. Metadata markers are kept on request.
pub fn optimize_jpeg_lossless(
    data: &[u8],
    keep_metadata: bool,
) -> Result<Vec<u8>, ConversionError> {
    let transcode = || {
        // SAFETY: see `transcode_coefficients`.
        unsafe { transcode_coefficients(data, keep_metadata) }
    };
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(transcode))
        .map_err(|panic| codec_error("JPEG recompression failed", panic_message(panic)))
}

extern "C" {
    fn free(pointer: *mut std::ffi::c_void);
}

#[cold]
extern "C-unwind" fn unwind_jpeg_error(cinfo: &mut mozjpeg_sys::jpeg_common_struct) {
    // SAFETY: libjpeg passes a live struct whose `err` points at our manager.
    let code = unsafe { cinfo.err.as_ref() }.map_or(0, |err| err.msg_code);
    std::panic::resume_unwind(Box::new(format!("libjpeg error {code}")));
}

extern "C-unwind" fn silence_jpeg_message(_: &mut mozjpeg_sys::jpeg_common_struct, _: i32) {}

/// # Safety
/// Every libjpeg structure is boxed so its address stays fixed, created
/// before use and destroyed by a guard even when libjpeg unwinds from a
/// fatal error. The memory destination is freed with the allocator libjpeg
/// used for it.
unsafe fn transcode_coefficients(data: &[u8], keep_metadata: bool) -> Vec<u8> {
    use mozjpeg_sys as sys;

    struct Decompress(Box<sys::jpeg_decompress_struct>);
    impl Drop for Decompress {
        fn drop(&mut self) {
            unsafe { sys::jpeg_destroy_decompress(&mut self.0) }
        }
    }
    struct Compress(Box<sys::jpeg_compress_struct>, *mut u8);
    impl Drop for Compress {
        fn drop(&mut self) {
            unsafe {
                sys::jpeg_destroy_compress(&mut self.0);
                if !self.1.is_null() {
                    free(self.1.cast());
                }
            }
        }
    }
    let error_manager = || {
        let mut manager: Box<sys::jpeg_error_mgr> = Box::new(unsafe { std::mem::zeroed() });
        unsafe { sys::jpeg_std_error(&mut manager) };
        manager.error_exit = Some(unwind_jpeg_error);
        manager.emit_message = Some(silence_jpeg_message);
        manager
    };

    unsafe {
        let mut source_errors = error_manager();
        let mut source = Decompress(Box::new(std::mem::zeroed()));
        source.0.common.err = &mut *source_errors;
        sys::jpeg_create_decompress(&mut *source.0);
        sys::jpeg_mem_src(&mut source.0, data.as_ptr(), data.len() as _);
        if keep_metadata {
            sys::jpeg_save_markers(&mut source.0, sys::jpeg_marker::COM as i32, 0xffff);
            for marker in 1..16 {
                sys::jpeg_save_markers(
                    &mut source.0,
                    sys::jpeg_marker::APP0 as i32 + marker,
                    0xffff,
                );
            }
        }
        sys::jpeg_read_header(&mut source.0, 1);
        let coefficients = sys::jpeg_read_coefficients(&mut source.0);

        let mut target_errors = error_manager();
        let mut target = Compress(Box::new(std::mem::zeroed()), std::ptr::null_mut());
        target.0.common.err = &mut *target_errors;
        sys::jpeg_create_compress(&mut *target.0);
        sys::jpeg_copy_critical_parameters(&source.0, &mut target.0);
        target.0.optimize_coding = 1;
        sys::jpeg_simple_progression(&mut target.0);
        let mut size = 0;
        sys::jpeg_mem_dest(&mut target.0, &mut target.1, &mut size);
        sys::jpeg_write_coefficients(&mut target.0, coefficients);
        // Stripping must not rotate the picture the user sees.
        let orientation = (!keep_metadata)
            .then(|| jpeg_orientation(data))
            .flatten()
            .filter(|orientation| *orientation != 1);
        if let Some(orientation) = orientation {
            let exif = orientation_exif(orientation);
            sys::jpeg_write_marker(
                &mut target.0,
                sys::jpeg_marker::APP0 as i32 + 1,
                exif.as_ptr(),
                exif.len() as u32,
            );
        }
        let mut marker = source.0.marker_list;
        while let Some(saved) = marker.as_ref() {
            sys::jpeg_write_marker(
                &mut target.0,
                i32::from(saved.marker),
                saved.data,
                saved.data_length,
            );
            marker = saved.next;
        }
        sys::jpeg_finish_compress(&mut target.0);
        sys::jpeg_finish_decompress(&mut source.0);
        std::slice::from_raw_parts(target.1, size as usize).to_vec()
    }
}

// ---------------------------------------------------------------------------
// GIF
// ---------------------------------------------------------------------------

/// Decodes a GIF and yields each frame as it is displayed: composited onto the
/// canvas with the previous frame's disposal applied, background disposal
/// clearing to transparent as browsers do.
struct GifRenderer<R: std::io::Read> {
    decoder: gif::Decoder<R>,
    canvas: Vec<u8>,
    pending: Option<(gif::DisposalMethod, [usize; 4], Vec<u8>)>,
    width: usize,
}

impl<R: std::io::Read> GifRenderer<R> {
    fn new(reader: R) -> Result<Self, gif::DecodingError> {
        let mut options = gif::DecodeOptions::new();
        options.set_color_output(gif::ColorOutput::RGBA);
        let decoder = options.read_info(reader)?;
        let (width, height) = (usize::from(decoder.width()), usize::from(decoder.height()));
        Ok(Self {
            decoder,
            canvas: vec![0; width * height * 4],
            pending: None,
            width,
        })
    }

    /// The next displayed canvas and the source frame's delay.
    fn next(&mut self) -> Result<Option<(&[u8], u16)>, gif::DecodingError> {
        if let Some((dispose, [left, top, width, height], saved)) = self.pending.take() {
            for row in top..top + height {
                let start = (row * self.width + left) * 4;
                let span = &mut self.canvas[start..start + width * 4];
                match dispose {
                    gif::DisposalMethod::Background => span.fill(0),
                    gif::DisposalMethod::Previous => {
                        let offset = (row - top) * width * 4;
                        span.copy_from_slice(&saved[offset..offset + width * 4]);
                    }
                    _ => {}
                }
            }
        }
        let canvas_height = self.canvas.len() / 4 / self.width.max(1);
        let Some(frame) = self.decoder.read_next_frame()? else {
            return Ok(None);
        };
        // Frames reaching past the canvas are clipped, as decoders do.
        let left = usize::from(frame.left).min(self.width);
        let top = usize::from(frame.top).min(canvas_height);
        let width = usize::from(frame.width).min(self.width - left);
        let height = usize::from(frame.height).min(canvas_height - top);
        let mut saved = Vec::new();
        if frame.dispose == gif::DisposalMethod::Previous {
            for row in top..top + height {
                let start = (row * self.width + left) * 4;
                saved.extend_from_slice(&self.canvas[start..start + width * 4]);
            }
        }
        for row in 0..height {
            for column in 0..width {
                let source = (row * usize::from(frame.width) + column) * 4;
                let pixel = &frame.buffer[source..source + 4];
                if pixel[3] != 0 {
                    let target = ((top + row) * self.width + left + column) * 4;
                    self.canvas[target..target + 4].copy_from_slice(pixel);
                }
            }
        }
        self.pending = Some((frame.dispose, [left, top, width, height], saved));
        Ok(Some((&self.canvas, frame.delay)))
    }
}

/// Losslessly shrink an animated GIF the way `gifsicle -O` does: every frame
/// after the first is cropped to the pixels that change and paints unchanged
/// pixels transparent, which LZW compresses far better. The result is decoded
/// again and compared frame by frame; `None` means no safe smaller encoding.
pub fn optimize_gif(data: &[u8]) -> Option<Vec<u8>> {
    let mut source = GifRenderer::new(data).ok()?;
    let (width, height) = (source.decoder.width(), source.decoder.height());
    let repeat = source.decoder.repeat();
    let mut output = Vec::new();
    {
        let mut encoder = gif::Encoder::new(&mut output, width, height, &[]).ok()?;
        encoder.set_repeat(repeat).ok()?;
        let mut previous: Option<Vec<u8>> = None;
        while let Some((canvas, delay)) = source.next().ok()? {
            let frame = difference_frame(previous.as_deref(), canvas, usize::from(width), delay)?;
            encoder.write_frame(&frame).ok()?;
            previous = Some(canvas.to_vec());
        }
    }
    (output.len() < data.len() && renders_identically(data, &output)).then_some(output)
}

fn difference_frame(
    previous: Option<&[u8]>,
    canvas: &[u8],
    canvas_width: usize,
    delay: u16,
) -> Option<gif::Frame<'static>> {
    let canvas_height = canvas.len() / 4 / canvas_width.max(1);
    let changed = |index: usize| {
        previous.is_none_or(|previous| {
            previous[index * 4..index * 4 + 4] != canvas[index * 4..index * 4 + 4]
        })
    };
    // A pixel cannot turn transparent under "keep" disposal; leave such
    // animations to the re-encode candidates.
    if let Some(previous) = previous {
        if (0..canvas.len() / 4)
            .any(|index| canvas[index * 4 + 3] == 0 && previous[index * 4 + 3] != 0)
        {
            return None;
        }
    }
    let (mut left, mut top, mut right, mut bottom) = (canvas_width, canvas_height, 0, 0);
    for row in 0..canvas_height {
        for column in 0..canvas_width {
            if changed(row * canvas_width + column) {
                left = left.min(column);
                right = right.max(column + 1);
                top = top.min(row);
                bottom = bottom.max(row + 1);
            }
        }
    }
    if right == 0 {
        // Nothing changed: a single transparent pixel keeps the frame and
        // its timing.
        (left, top, right, bottom) = (0, 0, 1, 1);
    }

    let mut palette: Vec<[u8; 3]> = Vec::new();
    let mut lookup = std::collections::HashMap::new();
    let mut transparent_needed = false;
    let mut indices = Vec::with_capacity((right - left) * (bottom - top));
    for row in top..bottom {
        for column in left..right {
            let index = row * canvas_width + column;
            let pixel = &canvas[index * 4..index * 4 + 4];
            if !changed(index) || pixel[3] == 0 {
                transparent_needed = true;
                indices.push(None);
                continue;
            }
            let color = [pixel[0], pixel[1], pixel[2]];
            let slot = *lookup.entry(color).or_insert_with(|| {
                palette.push(color);
                palette.len() - 1
            });
            indices.push(Some(slot));
        }
    }
    let transparent = transparent_needed.then_some(palette.len());
    let entries = palette.len() + usize::from(transparent_needed);
    if entries > 256 {
        return None;
    }
    let mut palette_bytes = palette.concat();
    palette_bytes.resize(entries.max(2).next_power_of_two() * 3, 0);
    Some(gif::Frame {
        delay,
        dispose: gif::DisposalMethod::Keep,
        transparent: transparent.map(|index| index as u8),
        left: left as u16,
        top: top as u16,
        width: (right - left) as u16,
        height: (bottom - top) as u16,
        palette: Some(palette_bytes),
        buffer: indices
            .into_iter()
            .map(|slot| slot.or(transparent).unwrap_or(0) as u8)
            .collect::<Vec<_>>()
            .into(),
        ..gif::Frame::default()
    })
}

fn renders_identically(original: &[u8], candidate: &[u8]) -> bool {
    let (Ok(mut original), Ok(mut candidate)) =
        (GifRenderer::new(original), GifRenderer::new(candidate))
    else {
        return false;
    };
    if original.decoder.repeat() != candidate.decoder.repeat() {
        return false;
    }
    loop {
        let expected = match original.next() {
            Ok(Some((canvas, delay))) => (canvas.to_vec(), delay),
            Ok(None) => return matches!(candidate.next(), Ok(None)),
            Err(_) => return false,
        };
        match candidate.next() {
            Ok(Some((canvas, delay))) if canvas == expected.0.as_slice() && delay == expected.1 => {
            }
            _ => return false,
        }
    }
}

// ---------------------------------------------------------------------------
// ICO
// ---------------------------------------------------------------------------

/// Every icon entry as a standalone PNG. ImageIO cannot decode some bitmap
/// entries even on their own, so bitmaps are decoded here.
pub fn split_ico(data: &[u8]) -> Option<Vec<Vec<u8>>> {
    if data.get(..4)? != [0, 0, 1, 0] {
        return None;
    }
    let count = usize::from(u16::from_le_bytes([*data.get(4)?, *data.get(5)?]));
    if count == 0 {
        return None;
    }
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let entry = data.get(6 + index * 16..6 + (index + 1) * 16)?;
        let length = u32::from_le_bytes(entry[8..12].try_into().ok()?) as usize;
        let offset = u32::from_le_bytes(entry[12..16].try_into().ok()?) as usize;
        let payload = data.get(offset..offset.checked_add(length)?)?;
        if payload.starts_with(PNG_SIGNATURE) {
            entries.push(payload.to_vec());
        } else {
            entries.push(encode_png(&decode_icon_bitmap(payload)?).ok()?);
        }
    }
    Some(entries)
}

/// Decode an icon's BITMAPINFOHEADER bitmap: 1, 4, 8, 24 or 32 bits per
/// pixel, bottom-up rows, followed by a 1-bit transparency mask.
fn decode_icon_bitmap(data: &[u8]) -> Option<RgbaFrame> {
    let u32_at = |offset: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            data.get(offset..offset + 4)?.try_into().ok()?,
        ))
    };
    let header = u32_at(0)? as usize;
    let width = i32::from_le_bytes(data.get(4..8)?.try_into().ok()?);
    let stored_height = i32::from_le_bytes(data.get(8..12)?.try_into().ok()?);
    let bits = u16::from_le_bytes(data.get(14..16)?.try_into().ok()?);
    let compression = u32_at(16)?;
    let colors_used = u32_at(32)? as usize;
    if header < 40 || compression != 0 || width <= 0 || width > 256 || stored_height <= 0 {
        return None;
    }
    let (width, height) = (width as usize, (stored_height as usize / 2).max(1));
    if height > 256 || !matches!(bits, 1 | 4 | 8 | 24 | 32) {
        return None;
    }
    let palette_size = if bits <= 8 {
        if colors_used == 0 {
            1 << bits
        } else {
            colors_used.min(1 << bits)
        }
    } else {
        0
    };
    let palette = data.get(header..header + palette_size * 4)?;
    let pixels_start = header + palette_size * 4;
    let stride = (width * usize::from(bits)).div_ceil(32) * 4;
    let mask_stride = width.div_ceil(32) * 4;
    let mask_start = pixels_start + stride * height;

    let mut rgba = vec![0u8; width * height * 4];
    let mut any_alpha = false;
    for row in 0..height {
        let source = data.get(pixels_start + row * stride..pixels_start + (row + 1) * stride)?;
        let target_row = height - 1 - row;
        for column in 0..width {
            let (red, green, blue, alpha) = match bits {
                32 => {
                    let pixel = &source[column * 4..column * 4 + 4];
                    (pixel[2], pixel[1], pixel[0], pixel[3])
                }
                24 => {
                    let pixel = &source[column * 3..column * 3 + 3];
                    (pixel[2], pixel[1], pixel[0], 255)
                }
                _ => {
                    let bit_offset = column * usize::from(bits);
                    let byte = source[bit_offset / 8];
                    let shift = 8 - usize::from(bits) - bit_offset % 8;
                    let index = usize::from((byte >> shift) & ((1u16 << bits) - 1) as u8);
                    let entry = palette.get(index * 4..index * 4 + 4)?;
                    (entry[2], entry[1], entry[0], 255)
                }
            };
            any_alpha |= bits == 32 && alpha != 0;
            let offset = (target_row * width + column) * 4;
            rgba[offset..offset + 4].copy_from_slice(&[red, green, blue, alpha]);
        }
    }
    // 32-bit entries carry real alpha; older ones, or 32-bit entries whose
    // alpha is all zero, use the 1-bit mask.
    if bits != 32 || !any_alpha {
        for row in 0..height {
            let mask =
                data.get(mask_start + row * mask_stride..mask_start + (row + 1) * mask_stride);
            let target_row = height - 1 - row;
            for column in 0..width {
                let transparent =
                    mask.is_some_and(|mask| mask[column / 8] & (0x80 >> (column % 8)) != 0);
                rgba[(target_row * width + column) * 4 + 3] = if transparent { 0 } else { 255 };
            }
        }
    }
    Some(RgbaFrame {
        pixels: rgba,
        width: width as u32,
        height: height as u32,
    })
}

pub fn encode_png(frame: &RgbaFrame) -> Result<Vec<u8>, ConversionError> {
    let mut data = Vec::new();
    let mut encoder = png::Encoder::new(&mut data, frame.width, frame.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(&frame.pixels))
        .map_err(|error| codec_error("A PNG could not be encoded", error))?;
    Ok(data)
}

/// An ICO container with one PNG-compressed entry per frame, the layout
/// Windows has read since Vista and ImageIO reads back.
pub fn encode_ico(frames: &[Vec<u8>]) -> Result<Vec<u8>, ConversionError> {
    const HEADER: usize = 6;
    const ENTRY: usize = 16;
    let count = u16::try_from(frames.len())
        .ok()
        .filter(|count| *count > 0)
        .ok_or_else(|| ConversionError::UnsupportedConversion {
            input: format!("This image has {} frames", frames.len()),
            output: "ICO".into(),
        })?;
    let mut offset = HEADER + ENTRY * frames.len();
    let mut icon = Vec::with_capacity(offset + frames.iter().map(Vec::len).sum::<usize>());
    icon.extend(0u16.to_le_bytes());
    icon.extend(1u16.to_le_bytes());
    icon.extend(count.to_le_bytes());
    for frame in frames {
        let (width, height) = png_dimensions(frame)
            .ok_or_else(|| codec_error("An icon frame could not be decoded", "not a PNG"))?;
        if width > 256 || height > 256 {
            return Err(ConversionError::UnsupportedConversion {
                input: format!("This frame is {width} × {height} px"),
                output: "ICO, which allows at most 256 × 256 px".into(),
            });
        }
        let size = u32::try_from(frame.len()).map_err(|_| ConversionError::OutputMissing)?;
        let start = u32::try_from(offset).map_err(|_| ConversionError::OutputMissing)?;
        // A stored 0 means 256 px.
        icon.extend([(width % 256) as u8, (height % 256) as u8, 0, 0]);
        icon.extend(1u16.to_le_bytes());
        icon.extend(32u16.to_le_bytes());
        icon.extend(size.to_le_bytes());
        icon.extend(start.to_le_bytes());
        offset += frame.len();
    }
    for frame in frames {
        icon.extend(frame);
    }
    Ok(icon)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, rgba: [u8; 4]) -> RgbaFrame {
        RgbaFrame {
            pixels: rgba.repeat((width * height) as usize),
            width,
            height,
        }
    }

    #[test]
    fn reads_helper_pixel_frames_and_rejects_truncation() {
        let directory = tempfile::tempdir().expect("tempdir");
        let path = directory.path().join("frames.pixels");
        let mut data = Vec::new();
        for (width, height, value) in [(2_u32, 1_u32, 7_u8), (1, 3, 9)] {
            data.extend_from_slice(PIXELS_MAGIC);
            data.extend_from_slice(&width.to_le_bytes());
            data.extend_from_slice(&height.to_le_bytes());
            data.extend(std::iter::repeat_n(value, (width * height * 4) as usize));
        }
        std::fs::write(&path, &data).expect("write frames");
        let frames = read_pixels(&path).expect("frames");
        assert_eq!(frames.len(), 2);
        assert_eq!((frames[0].width, frames[0].height), (2, 1));
        assert_eq!(frames[1].pixels, vec![9; 12]);

        std::fs::write(&path, &data[..data.len() - 1]).expect("write truncated");
        assert!(read_pixels(&path).is_err());
        std::fs::write(&path, b"").expect("write empty");
        assert!(read_pixels(&path).is_err());
    }

    #[test]
    fn encodes_still_and_animated_webp() {
        let still = encode_webp(&solid(8, 6, [1, 2, 3, 255]), Some(80), 4).expect("still");
        assert!(still.starts_with(b"RIFF") && &still[8..12] == b"WEBP");

        let frames = [solid(8, 6, [255, 0, 0, 255]), solid(8, 6, [0, 0, 255, 255])];
        let animated =
            encode_webp_animation(&frames, &[200, 500], 3, Some(80), 4).expect("animation");
        let chunks = riff_chunks(&animated).expect("riff");
        assert_eq!(&chunks[0].0, b"VP8X");
        let anim = chunks
            .iter()
            .find(|(fourcc, _)| fourcc == b"ANIM")
            .expect("ANIM");
        assert_eq!(u16::from_le_bytes([anim.1[4], anim.1[5]]), 3, "loop count");
        let durations = chunks
            .iter()
            .filter(|(fourcc, _)| fourcc == b"ANMF")
            .map(|(_, body)| u32::from_le_bytes([body[12], body[13], body[14], 0]))
            .collect::<Vec<_>>();
        assert_eq!(durations, vec![200, 500]);

        let mixed = [solid(8, 6, [0; 4]), solid(4, 6, [0; 4])];
        assert!(matches!(
            encode_webp_animation(&mixed, &[100, 100], 0, None, 4),
            Err(ConversionError::UnsupportedConversion { .. })
        ));
    }

    #[test]
    fn copies_webp_exif_and_xmp_but_not_colour_profiles() {
        let encoded = encode_webp(&solid(8, 6, [9, 9, 9, 255]), Some(80), 4).expect("encode");
        let mut source = b"RIFF\0\0\0\0WEBP".to_vec();
        for (fourcc, body) in [
            (b"ICCP", &b"profile"[..]),
            (b"EXIF", b"exif!"),
            (b"XMP ", b"<x/>"),
        ] {
            source.extend(fourcc);
            source.extend((body.len() as u32).to_le_bytes());
            source.extend(body);
            if body.len() & 1 == 1 {
                source.push(0);
            }
        }
        let tagged = copy_webp_metadata(&encoded, &source, 8, 6);
        let chunks = riff_chunks(&tagged).expect("riff");
        let names = chunks.iter().map(|(fourcc, _)| *fourcc).collect::<Vec<_>>();
        assert_eq!(names, vec![*b"VP8X", *b"VP8 ", *b"EXIF", *b"XMP "]);
        assert_eq!(chunks[0].1[0], 0x0c, "EXIF and XMP flags");
        assert_eq!(
            u32::from_le_bytes(tagged[4..8].try_into().unwrap()) as usize,
            tagged.len() - 8
        );
        assert_eq!(copy_webp_metadata(&encoded, b"not webp", 8, 6), encoded);
    }

    #[test]
    fn keeps_jpeg_exif_and_rewrites_orientation() {
        let mut exif = orientation_exif(6);
        set_exif_orientation(&mut exif, 1);
        assert_eq!(exif, orientation_exif(1));

        let segments = [AppSegment {
            marker: 1,
            data: orientation_exif(8),
        }];
        let jpeg = encode_jpeg(&solid(16, 16, [10, 20, 30, 255]), 85, &segments).expect("jpeg");
        let kept = jpeg_metadata_segments(&jpeg);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].marker, 1);
        assert_eq!(kept[0].data, orientation_exif(8));
    }

    #[test]
    fn losslessly_recompresses_jpeg_and_reports_corrupt_input() {
        // Noise keeps the Huffman tables doing real work.
        let mut state = 0x2545_f491_u32;
        let pixels = (0..64 * 48 * 4)
            .map(|index| {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                if index % 4 == 3 {
                    255
                } else {
                    (state & 0xff) as u8
                }
            })
            .collect();
        let frame = RgbaFrame {
            pixels,
            width: 64,
            height: 48,
        };
        let segments = [AppSegment {
            marker: 1,
            data: orientation_exif(6),
        }];
        let source = encode_jpeg(&frame, 92, &segments).expect("source jpeg");

        let kept = optimize_jpeg_lossless(&source, true).expect("lossless");
        assert!(kept.starts_with(&[0xff, 0xd8]));
        assert_eq!(jpeg_metadata_segments(&kept).len(), 1, "EXIF kept");
        let stripped = optimize_jpeg_lossless(&source, false).expect("lossless");
        let kept_segments = jpeg_metadata_segments(&stripped);
        assert_eq!(
            kept_segments.len(),
            1,
            "only orientation survives stripping"
        );
        assert_eq!(kept_segments[0].data, orientation_exif(6));
        assert_eq!(jpeg_orientation(&stripped), Some(6));

        assert!(optimize_jpeg_lossless(b"\xff\xd8 not a jpeg", false).is_err());
    }

    fn animated_gif(frames: &[(u16, u16, u16, u16, u8)]) -> Vec<u8> {
        // A 32x24 canvas whose frames repaint full-canvas copies with one
        // block moved, the shape screen recordings have.
        let palette = [
            0x25, 0x33, 0x4f, 0x90, 0xa8, 0xe8, 0xff, 0x00, 0x00, 0xff, 0xff, 0xff,
        ];
        let mut data = Vec::new();
        {
            let mut encoder = gif::Encoder::new(&mut data, 32, 24, &palette).expect("encoder");
            encoder.set_repeat(gif::Repeat::Finite(2)).expect("repeat");
            for &(x, y, width, height, color) in frames {
                let mut buffer = vec![0u8; 32 * 24];
                for row in y..y + height {
                    for column in x..x + width {
                        buffer[usize::from(row) * 32 + usize::from(column)] = color;
                    }
                }
                let mut frame = gif::Frame::from_indexed_pixels(32, 24, buffer, None);
                frame.delay = 7;
                encoder.write_frame(&frame).expect("frame");
            }
        }
        data
    }

    #[test]
    fn optimizes_gifs_losslessly() {
        let frames = (0..12)
            .map(|step| (step * 2, 4, 6, 6, 1 + (step % 2) as u8))
            .collect::<Vec<_>>();
        let source = animated_gif(&frames);
        let optimized = optimize_gif(&source).expect("smaller lossless GIF");
        assert!(optimized.len() < source.len());
        assert!(renders_identically(&source, &optimized));

        // A changed pixel is caught by the comparison.
        let mut moved = frames.clone();
        moved[5].0 += 1;
        assert!(!renders_identically(&source, &animated_gif(&moved)));
        assert!(optimize_gif(b"GIF89a broken").is_none());
    }

    #[test]
    fn splits_and_rebuilds_icons() {
        let png = |width: u32, height: u32| {
            let mut data = PNG_SIGNATURE.to_vec();
            data.extend(b"\0\0\0\rIHDR");
            data.extend(width.to_be_bytes());
            data.extend(height.to_be_bytes());
            data
        };
        let icon = encode_ico(&[png(20, 15), png(20, 10)]).expect("icon");
        let entries = split_ico(&icon).expect("entries");
        assert_eq!(entries.len(), 2);
        assert_eq!(png_dimensions(&entries[1]), Some((20, 10)));
        assert!(encode_ico(&[png(300, 10)]).is_err());
    }

    #[test]
    fn decodes_bitmap_icon_entries_with_alpha_and_masks() {
        // A 3x2 32-bit entry, bottom-up, followed by an unused mask.
        let mut bitmap = 40u32.to_le_bytes().to_vec();
        bitmap.extend(3i32.to_le_bytes());
        bitmap.extend(4i32.to_le_bytes()); // doubled height
        bitmap.extend(1u16.to_le_bytes());
        bitmap.extend(32u16.to_le_bytes());
        bitmap.extend([0; 24]);
        let bottom = [[0, 0, 255, 255], [0, 255, 0, 128], [255, 0, 0, 0]];
        let top = [[10, 20, 30, 255]; 3];
        for row in [bottom, top] {
            for pixel in row {
                bitmap.extend(pixel);
            }
        }
        bitmap.extend([0; 8]);
        let frame = decode_icon_bitmap(&bitmap).expect("32-bit entry");
        assert_eq!((frame.width, frame.height), (3, 2));
        assert_eq!(&frame.pixels[..4], &[30, 20, 10, 255], "top row first");
        assert_eq!(&frame.pixels[12..16], &[255, 0, 0, 255], "bottom-left red");
        assert_eq!(frame.pixels[19], 128, "real alpha is kept");

        // A 2x1 1-bit entry with a two-colour palette and one masked pixel.
        let mut mono = 40u32.to_le_bytes().to_vec();
        mono.extend(2i32.to_le_bytes());
        mono.extend(2i32.to_le_bytes());
        mono.extend(1u16.to_le_bytes());
        mono.extend(1u16.to_le_bytes());
        mono.extend([0; 24]);
        mono.extend([0, 0, 0, 0, 255, 255, 255, 0]); // black, white
        mono.extend([0b0100_0000, 0, 0, 0]); // pixels: black, white
        mono.extend([0b1000_0000, 0, 0, 0]); // mask: first pixel transparent
        let frame = decode_icon_bitmap(&mono).expect("1-bit entry");
        assert_eq!(frame.pixels, vec![0, 0, 0, 0, 255, 255, 255, 255]);
    }
}
