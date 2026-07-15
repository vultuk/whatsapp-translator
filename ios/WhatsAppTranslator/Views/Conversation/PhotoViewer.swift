import SwiftUI

enum PhotoViewerZoom {
    static let minimumScale: CGFloat = 1
    static let maximumScale: CGFloat = 5
    static let doubleTapScale: CGFloat = 2.5

    static func clampedScale(_ scale: CGFloat) -> CGFloat {
        min(maximumScale, max(minimumScale, scale))
    }

    static func toggledScale(from scale: CGFloat) -> CGFloat {
        scale > minimumScale ? minimumScale : doubleTapScale
    }
}

struct PhotoViewer: View {
    let image: PlatformImage
    let close: () -> Void

    @State private var scale = PhotoViewerZoom.minimumScale
    @State private var settledScale = PhotoViewerZoom.minimumScale
    @State private var offset = CGSize.zero
    @State private var settledOffset = CGSize.zero

    var body: some View {
        GeometryReader { proxy in
            ZStack {
                Color.black.ignoresSafeArea()

                Image(platformImage: image)
                    .resizable()
                    .scaledToFit()
                    .frame(
                        maxWidth: max(0, proxy.size.width),
                        maxHeight: max(0, proxy.size.height)
                    )
                    .scaleEffect(scale)
                    .offset(offset)
                    .contentShape(Rectangle())
                    .gesture(zoomAndPanGesture)
                    .onTapGesture(count: 2, perform: toggleZoom)
                    .accessibilityLabel("Full-screen photo")
                    .accessibilityHint("Pinch to zoom and drag to move around the photo")

                viewerControls
            }
        }
        .preferredColorScheme(.dark)
        .photoViewerExitCommand(close)
    }

    private var zoomAndPanGesture: some Gesture {
        SimultaneousGesture(
            MagnifyGesture()
                .onChanged { value in
                    scale = PhotoViewerZoom.clampedScale(settledScale * value.magnification)
                }
                .onEnded { _ in
                    settleZoom()
                },
            DragGesture(minimumDistance: 4)
                .onChanged { value in
                    guard scale > PhotoViewerZoom.minimumScale else { return }
                    offset = CGSize(
                        width: settledOffset.width + value.translation.width,
                        height: settledOffset.height + value.translation.height
                    )
                }
                .onEnded { _ in
                    settledOffset = offset
                }
        )
    }

    private var viewerControls: some View {
        VStack {
            HStack(spacing: 10) {
                Button("Close", systemImage: "xmark") { close() }
                    .labelStyle(.iconOnly)
                    .keyboardShortcut(.cancelAction)
                    .accessibilityLabel("Close photo")

                Spacer()

                Button("Zoom out", systemImage: "minus.magnifyingglass") {
                    setScale(scale - 0.5)
                }
                .labelStyle(.iconOnly)
                .disabled(scale <= PhotoViewerZoom.minimumScale)

                Text("\(Int((scale * 100).rounded()))%")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .frame(minWidth: 48)

                Button("Actual size", systemImage: "1.magnifyingglass") { resetZoom() }
                    .labelStyle(.iconOnly)
                    .disabled(scale == PhotoViewerZoom.minimumScale && offset == .zero)

                Button("Zoom in", systemImage: "plus.magnifyingglass") {
                    setScale(scale + 0.5)
                }
                .labelStyle(.iconOnly)
                .disabled(scale >= PhotoViewerZoom.maximumScale)
            }
            .buttonStyle(.bordered)
            .buttonBorderShape(.circle)
            .foregroundStyle(.white)
            .padding(16)
            .background(.black.opacity(0.52))

            Spacer()

            Text(viewerInstructions)
                .font(.caption)
                .foregroundStyle(.white.opacity(0.82))
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.black.opacity(0.58), in: Capsule())
                .padding(.bottom, 18)
                .allowsHitTesting(false)
        }
    }

    private var viewerInstructions: String {
        #if os(macOS)
        "Pinch to zoom • drag to move • double-click to toggle zoom"
        #else
        "Pinch to zoom • drag to move • double-tap to toggle zoom"
        #endif
    }

    private func toggleZoom() {
        setScale(PhotoViewerZoom.toggledScale(from: scale))
    }

    private func setScale(_ newScale: CGFloat) {
        withAnimation(.snappy) {
            scale = PhotoViewerZoom.clampedScale(newScale)
            settleZoom()
        }
    }

    private func settleZoom() {
        scale = PhotoViewerZoom.clampedScale(scale)
        settledScale = scale
        if scale == PhotoViewerZoom.minimumScale {
            offset = .zero
            settledOffset = .zero
        }
    }

    private func resetZoom() {
        withAnimation(.snappy) {
            scale = PhotoViewerZoom.minimumScale
            settledScale = PhotoViewerZoom.minimumScale
            offset = .zero
            settledOffset = .zero
        }
    }
}

private struct PhotoViewerPresentationModifier: ViewModifier {
    @Binding var isPresented: Bool
    let image: PlatformImage?

    func body(content: Content) -> some View {
        #if os(macOS)
        content.sheet(isPresented: $isPresented) {
            if let image {
                PhotoViewer(image: image) { isPresented = false }
                    .frame(minWidth: 900, minHeight: 650)
            }
        }
        #else
        content.fullScreenCover(isPresented: $isPresented) {
            if let image {
                PhotoViewer(image: image) { isPresented = false }
            }
        }
        #endif
    }
}

extension View {
    func photoViewer(isPresented: Binding<Bool>, image: PlatformImage?) -> some View {
        modifier(PhotoViewerPresentationModifier(isPresented: isPresented, image: image))
    }

    @ViewBuilder
    func photoViewerExitCommand(_ action: @escaping () -> Void) -> some View {
        #if os(macOS)
        onExitCommand(perform: action)
        #else
        self
        #endif
    }
}
