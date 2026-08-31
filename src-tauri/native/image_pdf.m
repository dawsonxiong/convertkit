#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>
#import <math.h>

static int fail(NSString *message, int code) {
  const char *bytes = message.UTF8String;
  if (bytes != NULL) {
    fwrite(bytes, 1, strlen(bytes), stderr);
    fwrite("\n", 1, 1, stderr);
  }
  return code;
}

static void removeOutput(NSString *path) {
  if (path != nil) {
    [[NSFileManager defaultManager] removeItemAtPath:path error:nil];
  }
}

int main(int argc, const char *argv[]) {
  @autoreleasepool {
    if (argc != 3) {
      return fail(@"Usage: convertkit-image-pdf <input-image> <output-pdf>", 2);
    }

    NSFileManager *files = [NSFileManager defaultManager];
    NSString *inputPath = [files stringWithFileSystemRepresentation:argv[1]
                                                       length:strlen(argv[1])];
    NSString *outputPath = [files stringWithFileSystemRepresentation:argv[2]
                                                        length:strlen(argv[2])];
    BOOL isDirectory = NO;
    if (inputPath == nil ||
        ![files fileExistsAtPath:inputPath isDirectory:&isDirectory] ||
        isDirectory) {
      return fail(@"The source image is unavailable.", 3);
    }
    if (outputPath == nil || [inputPath isEqualToString:outputPath]) {
      return fail(@"Choose a different PDF output path.", 4);
    }

    NSURL *inputURL = [NSURL fileURLWithPath:inputPath isDirectory:NO];
    CGImageSourceRef source =
        CGImageSourceCreateWithURL((__bridge CFURLRef)inputURL, NULL);
    if (source == NULL) {
      return fail(@"This image cannot be decoded by macOS.", 5);
    }

    CFStringRef sourceType = CGImageSourceGetType(source);
    BOOL supportedType = sourceType != NULL &&
        (CFEqual(sourceType, CFSTR("public.png")) ||
         CFEqual(sourceType, CFSTR("public.jpeg")));
    if (!supportedType) {
      CFRelease(source);
      return fail(@"Image to PDF currently supports PNG and JPEG images.", 6);
    }

    size_t frameCount = CGImageSourceGetCount(source);
    if (frameCount != 1) {
      CFRelease(source);
      return fail(@"Animated and multi-page images cannot be converted to a single-page PDF.", 7);
    }

    NSDictionary *properties = CFBridgingRelease(
        CGImageSourceCopyPropertiesAtIndex(source, 0, NULL));
    NSNumber *pixelWidth = properties[(__bridge NSString *)kCGImagePropertyPixelWidth];
    NSNumber *pixelHeight = properties[(__bridge NSString *)kCGImagePropertyPixelHeight];
    NSUInteger maxPixelSize = MAX(pixelWidth.unsignedIntegerValue,
                                  pixelHeight.unsignedIntegerValue);
    if (maxPixelSize == 0) {
      CFRelease(source);
      return fail(@"The source image has invalid dimensions.", 8);
    }

    // ImageIO applies EXIF orientation while decoding. Capping pathological
    // dimensions keeps malformed inputs from forcing an unbounded allocation.
    NSDictionary *thumbnailOptions = @{
      (__bridge NSString *)kCGImageSourceCreateThumbnailFromImageAlways : @YES,
      (__bridge NSString *)kCGImageSourceCreateThumbnailWithTransform : @YES,
      (__bridge NSString *)kCGImageSourceThumbnailMaxPixelSize :
          @(MIN(maxPixelSize, (NSUInteger)32768))
    };
    CGImageRef image = CGImageSourceCreateThumbnailAtIndex(
        source, 0, (__bridge CFDictionaryRef)thumbnailOptions);
    CFRelease(source);
    if (image == NULL) {
      return fail(@"This image cannot be rendered by macOS.", 9);
    }

    size_t width = CGImageGetWidth(image);
    size_t height = CGImageGetHeight(image);
    if (width == 0 || height == 0) {
      CGImageRelease(image);
      return fail(@"The rendered image has invalid dimensions.", 10);
    }

    // Quartz PDF pages use points. Preserve the image aspect ratio and keep
    // the page within the broadly interoperable 200-inch PDF page limit.
    const CGFloat maximumPageEdge = 14400.0;
    CGFloat scale = MIN(1.0, maximumPageEdge / MAX((CGFloat)width, (CGFloat)height));
    CGRect page = CGRectMake(0, 0, MAX(1.0, width * scale),
                             MAX(1.0, height * scale));

    removeOutput(outputPath);
    NSURL *outputURL = [NSURL fileURLWithPath:outputPath isDirectory:NO];
    CGContextRef context = CGPDFContextCreateWithURL(
        (__bridge CFURLRef)outputURL, &page, NULL);
    if (context == NULL) {
      CGImageRelease(image);
      removeOutput(outputPath);
      return fail(@"The PDF output could not be created.", 11);
    }

    CGPDFContextBeginPage(context, NULL);
    CGContextDrawImage(context, page, image);
    CGPDFContextEndPage(context);
    CGPDFContextClose(context);
    CGContextRelease(context);
    CGImageRelease(image);

    CGPDFDocumentRef document =
        CGPDFDocumentCreateWithURL((__bridge CFURLRef)outputURL);
    BOOL valid = document != NULL && CGPDFDocumentGetNumberOfPages(document) == 1;
    if (valid) {
      CGPDFPageRef firstPage = CGPDFDocumentGetPage(document, 1);
      if (firstPage == NULL) {
        valid = NO;
      } else {
        CGRect writtenPage = CGPDFPageGetBoxRect(firstPage, kCGPDFMediaBox);
        valid = writtenPage.size.width > 0 && writtenPage.size.height > 0 &&
            fabs(writtenPage.size.width - page.size.width) < 0.1 &&
            fabs(writtenPage.size.height - page.size.height) < 0.1;
      }
    }
    if (document != NULL) {
      CGPDFDocumentRelease(document);
    }
    if (!valid) {
      removeOutput(outputPath);
      return fail(@"The generated PDF did not pass validation.", 12);
    }

    NSDictionary *attributes = [files attributesOfItemAtPath:outputPath error:nil];
    if ([attributes fileSize] == 0) {
      removeOutput(outputPath);
      return fail(@"The generated PDF is empty.", 13);
    }
    return 0;
  }
}
