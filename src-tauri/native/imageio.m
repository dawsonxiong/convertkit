#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <math.h>

// Bundled ImageIO bridge for raster image inspection, conversion and resizing.
// It replaces the ImageMagick subset ConvertKit relies on so a fresh install
// needs no Homebrew packages for image work. WebP output is encoded on the
// Rust side because ImageIO can decode WebP but not write it.

static const size_t kMaxFrames = 10000;
static const size_t kMaxDimension = 32768;

static int fail(NSString *message, int code) {
  const char *bytes = message.UTF8String;
  if (bytes != NULL) {
    fwrite(bytes, 1, strlen(bytes), stderr);
    fwrite("\n", 1, 1, stderr);
  }
  return code;
}

static NSString *pathArgument(const char *value) {
  return [[NSFileManager defaultManager] stringWithFileSystemRepresentation:value
                                                                     length:strlen(value)];
}

// Names match `magick identify %m` so the Rust verification contract is
// unchanged whichever inspector produced the summary.
static NSString *formatName(CFStringRef type) {
  NSDictionary<NSString *, NSString *> *names = @{
    @"public.jpeg" : @"JPEG",
    @"public.png" : @"PNG",
    @"org.webmproject.webp" : @"WEBP",
    @"public.tiff" : @"TIFF",
    @"com.microsoft.bmp" : @"BMP",
    @"com.compuserve.gif" : @"GIF",
    @"com.microsoft.ico" : @"ICO",
    @"public.avif" : @"AVIF",
    @"public.avis" : @"AVIF",
    @"public.heic" : @"HEIC",
    @"public.heics" : @"HEIC",
    @"public.heif" : @"HEIF",
  };
  NSString *name = names[(__bridge NSString *)type];
  return name ?: @"UNSUPPORTED";
}

static CGImageSourceRef openSource(NSString *path, int *status) {
  BOOL isDirectory = NO;
  if (path == nil ||
      ![[NSFileManager defaultManager] fileExistsAtPath:path isDirectory:&isDirectory] ||
      isDirectory) {
    *status = fail(@"The source image is unavailable.", 3);
    return NULL;
  }
  NSURL *url = [NSURL fileURLWithPath:path isDirectory:NO];
  NSDictionary *options = @{(__bridge NSString *)kCGImageSourceShouldCache : @NO};
  CGImageSourceRef source =
      CGImageSourceCreateWithURL((__bridge CFURLRef)url, (__bridge CFDictionaryRef)options);
  if (source == NULL || CGImageSourceGetType(source) == NULL ||
      CGImageSourceGetStatus(source) != kCGImageStatusComplete) {
    if (source != NULL) {
      CFRelease(source);
    }
    *status = fail(@"This image cannot be decoded by macOS.", 5);
    return NULL;
  }
  size_t count = CGImageSourceGetCount(source);
  if (count == 0 || count > kMaxFrames) {
    CFRelease(source);
    *status = fail(@"The image frame count could not be verified.", 6);
    return NULL;
  }
  return source;
}

static BOOL frameSize(CGImageSourceRef source, size_t index, size_t *width, size_t *height) {
  NSDictionary *properties =
      CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, index, NULL));
  NSNumber *pixelWidth = properties[(__bridge NSString *)kCGImagePropertyPixelWidth];
  NSNumber *pixelHeight = properties[(__bridge NSString *)kCGImagePropertyPixelHeight];
  *width = pixelWidth.unsignedIntegerValue;
  *height = pixelHeight.unsignedIntegerValue;
  return *width > 0 && *height > 0;
}

static int runTypes(void) {
  NSArray *readable = CFBridgingRelease(CGImageSourceCopyTypeIdentifiers());
  NSArray *writable = CFBridgingRelease(CGImageDestinationCopyTypeIdentifiers());
  NSMutableString *summary = [NSMutableString string];
  for (NSString *type in readable) {
    [summary appendFormat:@"read\t%@\n", type];
  }
  for (NSString *type in writable) {
    [summary appendFormat:@"write\t%@\n", type];
  }
  fputs(summary.UTF8String, stdout);
  return 0;
}

