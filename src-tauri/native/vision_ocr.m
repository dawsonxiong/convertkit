#import <CoreGraphics/CoreGraphics.h>
#import <CoreText/CoreText.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <PDFKit/PDFKit.h>
#import <Vision/Vision.h>

#import <limits.h>
#import <math.h>
#import <signal.h>
#import <unistd.h>

static const NSUInteger CKMaxPages = 250;
static const NSUInteger CKMaxObservationsPerPage = 20000;
static const NSUInteger CKMaxTextBytesPerPage = 2 * 1024 * 1024;
static const NSUInteger CKMaxTotalTextBytes = 32 * 1024 * 1024;
static const unsigned long long CKMaxInputBytes = 4ULL * 1024ULL * 1024ULL * 1024ULL;
static const size_t CKMaxPixelEdge = 8192;
static const double CKMaxPixelArea = 24.0 * 1024.0 * 1024.0;
static const CGFloat CKMaxPageEdge = 14400.0;
static const CGFloat CKMaxPageCoordinate = 10000000.0;
static const CGFloat CKOCRDPI = 220.0;

static char CKWorkingOutput[PATH_MAX] = {0};

@interface CKRecognizedLine : NSObject
@property(nonatomic, copy) NSString *text;
@property(nonatomic) CGPoint topLeft;
@property(nonatomic) CGPoint topRight;
@property(nonatomic) CGPoint bottomLeft;
@property(nonatomic) CGPoint bottomRight;
@end

@implementation CKRecognizedLine
@end

typedef struct {
  size_t width;
  size_t height;
} CKPixelSize;

static NSError *CKError(NSString *message) {
  return [NSError errorWithDomain:@"com.convertkit.vision-ocr"
                             code:1
                         userInfo:@{NSLocalizedDescriptionKey : message}];
}

static void CKAssignError(NSError **error, NSString *message) {
  if (error != NULL) {
    *error = CKError(message);
  }
}

static int CKFail(NSString *message, int code) {
  NSData *data = [[message stringByAppendingString:@"\n"] dataUsingEncoding:NSUTF8StringEncoding];
  [data writeToFile:@"/dev/stderr" atomically:NO];
  return code;
}

static void CKRemoveOutput(NSString *path) {
  if (path.length > 0) {
    [[NSFileManager defaultManager] removeItemAtPath:path error:nil];
  }
}

static void CKHandleTermination(int signalNumber) {
  if (CKWorkingOutput[0] != '\0') {
    unlink(CKWorkingOutput);
  }
  _exit(128 + signalNumber);
}

static BOOL CKInstallTerminationCleanup(NSString *outputPath, NSError **error) {
  const char *fileSystemPath = outputPath.fileSystemRepresentation;
  if (fileSystemPath == NULL || strlen(fileSystemPath) >= sizeof(CKWorkingOutput)) {
    CKAssignError(error, @"The working output path is too long");
    return NO;
  }

  strlcpy(CKWorkingOutput, fileSystemPath, sizeof(CKWorkingOutput));
  signal(SIGINT, CKHandleTermination);
  signal(SIGTERM, CKHandleTermination);
  signal(SIGHUP, CKHandleTermination);
  return YES;
}

static void CKClearTerminationCleanup(void) {
  CKWorkingOutput[0] = '\0';
}

static BOOL CKFinitePoint(CGPoint point) {
  return isfinite(point.x) && isfinite(point.y);
}

static BOOL CKFiniteRect(CGRect rect) {
  return CKFinitePoint(rect.origin) && isfinite(rect.size.width) && isfinite(rect.size.height) &&
         rect.size.width > 0.0 && rect.size.height > 0.0;
}

static BOOL CKFiniteTransform(CGAffineTransform transform) {
  const double values[] = {transform.a, transform.b,  transform.c,
                           transform.d, transform.tx, transform.ty};
  for (NSUInteger index = 0; index < sizeof(values) / sizeof(values[0]); index++) {
    if (!isfinite(values[index]) || fabs(values[index]) > 1.0e12) {
      return NO;
    }
  }
  return YES;
}

static BOOL CKNormalizePoint(CGPoint input, CGPoint *output) {
  if (!CKFinitePoint(input) || input.x < -0.01 || input.x > 1.01 || input.y < -0.01 ||
      input.y > 1.01) {
    return NO;
  }

  output->x = fmin(1.0, fmax(0.0, input.x));
  output->y = fmin(1.0, fmax(0.0, input.y));
  return YES;
}

