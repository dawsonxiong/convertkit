#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <PDFKit/PDFKit.h>
#import <math.h>

// Bundled PDFKit/Quartz bridge that answers the Poppler commands ConvertKit
// uses, with the same arguments and output format:
//
//   convertkit-pdf pdfinfo [-f N -l M] [-box] <pdf>
//   convertkit-pdf pdftoppm -f N -l N -singlefile -cropbox -r DPI (-png|-jpeg)
//                           [-jpegopt quality=Q] <pdf> <output-prefix>
//   convertkit-pdf pdftotext [-enc UTF-8] [-eol unix] [-nopgbrk] <pdf> <txt>
//
// Keeping Poppler's interface lets the Rust parsers and verification stay
// unchanged, and Poppler itself becomes optional.

static const CGFloat kMaxRenderEdge = 32768;

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

// Opens a PDF the way Poppler does without a password: documents with only
// an owner password open, documents that need a user password do not.
static CGPDFDocumentRef openDocument(NSString *path, int *status) {
  BOOL isDirectory = NO;
  if (path == nil ||
      ![[NSFileManager defaultManager] fileExistsAtPath:path isDirectory:&isDirectory] ||
      isDirectory) {
    *status = fail(@"I/O Error: Couldn't open file.", 1);
    return NULL;
  }
  NSURL *url = [NSURL fileURLWithPath:path isDirectory:NO];
  CGPDFDocumentRef document = CGPDFDocumentCreateWithURL((__bridge CFURLRef)url);
  if (document == NULL) {
    *status = fail(@"Syntax Error: Couldn't read the PDF.", 1);
    return NULL;
  }
  if (CGPDFDocumentIsEncrypted(document) && !CGPDFDocumentIsUnlocked(document) &&
      !CGPDFDocumentUnlockWithPassword(document, "")) {
    CGPDFDocumentRelease(document);
    *status = fail(@"Command Line Error: Incorrect password", 1);
    return NULL;
  }
  if (CGPDFDocumentGetNumberOfPages(document) == 0) {
    CGPDFDocumentRelease(document);
    *status = fail(@"Syntax Error: The PDF has no pages.", 1);
    return NULL;
  }
  return document;
}

static BOOL parsePage(const char *value, size_t *page) {
  char *end = NULL;
  long parsed = strtol(value, &end, 10);
  if (end == value || *end != '\0' || parsed < 1) {
    return NO;
  }
  *page = (size_t)parsed;
  return YES;
}

static NSString *paperName(CGFloat width, CGFloat height) {
  CGFloat shortEdge = MIN(width, height);
  CGFloat longEdge = MAX(width, height);
  struct { CGFloat a, b; const char *name; } papers[] = {
    {612, 792, "letter"}, {612, 1008, "legal"}, {595.276, 841.89, "A4"},
    {419.528, 595.276, "A5"}, {841.89, 1190.55, "A3"},
  };
  for (size_t index = 0; index < sizeof papers / sizeof papers[0]; index++) {
    if (fabs(shortEdge - papers[index].a) < 1 && fabs(longEdge - papers[index].b) < 1) {
      return [NSString stringWithUTF8String:papers[index].name];
    }
  }
  return nil;
}

static NSString *number(CGFloat value) {
  return [NSString stringWithFormat:@"%g", value];
}

static void appendBox(NSMutableString *output, size_t page, const char *name, CGRect box) {
  [output appendFormat:@"Page %4zu %-9s %8.2f %8.2f %8.2f %8.2f\n", page,
                       [[NSString stringWithFormat:@"%s:", name] UTF8String],
                       CGRectGetMinX(box), CGRectGetMinY(box), CGRectGetMaxX(box),
                       CGRectGetMaxY(box)];
}