static int runIdentify(NSString *path) {
  int status = 0;
  CGImageSourceRef source = openSource(path, &status);
  if (source == NULL) {
    return status;
  }
  NSString *name = formatName(CGImageSourceGetType(source));
  size_t count = CGImageSourceGetCount(source);
  NSMutableString *summary = [NSMutableString string];
  for (size_t index = 0; index < count; index++) {
    size_t width = 0;
    size_t height = 0;
    if (!frameSize(source, index, &width, &height)) {
      CFRelease(source);
      return fail(@"The image structure could not be verified.", 7);
    }
    [summary appendFormat:@"%@|%zu|%zu|%zu\n", name, width, height, count];
  }
  CFRelease(source);
  fputs(summary.UTF8String, stdout);
  return 0;
}

// EXIF orientation (1-8) of the first frame, so in-process encoders can keep
// a photo upright after stripping its metadata.
static int runOrientation(NSString *path) {
  int status = 0;
  CGImageSourceRef source = openSource(path, &status);
  if (source == NULL) {
    return status;
  }
  NSDictionary *properties =
      CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, 0, NULL));
  CFRelease(source);
  NSNumber *orientation = properties[(__bridge NSString *)kCGImagePropertyOrientation];
  int value = orientation.intValue;
  printf("%d\n", value >= 1 && value <= 8 ? value : 1);
  return 0;
}

static NSNumber *frameDelay(NSDictionary *properties);

// Per-frame display time in milliseconds and the loop count, for encoders
// that rebuild an animation in process.
static int runTiming(NSString *path) {
  int status = 0;
  CGImageSourceRef source = openSource(path, &status);
  if (source == NULL) {
    return status;
  }
  NSDictionary *container = CFBridgingRelease(CGImageSourceCopyProperties(source, NULL));
  NSArray<NSString *> *loopKeys = @[
    (__bridge NSString *)kCGImagePropertyGIFDictionary,
    (__bridge NSString *)kCGImagePropertyPNGDictionary,
    (__bridge NSString *)kCGImagePropertyWebPDictionary,
    (__bridge NSString *)kCGImagePropertyHEICSDictionary,
  ];
  NSArray<NSString *> *loopNames = @[
    (__bridge NSString *)kCGImagePropertyGIFLoopCount,
    (__bridge NSString *)kCGImagePropertyAPNGLoopCount,
    (__bridge NSString *)kCGImagePropertyWebPLoopCount,
    (__bridge NSString *)kCGImagePropertyHEICSLoopCount,
  ];
  long loops = 0;
  for (NSUInteger index = 0; index < loopKeys.count; index++) {
    NSNumber *value = container[loopKeys[index]][loopNames[index]];
    if ([value isKindOfClass:[NSNumber class]]) {
      loops = value.longValue;
      break;
    }
  }
  NSMutableString *summary = [NSMutableString stringWithFormat:@"loop %ld\n", loops];
  size_t count = CGImageSourceGetCount(source);
  for (size_t index = 0; index < count; index++) {
    NSDictionary *frame =
        CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, index, NULL));
    NSNumber *delay = frameDelay(frame);
    long milliseconds = delay == nil ? 100 : lround(delay.doubleValue * 1000.0);
    // Browsers show GIF frames of 10 ms or less for 100 ms; keep that look.
    [summary appendFormat:@"%ld\n", milliseconds <= 10 ? 100L : milliseconds];
  }
  CFRelease(source);
  fputs(summary.UTF8String, stdout);
  return 0;
}

