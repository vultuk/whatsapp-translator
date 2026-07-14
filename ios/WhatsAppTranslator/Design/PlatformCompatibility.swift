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
}
#endif