static int runInfo(int argc, const char *argv[]) {
  size_t first = 1;
  size_t last = 0;
  BOOL boxes = NO;
  NSString *path = nil;
  for (int index = 2; index < argc; index++) {
    if (strcmp(argv[index], "-f") == 0 && index + 1 < argc) {
      if (!parsePage(argv[++index], &first)) {
        return fail(@"Invalid first page.", 99);
      }
    } else if (strcmp(argv[index], "-l") == 0 && index + 1 < argc) {
      if (!parsePage(argv[++index], &last)) {
        return fail(@"Invalid last page.", 99);
      }
    } else if (strcmp(argv[index], "-box") == 0) {
      boxes = YES;
    } else if (path == nil && argv[index][0] != '-') {
      path = pathArgument(argv[index]);
    } else {
      return fail([NSString stringWithFormat:@"Unknown option: %s", argv[index]], 99);
    }
  }
  int status = 0;
  CGPDFDocumentRef document = openDocument(path, &status);
  if (document == NULL) {
    return status;
  }
  size_t count = CGPDFDocumentGetNumberOfPages(document);
  NSMutableString *output = [NSMutableString string];
  [output appendFormat:@"Pages:           %zu\n", count];
  [output appendFormat:@"Encrypted:       %@\n", CGPDFDocumentIsEncrypted(document) ? @"yes" : @"no"];

  CGRect firstCrop = CGPDFPageGetBoxRect(CGPDFDocumentGetPage(document, 1), kCGPDFCropBox);
  NSString *paper = paperName(firstCrop.size.width, firstCrop.size.height);
  [output appendFormat:@"Page size:       %@ x %@ pts%@\n", number(firstCrop.size.width),
                       number(firstCrop.size.height),
                       paper ? [NSString stringWithFormat:@" (%@)", paper] : @""];

  // Poppler prints per-page lines only when a page range is given.
  BOOL ranged = last > 0 || first > 1;
  if (ranged) {
    if (last == 0) {
      last = first;
    }
    if (first > last || last > count) {
      CGPDFDocumentRelease(document);
      return fail(@"Wrong page range given.", 99);
    }
    for (size_t page = first; page <= last; page++) {
      CGPDFPageRef pdfPage = CGPDFDocumentGetPage(document, page);
      CGRect crop = CGPDFPageGetBoxRect(pdfPage, kCGPDFCropBox);
      [output appendFormat:@"Page %4zu size: %@ x %@ pts\n", page, number(crop.size.width),
                           number(crop.size.height)];
      [output appendFormat:@"Page %4zu rot:  %d\n", page, CGPDFPageGetRotationAngle(pdfPage)];
      if (boxes) {
        appendBox(output, page, "MediaBox", CGPDFPageGetBoxRect(pdfPage, kCGPDFMediaBox));
        appendBox(output, page, "CropBox", crop);
        appendBox(output, page, "BleedBox", CGPDFPageGetBoxRect(pdfPage, kCGPDFBleedBox));
        appendBox(output, page, "TrimBox", CGPDFPageGetBoxRect(pdfPage, kCGPDFTrimBox));
        appendBox(output, page, "ArtBox", CGPDFPageGetBoxRect(pdfPage, kCGPDFArtBox));
      }
    }
  }
  CGPDFDocumentRelease(document);
  fputs(output.UTF8String, stdout);
  return 0;
}

static int runRender(int argc, const char *argv[]) {
  size_t first = 0;
  size_t last = 0;
  double dpi = 150;
  BOOL jpeg = NO;
  double quality = 0.9;
  NSString *input = nil;
  NSString *prefix = nil;
  for (int index = 2; index < argc; index++) {
    const char *flag = argv[index];
    if (strcmp(flag, "-f") == 0 && index + 1 < argc) {
      if (!parsePage(argv[++index], &first)) {
        return fail(@"Invalid first page.", 99);
      }
    } else if (strcmp(flag, "-l") == 0 && index + 1 < argc) {
      if (!parsePage(argv[++index], &last)) {
        return fail(@"Invalid last page.", 99);
      }
    } else if (strcmp(flag, "-r") == 0 && index + 1 < argc) {
      dpi = atof(argv[++index]);
    } else if (strcmp(flag, "-jpegopt") == 0 && index + 1 < argc) {
      const char *option = argv[++index];
      if (strncmp(option, "quality=", 8) == 0) {
        quality = atoi(option + 8) / 100.0;
      }
    } else if (strcmp(flag, "-png") == 0) {
      jpeg = NO;
    } else if (strcmp(flag, "-jpeg") == 0) {
      jpeg = YES;
    } else if (strcmp(flag, "-singlefile") == 0 || strcmp(flag, "-cropbox") == 0) {
      // ConvertKit always renders one cropped page per call.
    } else if (input == nil && flag[0] != '-') {
      input = pathArgument(flag);
    } else if (prefix == nil && flag[0] != '-') {
      prefix = pathArgument(flag);
    } else {
      return fail([NSString stringWithFormat:@"Unknown option: %s", flag], 99);
    }
  }
  if (first == 0 || (last != 0 && last != first) || prefix == nil || dpi <= 0 || dpi > 2400 ||
      quality <= 0 || quality > 1) {
    return fail(@"Render one page: -f N -l N -singlefile -r DPI <pdf> <prefix>", 99);
  }

  NSURL *inputURL = [NSURL fileURLWithPath:input ?: @"" isDirectory:NO];
  int status = 0;
  CGPDFDocumentRef check = openDocument(input, &status);
  if (check == NULL) {
    return status;
  }
  size_t count = CGPDFDocumentGetNumberOfPages(check);
  CGPDFDocumentRelease(check);
  if (first > count) {
    return fail(@"Wrong page range given.", 99);
  }

  PDFDocument *document = [[PDFDocument alloc] initWithURL:inputURL];
  if (document.isLocked) {
    [document unlockWithPassword:@""];
  }
  PDFPage *page = [document pageAtIndex:first - 1];
  if (page == nil) {
    return fail(@"The page could not be loaded.", 1);
  }
  // Bounds already include the page's rotation, matching pdftoppm.
  CGRect bounds = [page boundsForBox:kPDFDisplayBoxCropBox];
  NSInteger rotation = ((page.rotation % 360) + 360) % 360;
  CGFloat pointsWide = (rotation == 90 || rotation == 270) ? bounds.size.height : bounds.size.width;
  CGFloat pointsHigh = (rotation == 90 || rotation == 270) ? bounds.size.width : bounds.size.height;
  size_t width = (size_t)ceil(pointsWide * dpi / 72.0);
  size_t height = (size_t)ceil(pointsHigh * dpi / 72.0);
  if (width == 0 || height == 0 || width > kMaxRenderEdge || height > kMaxRenderEdge) {
    return fail(@"The rendered page would be too large.", 1);
  }

  CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  CGContextRef context = CGBitmapContextCreate(NULL, width, height, 8, 0, space,
                                               (CGBitmapInfo)kCGImageAlphaNoneSkipLast);
  CGColorSpaceRelease(space);
  if (context == NULL) {
    return fail(@"The page canvas could not be allocated.", 1);
  }
  CGContextSetRGBFillColor(context, 1, 1, 1, 1);
  CGContextFillRect(context, CGRectMake(0, 0, width, height));
  CGContextSetInterpolationQuality(context, kCGInterpolationHigh);
  CGContextScaleCTM(context, dpi / 72.0, dpi / 72.0);
  // PDFKit draws annotations and form appearances, as Poppler does, and
  // applies the page rotation inside the rotated bounds.
  [page drawWithBox:kPDFDisplayBoxCropBox toContext:context];
  CGImageRef image = CGBitmapContextCreateImage(context);
  CGContextRelease(context);
  if (image == NULL) {
    return fail(@"The page could not be rendered.", 1);
  }

  NSString *outputPath = [prefix stringByAppendingPathExtension:jpeg ? @"jpg" : @"png"];
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath isDirectory:NO];
  CGImageDestinationRef destination = CGImageDestinationCreateWithURL(
      (__bridge CFURLRef)outputURL, jpeg ? CFSTR("public.jpeg") : CFSTR("public.png"), 1, NULL);
  if (destination == NULL) {
    CGImageRelease(image);
    return fail(@"The page image could not be created.", 1);
  }
  NSDictionary *properties =
      jpeg ? @{(__bridge NSString *)kCGImageDestinationLossyCompressionQuality : @(quality)} : @{};
  CGImageDestinationAddImage(destination, image, (__bridge CFDictionaryRef)properties);
  BOOL written = CGImageDestinationFinalize(destination);
  CFRelease(destination);
  CGImageRelease(image);
  if (!written) {
    [[NSFileManager defaultManager] removeItemAtURL:outputURL error:nil];
    return fail(@"The page image could not be encoded.", 1);
  }
  return 0;
}