static BOOL CKValidateInput(NSString *inputPath, NSString *outputPath, NSError **error) {
  NSFileManager *fileManager = [NSFileManager defaultManager];
  BOOL isDirectory = NO;
  if (![fileManager fileExistsAtPath:inputPath isDirectory:&isDirectory] || isDirectory) {
    CKAssignError(error, @"Input file does not exist");
    return NO;
  }

  NSDictionary<NSFileAttributeKey, id> *attributes =
      [fileManager attributesOfItemAtPath:inputPath error:error];
  if (attributes == nil) {
    return NO;
  }

  unsigned long long fileSize = [attributes[NSFileSize] unsignedLongLongValue];
  if (fileSize == 0 || fileSize > CKMaxInputBytes) {
    CKAssignError(error, @"Input file size is outside the supported range");
    return NO;
  }

  NSString *resolvedInput = inputPath.stringByStandardizingPath.stringByResolvingSymlinksInPath;
  NSString *resolvedOutput = outputPath.stringByStandardizingPath.stringByResolvingSymlinksInPath;
  if ([resolvedInput isEqualToString:resolvedOutput]) {
    CKAssignError(error, @"Input and working output must be different files");
    return NO;
  }

  NSString *parent = outputPath.stringByDeletingLastPathComponent;
  BOOL parentIsDirectory = NO;
  if (![fileManager fileExistsAtPath:parent isDirectory:&parentIsDirectory] || !parentIsDirectory) {
    CKAssignError(error, @"Working output directory does not exist");
    return NO;
  }

  return YES;
}

static BOOL CKInputLooksLikePDF(NSString *inputPath) {
  NSFileHandle *handle = [NSFileHandle fileHandleForReadingAtPath:inputPath];
  if (handle == nil) {
    return NO;
  }
  NSData *header = [handle readDataOfLength:1024];
  [handle closeFile];
  NSData *signature = [@"%PDF-" dataUsingEncoding:NSASCIIStringEncoding];
  return [header rangeOfData:signature options:0 range:NSMakeRange(0, header.length)].location !=
         NSNotFound;
}

static BOOL CKPixelSizeForPage(CGSize pointSize, CKPixelSize *pixelSize, NSError **error) {
  double width = pointSize.width * CKOCRDPI / 72.0;
  double height = pointSize.height * CKOCRDPI / 72.0;
  if (!isfinite(width) || !isfinite(height) || width <= 0.0 || height <= 0.0) {
    CKAssignError(error, @"Page has invalid raster dimensions");
    return NO;
  }

  double scale = 1.0;
  double longestEdge = fmax(width, height);
  if (longestEdge > (double)CKMaxPixelEdge) {
    scale = fmin(scale, (double)CKMaxPixelEdge / longestEdge);
  }

  double area = width * height;
  if (!isfinite(area) || area <= 0.0) {
    CKAssignError(error, @"Page has invalid pixel area");
    return NO;
  }
  if (area > CKMaxPixelArea) {
    scale = fmin(scale, sqrt(CKMaxPixelArea / area));
  }

  double boundedWidth = ceil(width * scale);
  double boundedHeight = ceil(height * scale);
  if (!isfinite(boundedWidth) || !isfinite(boundedHeight) || boundedWidth < 1.0 ||
      boundedHeight < 1.0 || boundedWidth > (double)CKMaxPixelEdge ||
      boundedHeight > (double)CKMaxPixelEdge) {
    CKAssignError(error, @"Page raster exceeds safe bounds");
    return NO;
  }

  pixelSize->width = (size_t)boundedWidth;
  pixelSize->height = (size_t)boundedHeight;
  return YES;
}

static CGContextRef CKCreateBitmapContext(CKPixelSize size, NSError **error) {
  if (size.width == 0 || size.height == 0 || size.width > SIZE_MAX / 4) {
    CKAssignError(error, @"Page raster dimensions overflow");
    return NULL;
  }

  size_t rowBytes = size.width * 4;
  if (size.height > SIZE_MAX / rowBytes) {
    CKAssignError(error, @"Page raster allocation overflows");
    return NULL;
  }

  CGColorSpaceRef colorSpace = CGColorSpaceCreateDeviceRGB();
  CGContextRef context = CGBitmapContextCreate(NULL, size.width, size.height, 8, rowBytes, colorSpace,
                                               kCGImageAlphaPremultipliedLast |
                                                   kCGBitmapByteOrder32Big);
  CGColorSpaceRelease(colorSpace);
  if (context == NULL) {
    CKAssignError(error, @"Could not allocate a bounded page raster");
  }
  return context;
}

