import CoreGraphics
import SwiftUI

#if canImport(UIKit)
import UIKit

typealias PlatformImage = UIImage
#elseif canImport(AppKit)
import AppKit

typealias PlatformImage = NSImage
#endif

extension Image {
    init(platformImage: PlatformImage) {
        #if canImport(UIKit)
        self.init(uiImage: platformImage)
        #elseif canImport(AppKit)
        self.init(nsImage: platformImage)
        #endif
    }
}

extension PlatformImage {
    func platformJPEGData(compressionQuality: CGFloat) -> Data? {
        #if canImport(UIKit)
        jpegData(compressionQuality: compressionQuality)
        #elseif canImport(AppKit)
        guard let tiffRepresentation,
              let representation = NSBitmapImageRep(data: tiffRepresentation) else { return nil }
        return representation.representation(
            using: .jpeg,
            properties: [.compressionFactor: compressionQuality]
        )
        #endif
    }

    func platformResizedForUpload(scale: CGFloat) -> PlatformImage? {
        guard scale > 0, scale < 1 else { return self }
        let targetSize = CGSize(
            width: max(1, size.width * scale),
            height: max(1, size.height * scale)
        )

        #if canImport(UIKit)
        let format = UIGraphicsImageRendererFormat.default()
        format.scale = 1
        format.opaque = true
        return UIGraphicsImageRenderer(size: targetSize, format: format).image { context in
            UIColor.white.setFill()
            context.fill(CGRect(origin: .zero, size: targetSize))
            draw(in: CGRect(origin: .zero, size: targetSize))
        }
        #elseif canImport(AppKit)
        let resized = NSImage(size: targetSize)
        resized.lockFocus()
        NSColor.white.setFill()
        NSRect(origin: .zero, size: targetSize).fill()
        draw(
            in: NSRect(origin: .zero, size: targetSize),
            from: NSRect(origin: .zero, size: size),
            operation: .copy,
            fraction: 1
        )
        resized.unlockFocus()
        return resized
        #endif
    }
}

enum PhotoUploadPreparer {
    private static let megabyte = 1_024 * 1_024
    private static let singlePhotoMaximumBytes = 15 * megabyte
    private static let albumMaximumBytes = 60 * megabyte
    private static let passthroughMimeTypes = Set(["image/jpeg", "image/png", "image/gif", "image/webp"])
    private static let jpegQualities: [CGFloat] = [0.88, 0.72, 0.56, 0.4, 0.28]

    static func maximumBytes(forPhotoCount count: Int) -> Int {
        let safeCount = max(1, count)
        return min(singlePhotoMaximumBytes, albumMaximumBytes / safeCount)
    }

    static func prepare(
        data: Data,
        mimeType: String,
        image: PlatformImage,
        maximumBytes: Int
    ) -> OutgoingImage? {
        guard !data.isEmpty, maximumBytes > 0 else { return nil }
        if passthroughMimeTypes.contains(mimeType), data.count <= maximumBytes {
            return OutgoingImage(data: data, mimeType: mimeType)
        }

        var candidate = image
        for _ in 0..<6 {
            var smallestData: Data?
            for quality in jpegQualities {
                guard let encoded = candidate.platformJPEGData(compressionQuality: quality) else { continue }
                smallestData = encoded
                if encoded.count <= maximumBytes {
                    return OutgoingImage(data: encoded, mimeType: "image/jpeg")
                }
            }

            guard let smallestData else { return nil }
            let estimatedScale = sqrt(CGFloat(maximumBytes) / CGFloat(smallestData.count)) * 0.9
            let scale = min(0.82, max(0.35, estimatedScale))
            guard let resized = candidate.platformResizedForUpload(scale: scale) else { return nil }
            candidate = resized
        }
        return nil
    }
}