static int runText(int argc, const char *argv[]) {
  NSString *input = nil;
  NSString *output = nil;
  for (int index = 2; index < argc; index++) {
    const char *flag = argv[index];
    if ((strcmp(flag, "-enc") == 0 || strcmp(flag, "-eol") == 0) && index + 1 < argc) {
      index++;  // Output is always UTF-8 with Unix line endings.
    } else if (strcmp(flag, "-nopgbrk") == 0) {
      // Pages are joined by newlines, never form feeds.
    } else if (input == nil && flag[0] != '-') {
      input = pathArgument(flag);
    } else if (output == nil && flag[0] != '-') {
      output = pathArgument(flag);
    } else {
      return fail([NSString stringWithFormat:@"Unknown option: %s", flag], 99);
    }
  }
  if (output == nil) {
    return fail(@"Usage: pdftotext [options] <pdf> <text-file>", 99);
  }
  int status = 0;
  CGPDFDocumentRef check = openDocument(input, &status);
  if (check == NULL) {
    return status;
  }
  CGPDFDocumentRelease(check);

  PDFDocument *document = [[PDFDocument alloc] initWithURL:[NSURL fileURLWithPath:input]];
  if (document.isLocked) {
    [document unlockWithPassword:@""];
  }
  NSMutableString *text = [NSMutableString string];
  for (NSInteger index = 0; index < document.pageCount; index++) {
    @autoreleasepool {
      NSString *pageText = [document pageAtIndex:index].string ?: @"";
      pageText = [pageText stringByReplacingOccurrencesOfString:@"\r\n" withString:@"\n"];
      pageText = [pageText stringByReplacingOccurrencesOfString:@"\r" withString:@"\n"];
      [text appendString:pageText];
      if (![pageText hasSuffix:@"\n"]) {
        [text appendString:@"\n"];
      }
    }
  }
  NSError *error = nil;
  if (![text writeToFile:output atomically:YES encoding:NSUTF8StringEncoding error:&error]) {
    return fail(@"The text output could not be written.", 1);
  }
  return 0;
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc >= 2 && strcmp(argv[1], "pdfinfo") == 0) {
      return runInfo(argc, argv);
    }
    if (argc >= 2 && strcmp(argv[1], "pdftoppm") == 0) {
      return runRender(argc, argv);
    }
    if (argc >= 2 && strcmp(argv[1], "pdftotext") == 0) {
      return runText(argc, argv);
    }
    return fail(@"Usage: convertkit-pdf (pdfinfo|pdftoppm|pdftotext) [Poppler options]", 99);
  }
}