static NSArray<CKRecognizedLine *> *CKRecognizeImage(CGImageRef image, NSError **error) {
  VNRecognizeTextRequest *request = [VNRecognizeTextRequest new];
  request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
  request.usesLanguageCorrection = YES;
  if (@available(macOS 13.0, *)) {
    request.automaticallyDetectsLanguage = YES;
  }

  VNImageRequestHandler *handler = [[VNImageRequestHandler alloc] initWithCGImage:image options:@{}];
  if (![handler performRequests:@[ request ] error:error]) {
    return nil;
  }

  NSArray<VNRecognizedTextObservation *> *results = request.results ?: @[];
  if (results.count > CKMaxObservationsPerPage) {
    CKAssignError(error, @"OCR produced too many text observations on one page");
    return nil;
  }

  NSMutableArray<CKRecognizedLine *> *lines = [NSMutableArray arrayWithCapacity:results.count];
  NSUInteger textBytes = 0;
  for (VNRecognizedTextObservation *observation in results) {
    VNRecognizedText *candidate = [observation topCandidates:1].firstObject;
    NSString *text = candidate.string;
    if (text.length == 0) {
      continue;
    }

    NSData *encoded = [text dataUsingEncoding:NSUTF8StringEncoding];
    if (encoded == nil || encoded.length > CKMaxTextBytesPerPage - textBytes) {
      CKAssignError(error, @"OCR text exceeds the safe per-page bound");
      return nil;
    }

    VNRectangleObservation *geometry = observation;
    if (candidate.string.length > 0) {
      VNRectangleObservation *candidateGeometry =
          [candidate boundingBoxForRange:NSMakeRange(0, candidate.string.length) error:nil];
      if (candidateGeometry != nil) {
        geometry = candidateGeometry;
      }
    }

    CGPoint topLeft;
    CGPoint topRight;
    CGPoint bottomLeft;
    CGPoint bottomRight;
    if (!CKNormalizePoint(geometry.topLeft, &topLeft) ||
        !CKNormalizePoint(geometry.topRight, &topRight) ||
        !CKNormalizePoint(geometry.bottomLeft, &bottomLeft) ||
        !CKNormalizePoint(geometry.bottomRight, &bottomRight)) {
      continue;
    }

    CKRecognizedLine *line = [CKRecognizedLine new];
    line.topLeft = topLeft;
    line.topRight = topRight;
    line.bottomLeft = bottomLeft;
    line.bottomRight = bottomRight;
    line.text = text;
    textBytes += encoded.length;
    [lines addObject:line];
  }

  [lines sortUsingComparator:^NSComparisonResult(CKRecognizedLine *left, CKRecognizedLine *right) {
    CGFloat leftTop = fmax(left.topLeft.y, left.topRight.y);
    CGFloat rightTop = fmax(right.topLeft.y, right.topRight.y);
    if (fabs(leftTop - rightTop) > 0.01) {
      return leftTop > rightTop ? NSOrderedAscending : NSOrderedDescending;
    }
    CGFloat leftX = fmin(left.topLeft.x, left.bottomLeft.x);
    CGFloat rightX = fmin(right.topLeft.x, right.bottomLeft.x);
    if (leftX == rightX) {
      return NSOrderedSame;
    }
    return leftX < rightX ? NSOrderedAscending : NSOrderedDescending;
  }];

  return lines;
}