// Combine the first frame of each input into one multi-image file. ICO
// sources ImageIO misreads are split into single-entry files and rejoined
// as a multi-page TIFF that every later step can decode.
static int runAssemble(NSString *outputPath, NSString *outputType, int argc,
                       const char *argv[], int start) {
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath isDirectory:NO];
  [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
  size_t count = (size_t)(argc - start);
  if (count == 0 || count > kMaxFrames) {
    return fail(@"Choose at least one frame to assemble.", 2);
  }
  CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
      (__bridge CFURLRef)outputURL, (__bridge CFStringRef)outputType, count, NULL);
  if (destination == NULL) {
    return fail(@"The output image could not be created.", 10);
  }
  for (int index = start; index < argc; index++) {
    int status = 0;
    CGImageSourceRef source = openSource(pathArgument(argv[index]), &status);
    if (source == NULL) {
      CFRelease(destination);
      [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
      return status;
    }
    CGImageDestinationAddImageFromSource(destination, source, 0, NULL);
    CFRelease(source);
  }
  BOOL written = CGImageDestinationFinalize(destination);
  CFRelease(destination);
  if (!written) {
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
    return fail(@"macOS could not encode the assembled image.", 13);
  }
  return 0;
}

typedef enum { ResizeNone, ResizeFit, ResizeExact, ResizeShrink } ResizeMode;

typedef struct {
  double quality;
  BOOL hasQuality;
  BOOL strip;
  BOOL orient;
  BOOL srgb;
  BOOL flatten;
  BOOL lzw;
  BOOL single;
  BOOL rgba8;
  BOOL hasFrame;
  size_t frame;
  ResizeMode resize;
  size_t resizeWidth;
  size_t resizeHeight;
} ConvertOptions;

static size_t roundedRatio(size_t value, size_t multiplier, size_t divisor) {
  unsigned long long scaled = (unsigned long long)value * multiplier;
  unsigned long long rounded = (scaled + divisor / 2) / divisor;
  return rounded < 1 ? 1 : (size_t)rounded;
}

// Same arithmetic as ImageMagick's `WxH`, `WxH!` and `WxH>` geometry, which
// the Rust side mirrors when it verifies the written frames.
static void targetSize(const ConvertOptions *options, size_t width, size_t height,
                       size_t *targetWidth, size_t *targetHeight) {
  *targetWidth = width;
  *targetHeight = height;
  switch (options->resize) {
  case ResizeNone:
    return;
  case ResizeExact:
    *targetWidth = options->resizeWidth;
    *targetHeight = options->resizeHeight;
    return;
  case ResizeShrink:
    if (width <= options->resizeWidth && height <= options->resizeHeight) {
      return;
    }
    // fall through
  case ResizeFit:
    if ((unsigned long long)options->resizeWidth * height <=
        (unsigned long long)options->resizeHeight * width) {
      *targetWidth = options->resizeWidth;
      *targetHeight = roundedRatio(height, options->resizeWidth, width);
    } else {
      *targetWidth = roundedRatio(width, options->resizeHeight, height);
      *targetHeight = options->resizeHeight;
    }
    return;
  }
}

static BOOL needsPixels(const ConvertOptions *options, BOOL forceFlatten) {
  return options->strip || options->orient || options->srgb || options->flatten ||
      options->rgba8 || forceFlatten || options->resize != ResizeNone;
}