enum DemoImageFactory {
    static func landscape(size: CGSize) -> PlatformImage {
        let width = Int(size.width)
        let height = Int(size.height)
        let colorSpace = CGColorSpaceCreateDeviceRGB()
        guard let context = CGContext(
            data: nil,
            width: width,
            height: height,
            bitsPerComponent: 8,
            bytesPerRow: 0,
            space: colorSpace,
            bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue
        ) else {
            return emptyImage(size: size)
        }

        context.setFillColor(CGColor(red: 0.57, green: 0.83, blue: 0.93, alpha: 1))
        context.fill(CGRect(origin: .zero, size: size))
        context.setFillColor(CGColor(red: 1, green: 0.84, blue: 0.38, alpha: 1))
        context.fillEllipse(in: CGRect(
            x: size.width * 0.75,
            y: size.height * 0.67,
            width: size.width * 0.134,
            height: size.width * 0.134
        ))

        let hills = CGMutablePath()
        hills.move(to: CGPoint(x: 0, y: size.height * 0.21))
        hills.addCurve(
            to: CGPoint(x: size.width, y: size.height * 0.35),
            control1: CGPoint(x: size.width * 0.23, y: size.height * 0.48),
            control2: CGPoint(x: size.width * 0.66, y: size.height * 0.07)
        )
        hills.addLine(to: CGPoint(x: size.width, y: 0))
        hills.addLine(to: .zero)
        hills.closeSubpath()
        context.addPath(hills)
        context.setFillColor(CGColor(red: 0.24, green: 0.57, blue: 0.36, alpha: 1))
        context.fillPath()
        context.setFillColor(CGColor(red: 0.13, green: 0.42, blue: 0.28, alpha: 1))
        context.fill(CGRect(x: 0, y: 0, width: size.width, height: size.height * 0.17))

        guard let cgImage = context.makeImage() else { return emptyImage(size: size) }
        #if canImport(UIKit)
        return UIImage(cgImage: cgImage)
        #elseif canImport(AppKit)
        return NSImage(cgImage: cgImage, size: size)
        #endif
    }

    private static func emptyImage(size: CGSize) -> PlatformImage {
        #if canImport(UIKit)
        return UIImage()
        #elseif canImport(AppKit)
        return NSImage(size: size)
        #endif
    }
}

extension View {
    @ViewBuilder
    func platformInlineNavigationTitle() -> some View {
        #if os(iOS)
        navigationBarTitleDisplayMode(.inline)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformDismissesKeyboard() -> some View {
        #if os(iOS)
        scrollDismissesKeyboard(.interactively)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformInteractiveDismissDisabled(_ disabled: Bool) -> some View {
        #if os(iOS)
        interactiveDismissDisabled(disabled)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformUncapitalizedInput() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformWordsInput() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.words)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformURLInput() -> some View {
        #if os(iOS)
        textInputAutocapitalization(.never)
            .keyboardType(.URL)
            .textContentType(.URL)
            .submitLabel(.next)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformPasswordInput() -> some View {
        #if os(iOS)
        textContentType(.password)
            .submitLabel(.go)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformCompactControlTypography() -> some View {
        #if os(iOS)
        dynamicTypeSize(...DynamicTypeSize.xxxLarge)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformSheetSize(minWidth: CGFloat, minHeight: CGFloat) -> some View {
        #if os(macOS)
        frame(width: minWidth, height: minHeight)
        #else
        self
        #endif
    }

    @ViewBuilder
    func platformGroupedFormStyle() -> some View {
        #if os(macOS)
        formStyle(.grouped)
        #else
        self
        #endif
    }
}

enum NativeLayoutPolicy {
    static func usesStackedChatRow(for dynamicTypeSize: DynamicTypeSize) -> Bool {
        dynamicTypeSize.isAccessibilitySize
    }

    static func usesCompactMessageChrome(for dynamicTypeSize: DynamicTypeSize) -> Bool {
        dynamicTypeSize.isAccessibilitySize
    }
}

extension Color {
    static var platformPrimaryLabel: Color {
        #if canImport(UIKit)
        Color(uiColor: .label)
        #else
        Color(nsColor: .labelColor)
        #endif
    }

    static var platformSecondaryLabel: Color {
        #if canImport(UIKit)
        Color(uiColor: .secondaryLabel)
        #else
        Color(nsColor: .secondaryLabelColor)
        #endif
    }
}

var platformTrailingToolbarPlacement: ToolbarItemPlacement {
    #if os(macOS)
    .primaryAction
    #else
    .topBarTrailing
    #endif
}

#if os(macOS)
enum MacChatLayoutMetrics {
    static let minimumWindowWidth: CGFloat = 980
    static let minimumWindowHeight: CGFloat = 620
    static let defaultWindowWidth: CGFloat = 1_260
    static let defaultWindowHeight: CGFloat = 800
    static let minimumSidebarWidth: CGFloat = 300
    static let idealSidebarWidth: CGFloat = 330
    static let maximumSidebarWidth: CGFloat = 380
    static let maximumBubbleWidth: CGFloat = 620
    static let timelineHorizontalPadding: CGFloat = 32
    static let settingsSheetMinimumWidth: CGFloat = 560
    static let settingsSheetMinimumHeight: CGFloat = 500
    static let costSheetMinimumWidth: CGFloat = 420
    static let costSheetMinimumHeight: CGFloat = 320
    static let mediaSheetMinimumWidth: CGFloat = 560
    static let mediaSheetMinimumHeight: CGFloat = 520
}
#endif