static NSUInteger CKUTF8BytesForLines(NSArray<CKRecognizedLine *> *lines) {
  NSUInteger bytes = 0;
  for (CKRecognizedLine *line in lines) {
    bytes += [line.text lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
  }
  return bytes;
}

static void CKDrawInvisibleText(CGContextRef context, NSArray<CKRecognizedLine *> *lines,
                                CGSize pageSize) {
  CTFontRef font = CTFontCreateUIFontForLanguage(kCTFontUIFontSystem, 100.0, NULL);
  if (font == NULL) {
    font = CTFontCreateWithName(CFSTR("Helvetica"), 100.0, NULL);
  }

  NSDictionary *attributes = @{(__bridge NSString *)kCTFontAttributeName : (__bridge id)font};
  for (CKRecognizedLine *recognized in lines) {
    CGPoint bottomLeft = CGPointMake(recognized.bottomLeft.x * pageSize.width,
                                     recognized.bottomLeft.y * pageSize.height);
    CGPoint bottomRight = CGPointMake(recognized.bottomRight.x * pageSize.width,
                                      recognized.bottomRight.y * pageSize.height);
    CGPoint topLeft = CGPointMake(recognized.topLeft.x * pageSize.width,
                                  recognized.topLeft.y * pageSize.height);
    CGFloat lineWidth = hypot(bottomRight.x - bottomLeft.x, bottomRight.y - bottomLeft.y);
    CGFloat lineHeight = hypot(topLeft.x - bottomLeft.x, topLeft.y - bottomLeft.y);
    if (!isfinite(lineWidth) || !isfinite(lineHeight) || lineWidth < 0.25 || lineHeight < 0.25) {
      continue;
    }

    NSAttributedString *attributed =
        [[NSAttributedString alloc] initWithString:recognized.text attributes:attributes];
    CTLineRef line = CTLineCreateWithAttributedString((__bridge CFAttributedStringRef)attributed);
    CGFloat ascent = 0.0;
    CGFloat descent = 0.0;
    double typographicWidth = CTLineGetTypographicBounds(line, &ascent, &descent, NULL);
    double typographicHeight = ascent + descent;
    if (!isfinite(typographicWidth) || !isfinite(typographicHeight) || typographicWidth <= 0.0 ||
        typographicHeight <= 0.0) {
      CFRelease(line);
      continue;
    }

    double horizontalScale = lineWidth / typographicWidth;
    double verticalScale = lineHeight / typographicHeight;
    if (!isfinite(horizontalScale) || !isfinite(verticalScale) || horizontalScale < 0.0001 ||
        horizontalScale > 100.0 || verticalScale < 0.0001 || verticalScale > 100.0) {
      CFRelease(line);
      continue;
    }

    CGContextSaveGState(context);
    CGContextSetTextMatrix(context, CGAffineTransformIdentity);
    CGContextSetTextDrawingMode(context, kCGTextInvisible);
    CGContextTranslateCTM(context, bottomLeft.x, bottomLeft.y);
    CGContextRotateCTM(context,
                       atan2(bottomRight.y - bottomLeft.y, bottomRight.x - bottomLeft.x));
    CGContextScaleCTM(context, horizontalScale, verticalScale);
    CGContextSetTextPosition(context, 0.0, descent);
    CTLineDraw(line, context);
    CGContextRestoreGState(context);
    CFRelease(line);
  }

  CFRelease(font);
}

static NSDictionary *CKPageDictionary(CGRect pageRect) {
  NSData *boxData = [NSData dataWithBytes:&pageRect length:sizeof(pageRect)];
  return @{(__bridge NSString *)kCGPDFContextMediaBox : boxData,
           (__bridge NSString *)kCGPDFContextCropBox : boxData};
}

static CGRect CKPDFSourceBox(CGPDFPageRef page, CGPDFBox *boxKind) {
  CGRect box = CGPDFPageGetBoxRect(page, kCGPDFCropBox);
  *boxKind = kCGPDFCropBox;
  if (!CKFiniteRect(box)) {
    box = CGPDFPageGetBoxRect(page, kCGPDFMediaBox);
    *boxKind = kCGPDFMediaBox;
  }
  return box;
}

static BOOL CKPDFPageSize(CGPDFPageRef page, CGSize *pageSize, CGPDFBox *boxKind,
                          NSError **error) {
  CGRect box = CKPDFSourceBox(page, boxKind);
  if (!CKFiniteRect(box)) {
    CKAssignError(error, @"PDF page has no finite crop or media box");
    return NO;
  }
  if (fabs(box.origin.x) > CKMaxPageCoordinate || fabs(box.origin.y) > CKMaxPageCoordinate) {
    CKAssignError(error, @"PDF page origin exceeds safe bounds");
    return NO;
  }

  NSInteger rotation = CGPDFPageGetRotationAngle(page) % 360;
  if (rotation < 0) {
    rotation += 360;
  }
  if (rotation % 90 != 0) {
    CKAssignError(error, @"PDF page has an unsupported rotation");
    return NO;
  }

  CGFloat width = box.size.width;
  CGFloat height = box.size.height;
  if (rotation == 90 || rotation == 270) {
    CGFloat temporary = width;
    width = height;
    height = temporary;
  }
  if (!isfinite(width) || !isfinite(height) || width <= 0.0 || height <= 0.0 ||
      width > CKMaxPageEdge || height > CKMaxPageEdge) {
    CKAssignError(error, @"PDF page geometry exceeds safe bounds");
    return NO;
  }

  *pageSize = CGSizeMake(width, height);
  return YES;
}

static CGImageRef CKRasterizePDFPage(CGPDFPageRef page, CGPDFBox boxKind, CGSize pageSize,
                                     NSError **error) {
  CKPixelSize pixelSize;
  if (!CKPixelSizeForPage(pageSize, &pixelSize, error)) {
    return NULL;
  }

  CGContextRef bitmap = CKCreateBitmapContext(pixelSize, error);
  if (bitmap == NULL) {
    return NULL;
  }

  CGRect target = CGRectMake(0.0, 0.0, pixelSize.width, pixelSize.height);
  CGContextSetRGBFillColor(bitmap, 1.0, 1.0, 1.0, 1.0);
  CGContextFillRect(bitmap, target);
  CGAffineTransform transform = CGPDFPageGetDrawingTransform(page, boxKind, target, 0, true);
  if (!CKFiniteTransform(transform)) {
    CGContextRelease(bitmap);
    CKAssignError(error, @"PDF page transform exceeds safe bounds");
    return NULL;
  }
  CGContextConcatCTM(bitmap, transform);
  CGContextDrawPDFPage(bitmap, page);
  CGImageRef image = CGBitmapContextCreateImage(bitmap);
  CGContextRelease(bitmap);
  if (image == NULL) {
    CKAssignError(error, @"Could not rasterize PDF page for OCR");
  }
  return image;
}

static BOOL CKDrawPDFPage(CGContextRef context, CGPDFPageRef page, CGPDFBox boxKind,
                          CGSize pageSize, NSError **error) {
  CGRect target = CGRectMake(0.0, 0.0, pageSize.width, pageSize.height);
  CGContextSetRGBFillColor(context, 1.0, 1.0, 1.0, 1.0);
  CGContextFillRect(context, target);
  CGContextSaveGState(context);
  CGAffineTransform transform = CGPDFPageGetDrawingTransform(page, boxKind, target, 0, true);
  if (!CKFiniteTransform(transform)) {
    CGContextRestoreGState(context);
    CKAssignError(error, @"PDF page transform exceeds safe bounds");
    return NO;
  }
  CGContextConcatCTM(context, transform);
  CGContextDrawPDFPage(context, page);
  CGContextRestoreGState(context);
  return YES;
}

static BOOL CKImageDimensionsAtIndex(CGImageSourceRef source, size_t index, size_t *width,
                                     size_t *height, NSError **error) {
  NSDictionary *properties =
      CFBridgingRelease(CGImageSourceCopyPropertiesAtIndex(source, index, NULL));
  NSNumber *widthNumber = properties[(__bridge NSString *)kCGImagePropertyPixelWidth];
  NSNumber *heightNumber = properties[(__bridge NSString *)kCGImagePropertyPixelHeight];
  unsigned long long rawWidth = widthNumber.unsignedLongLongValue;
  unsigned long long rawHeight = heightNumber.unsignedLongLongValue;
  if (rawWidth == 0 || rawHeight == 0 || rawWidth > SIZE_MAX || rawHeight > SIZE_MAX) {
    CKAssignError(error, @"Image frame has invalid pixel dimensions");
    return NO;
  }
  *width = (size_t)rawWidth;
  *height = (size_t)rawHeight;
  return YES;
}

static CGImageRef CKDecodeImageAtIndex(CGImageSourceRef source, size_t index, NSError **error) {
  size_t rawWidth = 0;
  size_t rawHeight = 0;
  if (!CKImageDimensionsAtIndex(source, index, &rawWidth, &rawHeight, error)) {
    return NULL;
  }

  double scale = 1.0;
  size_t longestEdge = MAX(rawWidth, rawHeight);
  if (longestEdge > CKMaxPixelEdge) {
    scale = fmin(scale, (double)CKMaxPixelEdge / (double)longestEdge);
  }
  double area = (double)rawWidth * (double)rawHeight;
  if (!isfinite(area) || area <= 0.0) {
    CKAssignError(error, @"Image frame has invalid pixel area");
    return NULL;
  }
  if (area > CKMaxPixelArea) {
    scale = fmin(scale, sqrt(CKMaxPixelArea / area));
  }
  size_t maximumPixelSize = (size_t)ceil((double)longestEdge * scale);
  maximumPixelSize = MAX((size_t)1, MIN(maximumPixelSize, CKMaxPixelEdge));

  NSDictionary *options = @{
    (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways : @YES,
    (__bridge NSString *)kCGImageSourceCreateThumbnailWithTransform : @YES,
    (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize : @(maximumPixelSize),
    (__bridge NSString *)kCGImageSourceShouldCacheImmediately : @YES,
  };
  CGImageRef image = CGImageSourceCreateThumbnailAtIndex(
      source, index, (__bridge CFDictionaryRef)options);
  if (image == NULL) {
    CKAssignError(error, @"Image frame could not be decoded within safe bounds");
  }
  return image;
}

static BOOL CKImagePageSize(CGImageRef image, CGSize *pageSize, NSError **error) {
  CGFloat width = (CGFloat)CGImageGetWidth(image);
  CGFloat height = (CGFloat)CGImageGetHeight(image);
  if (!isfinite(width) || !isfinite(height) || width <= 0.0 || height <= 0.0) {
    CKAssignError(error, @"Decoded image has invalid dimensions");
    return NO;
  }

  CGFloat scale = fmin(1.0, CKMaxPageEdge / fmax(width, height));
  width *= scale;
  height *= scale;
  if (width <= 0.0 || height <= 0.0 || width > CKMaxPageEdge || height > CKMaxPageEdge) {
    CKAssignError(error, @"Image page geometry exceeds safe bounds");
    return NO;
  }
  *pageSize = CGSizeMake(width, height);
  return YES;
}

static void CKDrawImagePage(CGContextRef context, CGImageRef image, CGSize pageSize) {
  CGRect pageRect = CGRectMake(0.0, 0.0, pageSize.width, pageSize.height);
  CGContextSetRGBFillColor(context, 1.0, 1.0, 1.0, 1.0);
  CGContextFillRect(context, pageRect);
  CGContextDrawImage(context, pageRect, image);
}

static void CKWritePageProgress(NSUInteger page, NSUInteger total) {
  fprintf(stdout, "{\"page\":%lu,\"total\":%lu}\n", (unsigned long)page,
          (unsigned long)total);
  fflush(stdout);
}

static BOOL CKValidateOutput(NSString *outputPath, NSArray<NSValue *> *expectedSizes,
                             NSError **error) {
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath];
  CGPDFDocumentRef document = CGPDFDocumentCreateWithURL((__bridge CFURLRef)outputURL);
  if (document == NULL) {
    CKAssignError(error, @"Searchable PDF output could not be reopened");
    return NO;
  }

  size_t pageCount = CGPDFDocumentGetNumberOfPages(document);
  if (pageCount != expectedSizes.count) {
    CGPDFDocumentRelease(document);
    CKAssignError(error, @"Searchable PDF output has the wrong page count");
    return NO;
  }

  for (size_t index = 0; index < pageCount; index++) {
    CGPDFPageRef page = CGPDFDocumentGetPage(document, index + 1);
    CGRect mediaBox = page == NULL ? CGRectZero : CGPDFPageGetBoxRect(page, kCGPDFMediaBox);
    CGSize expected = expectedSizes[index].sizeValue;
    if (!CKFiniteRect(mediaBox) || fabs(mediaBox.size.width - expected.width) > 0.5 ||
        fabs(mediaBox.size.height - expected.height) > 0.5) {
      CGPDFDocumentRelease(document);
      CKAssignError(error, @"Searchable PDF output has invalid page geometry");
      return NO;
    }
  }
  CGPDFDocumentRelease(document);

  PDFDocument *kitDocument = [[PDFDocument alloc] initWithURL:outputURL];
  if (kitDocument == nil || kitDocument.pageCount != expectedSizes.count) {
    CKAssignError(error, @"Searchable PDF output failed PDFKit validation");
    return NO;
  }
  BOOL foundExtractedText = NO;
  for (NSUInteger index = 0; index < kitDocument.pageCount; index++) {
    NSString *pageText = [kitDocument pageAtIndex:index].string ?: @"";
    if ([[pageText stringByTrimmingCharactersInSet:NSCharacterSet.whitespaceAndNewlineCharacterSet]
            length] > 0) {
      foundExtractedText = YES;
      break;
    }
  }
  if (!foundExtractedText) {
    CKAssignError(error, @"Searchable PDF output contains no extractable text page");
    return NO;
  }
  return YES;
}

static BOOL CKWriteSearchablePDF(NSString *inputPath, NSString *outputPath, NSError **error) {
  if (!CKValidateInput(inputPath, outputPath, error)) {
    return NO;
  }
  CKRemoveOutput(outputPath);
  if (!CKInstallTerminationCleanup(outputPath, error)) {
    return NO;
  }

  NSURL *inputURL = [NSURL fileURLWithPath:inputPath];
  NSURL *outputURL = [NSURL fileURLWithPath:outputPath];
  BOOL expectsPDF = CKInputLooksLikePDF(inputPath);
  CGPDFDocumentRef sourcePDF =
      expectsPDF ? CGPDFDocumentCreateWithURL((__bridge CFURLRef)inputURL) : NULL;
  CGImageSourceRef imageSource = NULL;
  PDFDocument *kitSource = nil;
  NSUInteger pageCount = 0;

  if (expectsPDF) {
    if (sourcePDF == NULL || CGPDFDocumentGetNumberOfPages(sourcePDF) == 0) {
      if (sourcePDF != NULL) {
        CGPDFDocumentRelease(sourcePDF);
      }
      CKClearTerminationCleanup();
      CKAssignError(error, @"Input PDF is invalid or has no pages");
      return NO;
    }
    if (CGPDFDocumentIsEncrypted(sourcePDF) && !CGPDFDocumentIsUnlocked(sourcePDF)) {
      CGPDFDocumentRelease(sourcePDF);
      CKClearTerminationCleanup();
      CKAssignError(error, @"Encrypted PDF must be unlocked before OCR");
      return NO;
    }
    if (CGPDFDocumentIsEncrypted(sourcePDF) && !CGPDFDocumentAllowsCopying(sourcePDF)) {
      CGPDFDocumentRelease(sourcePDF);
      CKClearTerminationCleanup();
      CKAssignError(error, @"PDF permissions do not allow searchable-text extraction");
      return NO;
    }
    pageCount = CGPDFDocumentGetNumberOfPages(sourcePDF);
    kitSource = [[PDFDocument alloc] initWithURL:inputURL];
    if (kitSource == nil || kitSource.isLocked || kitSource.pageCount != pageCount) {
      CGPDFDocumentRelease(sourcePDF);
      CKClearTerminationCleanup();
      CKAssignError(error, @"PDF could not be opened safely");
      return NO;
    }
  } else {
    imageSource = CGImageSourceCreateWithURL((__bridge CFURLRef)inputURL, NULL);
    if (imageSource == NULL) {
      CKClearTerminationCleanup();
      CKAssignError(error, @"Input is not a supported PDF or image");
      return NO;
    }
    pageCount = CGImageSourceGetCount(imageSource);
  }

  if (pageCount == 0 || pageCount > CKMaxPages) {
    if (sourcePDF != NULL) {
      CGPDFDocumentRelease(sourcePDF);
    }
    if (imageSource != NULL) {
      CFRelease(imageSource);
    }
    CKClearTerminationCleanup();
    CKAssignError(error, @"Input page count is outside the supported range");
    return NO;
  }

  CGRect defaultPage = CGRectMake(0.0, 0.0, 612.0, 792.0);
  CGContextRef output = CGPDFContextCreateWithURL((__bridge CFURLRef)outputURL, &defaultPage, NULL);
  if (output == NULL) {
    if (sourcePDF != NULL) {
      CGPDFDocumentRelease(sourcePDF);
    }
    if (imageSource != NULL) {
      CFRelease(imageSource);
    }
    CKRemoveOutput(outputPath);
    CKClearTerminationCleanup();
    CKAssignError(error, @"Could not create searchable PDF working output");
    return NO;
  }

  NSMutableArray<NSValue *> *expectedSizes = [NSMutableArray arrayWithCapacity:pageCount];
  NSUInteger totalTextBytes = 0;
  BOOL wroteAllPages = YES;
  NSError *pageError = nil;

  for (NSUInteger index = 0; index < pageCount; index++) {
    @autoreleasepool {
      CGImageRef recognitionImage = NULL;
      CGImageRef decodedImage = NULL;
      CGPDFPageRef sourcePage = NULL;
      CGPDFBox boxKind = kCGPDFCropBox;
      CGSize pageSize = CGSizeZero;
      BOOL preserveExistingText = NO;

      if (sourcePDF != NULL) {
        sourcePage = CGPDFDocumentGetPage(sourcePDF, index + 1);
        if (sourcePage == NULL || !CKPDFPageSize(sourcePage, &pageSize, &boxKind, &pageError)) {
          wroteAllPages = NO;
          break;
        }
        NSString *existing = [kitSource pageAtIndex:index].string ?: @"";
        existing = [existing stringByTrimmingCharactersInSet:
                                 NSCharacterSet.whitespaceAndNewlineCharacterSet];
        NSUInteger existingBytes = [existing lengthOfBytesUsingEncoding:NSUTF8StringEncoding];
        if (existingBytes > 0) {
          if (existingBytes > CKMaxTextBytesPerPage ||
              existingBytes > CKMaxTotalTextBytes - totalTextBytes) {
            pageError = CKError(@"Existing PDF text exceeds safe bounds");
            wroteAllPages = NO;
            break;
          }
          totalTextBytes += existingBytes;
          preserveExistingText = YES;
        } else {
          recognitionImage = CKRasterizePDFPage(sourcePage, boxKind, pageSize, &pageError);
        }
      } else {
        decodedImage = CKDecodeImageAtIndex(imageSource, index, &pageError);
        recognitionImage = decodedImage;
        if (decodedImage == NULL || !CKImagePageSize(decodedImage, &pageSize, &pageError)) {
          if (decodedImage != NULL) {
            CGImageRelease(decodedImage);
          }
          wroteAllPages = NO;
          break;
        }
      }

      if (!preserveExistingText && recognitionImage == NULL) {
        wroteAllPages = NO;
        if (pageError == nil) {
          pageError = CKError(@"Could not prepare page for OCR");
        }
        break;
      }

      NSArray<CKRecognizedLine *> *lines = @[];
      if (!preserveExistingText) {
        lines = CKRecognizeImage(recognitionImage, &pageError);
        if (lines == nil) {
          wroteAllPages = NO;
          if (decodedImage != NULL) {
            CGImageRelease(decodedImage);
          } else {
            CGImageRelease(recognitionImage);
          }
          break;
        }
        NSUInteger pageTextBytes = CKUTF8BytesForLines(lines);
        if (pageTextBytes > CKMaxTotalTextBytes - totalTextBytes) {
          wroteAllPages = NO;
          pageError = CKError(@"OCR text exceeds the safe document bound");
          if (decodedImage != NULL) {
            CGImageRelease(decodedImage);
          } else {
            CGImageRelease(recognitionImage);
          }
          break;
        }
        totalTextBytes += pageTextBytes;
      }

      CGRect pageRect = CGRectMake(0.0, 0.0, pageSize.width, pageSize.height);
      CGPDFContextBeginPage(output, (__bridge CFDictionaryRef)CKPageDictionary(pageRect));
      if (sourcePage != NULL) {
        if (!CKDrawPDFPage(output, sourcePage, boxKind, pageSize, &pageError)) {
          CGPDFContextEndPage(output);
          if (recognitionImage != NULL) {
            CGImageRelease(recognitionImage);
          }
          wroteAllPages = NO;
          break;
        }
      } else {
        CKDrawImagePage(output, decodedImage, pageSize);
      }
      if (lines.count > 0) {
        CKDrawInvisibleText(output, lines, pageSize);
      }
      CGPDFContextEndPage(output);

      [expectedSizes addObject:[NSValue valueWithSize:NSMakeSize(pageSize.width, pageSize.height)]];
      CKWritePageProgress(index + 1, pageCount);
      if (decodedImage != NULL) {
        CGImageRelease(decodedImage);
      } else if (recognitionImage != NULL) {
        CGImageRelease(recognitionImage);
      }
    }
  }

  CGPDFContextClose(output);
  CGContextRelease(output);
  if (sourcePDF != NULL) {
    CGPDFDocumentRelease(sourcePDF);
  }
  if (imageSource != NULL) {
    CFRelease(imageSource);
  }

  if (!wroteAllPages || expectedSizes.count != pageCount) {
    CKRemoveOutput(outputPath);
    CKClearTerminationCleanup();
    if (error != NULL) {
      *error = pageError ?: CKError(@"Searchable PDF generation failed");
    }
    return NO;
  }
  if (totalTextBytes == 0) {
    CKRemoveOutput(outputPath);
    CKClearTerminationCleanup();
    CKAssignError(error, @"No text was recognized in the input");
    return NO;
  }
  if (!CKValidateOutput(outputPath, expectedSizes, error)) {
    CKRemoveOutput(outputPath);
    CKClearTerminationCleanup();
    return NO;
  }

  CKClearTerminationCleanup();
  return YES;
}

static int CKRunPlainText(NSString *path) {
  BOOL isDirectory = NO;
  if (![[NSFileManager defaultManager] fileExistsAtPath:path isDirectory:&isDirectory] ||
      isDirectory) {
    return CKFail(@"Input image does not exist", 2);
  }

  VNRecognizeTextRequest *request = [VNRecognizeTextRequest new];
  request.recognitionLevel = VNRequestTextRecognitionLevelAccurate;
  request.usesLanguageCorrection = YES;
  if (@available(macOS 13.0, *)) {
    request.automaticallyDetectsLanguage = YES;
  }

  NSError *error = nil;
  VNImageRequestHandler *handler =
      [[VNImageRequestHandler alloc] initWithURL:[NSURL fileURLWithPath:path] options:@{}];
  if (![handler performRequests:@[ request ] error:&error]) {
    return CKFail(error.localizedDescription ?: @"Vision OCR failed", 1);
  }

  NSMutableArray<NSString *> *lines = [NSMutableArray array];
  for (VNRecognizedTextObservation *observation in request.results) {
    VNRecognizedText *candidate = [observation topCandidates:1].firstObject;
    if (candidate.string.length > 0) {
      [lines addObject:candidate.string];
    }
  }

  if (lines.count == 0) {
    return CKFail(@"No text was recognized", 1);
  }

  NSString *text = [[lines componentsJoinedByString:@"\n"] stringByAppendingString:@"\n"];
  NSData *output = [text dataUsingEncoding:NSUTF8StringEncoding];
  [[NSFileHandle fileHandleWithStandardOutput] writeData:output];
  return 0;
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc == 2) {
      NSString *path = [[NSFileManager defaultManager]
          stringWithFileSystemRepresentation:argv[1]
                                      length:strlen(argv[1])];
      return CKRunPlainText(path);
    }

    if (argc == 4 && strcmp(argv[1], "--searchable-pdf") == 0) {
      NSString *inputPath = [[NSFileManager defaultManager]
          stringWithFileSystemRepresentation:argv[2]
                                      length:strlen(argv[2])];
      NSString *outputPath = [[NSFileManager defaultManager]
          stringWithFileSystemRepresentation:argv[3]
                                      length:strlen(argv[3])];
      NSError *error = nil;
      if (!CKWriteSearchablePDF(inputPath, outputPath, &error)) {
        return CKFail(error.localizedDescription ?: @"Searchable PDF generation failed", 1);
      }
      return 0;
    }

    return CKFail(@"Usage: convertkit-vision-ocr <image>\n       "
                  @"convertkit-vision-ocr --searchable-pdf <input> <working-output>",
                  2);
  }
}