static CGImageRef decodeFrame(CGImageSourceRef source, size_t index, BOOL orient) {
  if (!orient) {
    return CGImageSourceCreateImageAtIndex(source, index, NULL);
  }
  size_t width = 0;
  size_t height = 0;
  if (!frameSize(source, index, &width, &height)) {
    return NULL;
  }
  // The thumbnail API is the documented way to apply EXIF orientation while
  // decoding. A max size of the longest edge keeps the full resolution.
  NSDictionary *options = @{
    (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways : @YES,
    (__bridge NSString *)kCGImageSourceCreateThumbnailWithTransform : @YES,
    (__bridge NSString *)kCGImageSourceShouldCacheImmediately : @YES,
    (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize : @(MAX(width, height)),
  };
  return CGImageSourceCreateThumbnailAtIndex(source, index, (__bridge CFDictionaryRef)options);
}

static CGImageRef redraw(CGImageRef image, const ConvertOptions *options, BOOL flatten) {
  size_t width = 0;
  size_t height = 0;
  targetSize(options, CGImageGetWidth(image), CGImageGetHeight(image), &width, &height);
  if (width == 0 || height == 0 || width > kMaxDimension || height > kMaxDimension) {
    return NULL;
  }

  CGColorSpaceRef sourceSpace = CGImageGetColorSpace(image);
  BOOL keepSpace = !options->srgb && !options->rgba8 && sourceSpace != NULL &&
      CGColorSpaceGetModel(sourceSpace) == kCGColorSpaceModelRGB;
  CGColorSpaceRef space = keepSpace ? CGColorSpaceRetain(sourceSpace)
                                    : CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  BOOL deep = !options->rgba8 && CGImageGetBitsPerComponent(image) > 8;
  size_t bitsPerComponent = deep ? 16 : 8;
  CGBitmapInfo info = (CGBitmapInfo)(flatten ? kCGImageAlphaNoneSkipLast
                                              : kCGImageAlphaPremultipliedLast);
  if (deep) {
    info |= kCGBitmapByteOrder16Host;
  }
  CGContextRef context =
      CGBitmapContextCreate(NULL, width, height, bitsPerComponent, 0, space, info);
  CGColorSpaceRelease(space);
  if (context == NULL) {
    return NULL;
  }
  CGRect bounds = CGRectMake(0, 0, width, height);
  if (flatten) {
    CGContextSetRGBFillColor(context, 1, 1, 1, 1);
    CGContextFillRect(context, bounds);
  }
  CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
  CGContextDrawImage(context, bounds, image);
  CGImageRef result = CGBitmapContextCreateImage(context);
  CGContextRelease(context);
  return result;
}

static BOOL hasAlpha(CGImageRef image) {
  CGImageAlphaInfo alpha = CGImageGetAlphaInfo(image);
  return alpha != kCGImageAlphaNone && alpha != kCGImageAlphaNoneSkipLast &&
      alpha != kCGImageAlphaNoneSkipFirst;
}

static NSNumber *frameDelay(NSDictionary *properties) {
  NSArray<NSArray<NSString *> *> *keys = @[
    @[ (__bridge NSString *)kCGImagePropertyGIFDictionary,
       (__bridge NSString *)kCGImagePropertyGIFUnclampedDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyGIFDictionary,
       (__bridge NSString *)kCGImagePropertyGIFDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyPNGDictionary,
       (__bridge NSString *)kCGImagePropertyAPNGUnclampedDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyPNGDictionary,
       (__bridge NSString *)kCGImagePropertyAPNGDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyWebPDictionary,
       (__bridge NSString *)kCGImagePropertyWebPUnclampedDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyWebPDictionary,
       (__bridge NSString *)kCGImagePropertyWebPDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyHEICSDictionary,
       (__bridge NSString *)kCGImagePropertyHEICSUnclampedDelayTime ],
    @[ (__bridge NSString *)kCGImagePropertyHEICSDictionary,
       (__bridge NSString *)kCGImagePropertyHEICSDelayTime ],
  ];
  // The unclamped value comes first so a stored delay of 0 survives; the
  // clamped key reports 0.1 s for it.
  for (NSArray<NSString *> *pair in keys) {
    NSNumber *delay = properties[pair[0]][pair[1]];
    if ([delay isKindOfClass:[NSNumber class]] && delay.doubleValue >= 0) {
      return delay;
    }
  }
  return nil;
}

static NSDictionary *timingProperties(NSString *outputType, NSNumber *delay) {
  if (delay == nil) {
    return @{};
  }
  if ([outputType isEqualToString:@"com.compuserve.gif"]) {
    return @{
      (__bridge NSString *)kCGImagePropertyGIFDictionary :
          @{(__bridge NSString *)kCGImagePropertyGIFDelayTime : delay}
    };
  }
  if ([outputType isEqualToString:@"public.heic"] ||
      [outputType isEqualToString:@"public.heics"]) {
    return @{
      (__bridge NSString *)kCGImagePropertyHEICSDictionary :
          @{(__bridge NSString *)kCGImagePropertyHEICSDelayTime : delay}
    };
  }
  return @{};
}

static BOOL parseUnsigned(const char *value, size_t *result) {
  char *end = NULL;
  unsigned long long parsed = strtoull(value, &end, 10);
  if (end == value || *end != '\0' || parsed == 0 || parsed > kMaxDimension) {
    return NO;
  }
  *result = (size_t)parsed;
  return YES;
}

static int parseOptions(int argc, const char *argv[], int start, ConvertOptions *options) {
  for (int index = start; index < argc; index++) {
    const char *flag = argv[index];
    if (strcmp(flag, "--quality") == 0 && index + 1 < argc) {
      char *end = NULL;
      double quality = strtod(argv[++index], &end);
      if (*end != '\0' || quality < 1 || quality > 100) {
        return fail(@"Quality must be between 1 and 100.", 2);
      }
      options->quality = quality / 100.0;
      options->hasQuality = YES;
    } else if ((strcmp(flag, "--fit") == 0 || strcmp(flag, "--exact") == 0 ||
                strcmp(flag, "--shrink") == 0) &&
               index + 2 < argc) {
      options->resize = strcmp(flag, "--fit") == 0     ? ResizeFit
                        : strcmp(flag, "--exact") == 0 ? ResizeExact
                                                       : ResizeShrink;
      if (!parseUnsigned(argv[index + 1], &options->resizeWidth) ||
          !parseUnsigned(argv[index + 2], &options->resizeHeight)) {
        return fail(@"Resize dimensions must be between 1 and 32768 pixels.", 2);
      }
      index += 2;
    } else if (strcmp(flag, "--frame") == 0 && index + 1 < argc) {
      char *end = NULL;
      const char *value = argv[++index];
      unsigned long long frame = strtoull(value, &end, 10);
      if (end == value || *end != '\0' || frame >= kMaxFrames) {
        return fail(@"Frame index is out of range.", 2);
      }
      options->frame = (size_t)frame;
      options->hasFrame = YES;
    } else if (strcmp(flag, "--strip") == 0) {
      options->strip = YES;
    } else if (strcmp(flag, "--orient") == 0) {
      options->orient = YES;
    } else if (strcmp(flag, "--srgb") == 0) {
      options->srgb = YES;
    } else if (strcmp(flag, "--flatten") == 0) {
      options->flatten = YES;
    } else if (strcmp(flag, "--lzw") == 0) {
      options->lzw = YES;
    } else if (strcmp(flag, "--single") == 0) {
      options->single = YES;
    } else if (strcmp(flag, "--rgba8") == 0) {
      options->rgba8 = YES;
    } else {
      return fail([NSString stringWithFormat:@"Unknown option: %s", flag], 2);
    }
  }
  return 0;
}

static int runConvert(NSString *inputPath, NSString *outputPath, NSString *outputType,
                      const ConvertOptions *options) {
  if (outputPath == nil || [inputPath isEqualToString:outputPath]) {
    return fail(@"Choose a different output path.", 4);
  }
  NSArray *writable = CFBridgingRelease(CGImageDestinationCopyTypeIdentifiers());
  if (![writable containsObject:outputType]) {
    return fail([NSString stringWithFormat:@"macOS cannot write %@ images.", outputType], 8);
  }

  int status = 0;
  CGImageSourceRef source = openSource(inputPath, &status);
  if (source == NULL) {
    return status;
  }
  size_t sourceCount = CGImageSourceGetCount(source);
  if (options->single && sourceCount != 1) {
    CFRelease(source);
    return fail(@"Animated and multi-page images are not supported for this output.", 9);
  }
  if (options->hasFrame && options->frame >= sourceCount) {
    CFRelease(source);
    return fail(@"Frame index is out of range.", 9);
  }
  size_t first = options->hasFrame ? options->frame : 0;
  size_t count = options->hasFrame ? 1 : sourceCount;

  BOOL opaqueOutput = [outputType isEqualToString:@"public.jpeg"];
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath isDirectory:NO];
  [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
  CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
      (__bridge CFURLRef)outputURL, (__bridge CFStringRef)outputType, count, NULL);
  if (destination == NULL) {
    CFRelease(source);
    return fail(@"The output image could not be created.", 10);
  }

  NSDictionary *container =
      CFBridgingRelease(CGImageSourceCopyProperties(source, NULL));
  NSNumber *loopCount =
      container[(__bridge NSString *)kCGImagePropertyGIFDictionary]
               [(__bridge NSString *)kCGImagePropertyGIFLoopCount];
  if (count > 1 && [outputType isEqualToString:@"com.compuserve.gif"]) {
    NSDictionary *gif = @{
      (__bridge NSString *)kCGImagePropertyGIFDictionary :
          @{(__bridge NSString *)kCGImagePropertyGIFLoopCount : loopCount ?: @0}
    };
    CGImageDestinationSetProperties(destination, (__bridge CFDictionaryRef)gif);
  }

  for (size_t index = first; index < first + count; index++) {
    NSDictionary *frame =
        CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, index, NULL));
    NSMutableDictionary *properties = [NSMutableDictionary dictionary];
    if (!options->strip) {
      [properties addEntriesFromDictionary:frame];
    }
    [properties addEntriesFromDictionary:timingProperties(outputType, frameDelay(frame))];
    if (options->hasQuality) {
      properties[(__bridge NSString *)kCGImageDestinationLossyCompressionQuality] =
          @(options->quality);
    }
    if (options->lzw) {
      properties[(__bridge NSString *)kCGImagePropertyTIFFDictionary] =
          @{(__bridge NSString *)kCGImagePropertyTIFFCompression : @5};
    }
    NSNumber *orientation = frame[(__bridge NSString *)kCGImagePropertyOrientation];
    if (options->orient) {
      properties[(__bridge NSString *)kCGImagePropertyOrientation] = @1;
    } else if (orientation != nil) {
      // Stripping metadata must not rotate the picture the user sees.
      properties[(__bridge NSString *)kCGImagePropertyOrientation] = orientation;
    }

    CGImageRef decoded = NULL;
    BOOL flatten = options->flatten;
    if (opaqueOutput && !flatten) {
      decoded = decodeFrame(source, index, options->orient);
      flatten = decoded != NULL && hasAlpha(decoded);
    }
    if (!needsPixels(options, flatten)) {
      if (decoded != NULL) {
        CGImageRelease(decoded);
      }
      CGImageDestinationAddImageFromSource(destination, source, index,
                                           (__bridge CFDictionaryRef)properties);
      continue;
    }
    if (decoded == NULL) {
      decoded = decodeFrame(source, index, options->orient);
    }
    if (decoded == NULL) {
      CFRelease(destination);
      CFRelease(source);
      [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
      return fail(@"This image cannot be rendered by macOS.", 11);
    }
    CGImageRef pixels = decoded;
    BOOL resized = options->resize != ResizeNone;
    if (resized || flatten || options->srgb || options->rgba8) {
      pixels = redraw(decoded, options, flatten);
      CGImageRelease(decoded);
      if (pixels == NULL) {
        CFRelease(destination);
        CFRelease(source);
        [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
        return fail(@"The resized image could not be rendered.", 12);
      }
    }
    // Pixel dimensions describe the new raster, never the source.
    [properties removeObjectForKey:(__bridge NSString *)kCGImagePropertyPixelWidth];
    [properties removeObjectForKey:(__bridge NSString *)kCGImagePropertyPixelHeight];
    CGImageDestinationAddImage(destination, pixels, (__bridge CFDictionaryRef)properties);
    CGImageRelease(pixels);
  }

  BOOL written = CGImageDestinationFinalize(destination);
  CFRelease(destination);
  CFRelease(source);
  if (!written) {
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
    return fail(@"macOS could not encode the output image.", 13);
  }
  return 0;
}

static int runThumbnail(NSString *inputPath, NSString *outputPath, const char *edge) {
  size_t maxEdge = 0;
  if (!parseUnsigned(edge, &maxEdge)) {
    return fail(@"Thumbnail size must be between 1 and 32768 pixels.", 2);
  }
  int status = 0;
  CGImageSourceRef source = openSource(inputPath, &status);
  if (source == NULL) {
    return status;
  }
  NSDictionary *options = @{
    (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways : @YES,
    (__bridge NSString *)kCGImageSourceCreateThumbnailWithTransform : @YES,
    (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize : @(maxEdge),
  };
  CGImageRef image =
      CGImageSourceCreateThumbnailAtIndex(source, 0, (__bridge CFDictionaryRef)options);
  CFRelease(source);
  if (image == NULL) {
    return fail(@"This image cannot be rendered by macOS.", 11);
  }
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath isDirectory:NO];
  CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
      (__bridge CFURLRef)outputURL, CFSTR("public.png"), 1, NULL);
  if (destination == NULL) {
    CGImageRelease(image);
    return fail(@"The thumbnail could not be created.", 10);
  }
  CGImageDestinationAddImage(destination, image, NULL);
  BOOL written = CGImageDestinationFinalize(destination);
  CFRelease(destination);
  CGImageRelease(image);
  if (!written) {
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
    return fail(@"macOS could not encode the thumbnail.", 13);
  }
  return 0;
}

static int usage(void) {
  return fail(@"Usage:\n"
              @"  convertkit-imageio types\n"
              @"  convertkit-imageio identify <image>\n"
              @"  convertkit-imageio orientation <image>\n"
              @"  convertkit-imageio timing <image>\n"
              @"  convertkit-imageio assemble <output> <type-identifier> <image>...\n"
              @"  convertkit-imageio thumbnail <image> <output.png> <max-edge>\n"
              @"  convertkit-imageio convert <image> <output> <type-identifier> [options]\n"
              @"    --quality N  --fit W H  --exact W H  --shrink W H  --strip  --orient\n"
              @"    --srgb  --flatten  --lzw  --single  --rgba8  --frame N",
              2);
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc < 2) {
      return usage();
    }
    const char *command = argv[1];
    if (strcmp(command, "types") == 0 && argc == 2) {
      return runTypes();
    }
    if (strcmp(command, "identify") == 0 && argc == 3) {
      return runIdentify(pathArgument(argv[2]));
    }
    if (strcmp(command, "timing") == 0 && argc == 3) {
      return runTiming(pathArgument(argv[2]));
    }
    if (strcmp(command, "assemble") == 0 && argc >= 5) {
      return runAssemble(pathArgument(argv[2]), [NSString stringWithUTF8String:argv[3]],
                         argc, argv, 4);
    }
    if (strcmp(command, "orientation") == 0 && argc == 3) {
      return runOrientation(pathArgument(argv[2]));
    }
    if (strcmp(command, "thumbnail") == 0 && argc == 5) {
      return runThumbnail(pathArgument(argv[2]), pathArgument(argv[3]), argv[4]);
    }
    if (strcmp(command, "convert") == 0 && argc >= 5) {
      ConvertOptions options = {0};
      int status = parseOptions(argc, argv, 5, &options);
      if (status != 0) {
        return status;
      }
      NSString *outputType = [NSString stringWithUTF8String:argv[4]];
      return runConvert(pathArgument(argv[2]), pathArgument(argv[3]), outputType, &options);
    }
    return usage();
  }
}
