import Observation
import SwiftUI
import UniformTypeIdentifiers

enum ImagePasteError: LocalizedError {
    case tooManyImages, unreadableImage
    var errorDescription: String? {
        switch self {
        case .tooManyImages: "Paste up to 30 images at a time."
        case .unreadableImage: "An image on the clipboard couldn’t be opened. Try copying the image again."
        }
    }
}

@MainActor
enum ClipboardImages {
    #if DEBUG
    static func prepareDemoClipboard() {
        let arguments = ProcessInfo.processInfo.arguments
        if arguments.contains("-demoClipboardText") {
            #if os(iOS)
            UIPasteboard.general.string = "Copied text\nsecond line"
            #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString("Copied text\nsecond line", forType: .string)
            #endif
        } else if arguments.contains("-demoClipboardImage") {
            let images = [CGSize(width: 640, height: 420), CGSize(width: 420, height: 640)]
                .compactMap { DemoImageFactory.landscape(size: $0).platformJPEGData(compressionQuality: 0.9) }
            #if os(iOS)
            UIPasteboard.general.items = images.map { [UTType.jpeg.identifier: $0, UTType.utf8PlainText.identifier: "https://example.com/photo.jpg"] }
            #else
            let items = images.map { data in
                let item = NSPasteboardItem()
                item.setData(data, forType: NSPasteboard.PasteboardType(UTType.jpeg.identifier))
                item.setString("https://example.com/photo.jpg", forType: .string)
                return item
            }
            NSPasteboard.general.clearContents()
            NSPasteboard.general.writeObjects(items)
            #endif
        }
    }
    #endif

    static func imageProviders(in providers: [NSItemProvider]) -> [NSItemProvider] {
        providers.filter { $0.hasItemConformingToTypeIdentifier(UTType.image.identifier) }
    }

    static func load(_ providers: [NSItemProvider]) async throws -> [PendingPhoto] {
        let providers = imageProviders(in: providers)
        guard providers.count <= 30 else { throw ImagePasteError.tooManyImages }
        var photos: [PendingPhoto] = []
        for provider in providers {
            try Task.checkCancellation()
            // Each provider is one image. Alternate PNG/JPEG/TIFF representations
            // must not turn a single copied image into several attachments.
            let types = provider.registeredTypeIdentifiers.filter { UTType($0)?.conforms(to: .image) == true }
            var photo: PendingPhoto?
            for identifier in types {
                let loaded: Data? = try? await withCheckedThrowingContinuation { continuation in
                    provider.loadDataRepresentation(forTypeIdentifier: identifier) { data, error in
                        if let data { continuation.resume(returning: data) }
                        else { continuation.resume(throwing: error ?? ImagePasteError.unreadableImage) }
                    }
                }
                guard let data = loaded,
                      !data.isEmpty, data.count <= 64 * 1_024 * 1_024,
                      let image = PlatformImage(data: data) else { continue }
                try Task.checkCancellation()
                photo = PendingPhoto(data: data, mimeType: UTType(identifier)?.preferredMIMEType ?? "application/octet-stream", image: image)
                break
            }
            guard let photo else { throw ImagePasteError.unreadableImage }
            photos.append(photo)
        }
        return photos
    }

    #if os(macOS)
    static func providers(from pasteboard: NSPasteboard) -> [NSItemProvider] {
        // Reading NSURL objects asks AppKit for the clipboard's file access grant.
        let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: [.urlReadingFileURLsOnly: true]) as? [URL] ?? []
        return (pasteboard.pasteboardItems ?? []).compactMap { item in
            let types = item.types.filter { UTType($0.rawValue)?.conforms(to: .image) == true }
            if !types.isEmpty {
                let provider = NSItemProvider()
                for type in types {
                    if let data = item.data(forType: type) {
                        provider.registerDataRepresentation(forTypeIdentifier: type.rawValue, visibility: .all) { completion in
                            completion(data, nil)
                            return nil
                        }
                    }
                }
                return provider
            }
            if let value = item.string(forType: .fileURL),
               let url = urls.first(where: { $0.absoluteString == value }),
               UTType(filenameExtension: url.pathExtension)?.conforms(to: .image) == true {
                return NSItemProvider(contentsOf: url)
            }
            return nil
        }
    }
    #endif
}

struct PastedImageSelection: Identifiable {
    let id = UUID()
    let photos: [PendingPhoto]
    let reply: MessageReplyTarget?
    let destination: String?
    let send: ([OutgoingImage], String?) -> Bool
}

@MainActor @Observable
final class ImagePasteController {
    var selection: PastedImageSelection?
    var error: String?
    private(set) var isLoading = false
    private var task: Task<Void, Never>?
    private var requestID: UUID?

    func begin(_ providers: [NSItemProvider], reply: MessageReplyTarget?, destination: String? = nil,
               send: @escaping ([OutgoingImage], String?) -> Bool) {
        cancel()
        guard !ClipboardImages.imageProviders(in: providers).isEmpty else { return }
        let id = UUID()
        requestID = id
        isLoading = true
        task = Task {
            do {
                let photos = try await ClipboardImages.load(providers)
                try Task.checkCancellation()
                guard requestID == id else { return }
                selection = PastedImageSelection(photos: photos, reply: reply, destination: destination, send: send)
            } catch {
                if requestID == id && !Task.isCancelled { self.error = error.localizedDescription }
            }
            if requestID == id { isLoading = false; task = nil }
        }
    }

    func cancel() {
        task?.cancel()
        task = nil
        requestID = nil
        isLoading = false
        selection = nil
        error = nil
    }
}

/// Native editing keeps selection, undo, text paste and keyboard shortcuts intact.
/// Clipboard contents are read only when the user invokes Paste.
struct MessageComposerTextInput: View {
    @Binding var text: String
    @Binding var focus: Bool
    var allowsImagePaste = true
    let pasteImages: ([NSItemProvider]) -> Void

    var body: some View {
        ZStack(alignment: .topLeading) {
            NativeMessageTextInput(text: $text,
                                   focused: $focus,
                                   allowsImagePaste: allowsImagePaste, pasteImages: pasteImages)
            if text.isEmpty {
                Text("Message").font(.body).foregroundStyle(.tertiary)
                    .allowsHitTesting(false).accessibilityHidden(true)
            }
        }
    }
}

#if os(iOS)
final class ImagePasteTextView: UITextView {
    var allowsImagePaste = true
    var pasteImages: ([NSItemProvider]) -> Void = { _ in }
    var imagePasteboard: UIPasteboard = .general

    override func canPerformAction(_ action: Selector, withSender sender: Any?) -> Bool {
        if action == #selector(paste(_:)), allowsImagePaste, imagePasteboard.hasImages { return true }
        return super.canPerformAction(action, withSender: sender)
    }

    override func paste(_ sender: Any?) {
        let providers = ClipboardImages.imageProviders(in: imagePasteboard.itemProviders)
        if !providers.isEmpty {
            if allowsImagePaste { pasteImages(providers) }
            return
        }
        super.paste(sender)
    }
}

private struct NativeMessageTextInput: UIViewRepresentable {
    @Binding var text: String
    @Binding var focused: Bool
    @Environment(\.isEnabled) private var enabled
    let allowsImagePaste: Bool
    let pasteImages: ([NSItemProvider]) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeUIView(context: Context) -> ImagePasteTextView {
        let view = ImagePasteTextView()
        view.delegate = context.coordinator
        view.font = .preferredFont(forTextStyle: .body)
        view.adjustsFontForContentSizeCategory = true
        view.backgroundColor = .clear
        view.textColor = .label
        view.textContainerInset = .zero
        view.textContainer.lineFragmentPadding = 0
        view.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        view.accessibilityLabel = "Message"
        view.accessibilityIdentifier = "message-composer-input"
        return view
    }
    func updateUIView(_ view: ImagePasteTextView, context: Context) {
        context.coordinator.parent = self
        if view.text != text { view.text = text }
        if view.isEditable != enabled { view.isEditable = enabled }
        view.allowsImagePaste = enabled && allowsImagePaste
        view.pasteImages = pasteImages
        if focused && enabled && !view.isFirstResponder { view.becomeFirstResponder() }
        else if (!focused || !enabled) && view.isFirstResponder { view.resignFirstResponder() }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, uiView view: ImagePasteTextView, context: Context) -> CGSize? {
        guard let width = proposal.width, width > 0 else { return nil }
        let line = view.font?.lineHeight ?? 21
        let height = view.sizeThatFits(CGSize(width: width, height: .greatestFiniteMagnitude)).height
        return CGSize(width: width, height: min(max(line, height), line * 6))
    }
    final class Coordinator: NSObject, UITextViewDelegate {
        var parent: NativeMessageTextInput
        init(_ parent: NativeMessageTextInput) { self.parent = parent }
        func textViewDidChange(_ textView: UITextView) { parent.text = textView.text }
        func textViewDidBeginEditing(_ textView: UITextView) { parent.focused = true }
        func textViewDidEndEditing(_ textView: UITextView) { parent.focused = false }
    }
}
#else
final class ImagePasteTextView: NSTextView {
    var allowsImagePaste = true
    var pasteImages: ([NSItemProvider]) -> Void = { _ in }
    var focusChanged: (Bool) -> Void = { _ in }
    // Injectable for tests so they never replace the user's clipboard.
    var imagePasteboard: NSPasteboard = .general

    override func becomeFirstResponder() -> Bool {
        let accepted = super.becomeFirstResponder()
        if accepted { focusChanged(true) }
        return accepted
    }
    override func resignFirstResponder() -> Bool {
        let accepted = super.resignFirstResponder()
        if accepted { focusChanged(false) }
        return accepted
    }

    override var readablePasteboardTypes: [NSPasteboard.PasteboardType] {
        super.readablePasteboardTypes + NSImage.imageTypes.map { NSPasteboard.PasteboardType($0) } + [.fileURL]
    }
    override func paste(_ sender: Any?) {
        let providers = ClipboardImages.providers(from: imagePasteboard)
        if !providers.isEmpty {
            if allowsImagePaste { pasteImages(providers) }
            return
        }
        super.paste(sender)
    }
    override func validateUserInterfaceItem(_ item: NSValidatedUserInterfaceItem) -> Bool {
        if item.action == #selector(paste(_:)), allowsImagePaste,
           (imagePasteboard.types?.contains { UTType($0.rawValue)?.conforms(to: .image) == true } == true
            || imagePasteboard.canReadObject(forClasses: [NSImage.self], options: nil)) { return true }
        return super.validateUserInterfaceItem(item)
    }
}

private struct NativeMessageTextInput: NSViewRepresentable {
    @Binding var text: String
    @Binding var focused: Bool
    @Environment(\.isEnabled) private var enabled
    let allowsImagePaste: Bool
    let pasteImages: ([NSItemProvider]) -> Void
    func makeCoordinator() -> Coordinator { Coordinator(self) }
    func makeNSView(context: Context) -> NSScrollView {
        let scroll = NSScrollView()
        scroll.drawsBackground = false
        scroll.hasVerticalScroller = true
        scroll.autohidesScrollers = true
        let view = ImagePasteTextView(frame: .zero)
        view.isRichText = false
        view.allowsUndo = true
        view.isVerticallyResizable = true
        view.isHorizontallyResizable = false
        view.autoresizingMask = [.width]
        view.textContainer?.widthTracksTextView = true
        view.textContainer?.containerSize = NSSize(width: 0, height: CGFloat.greatestFiniteMagnitude)
        view.textContainer?.lineFragmentPadding = 0
        view.textContainerInset = .zero
        view.drawsBackground = false
        view.font = .preferredFont(forTextStyle: .body)
        view.textColor = .labelColor
        view.insertionPointColor = .labelColor
        view.delegate = context.coordinator
        view.setAccessibilityLabel("Message")
        view.setAccessibilityIdentifier("message-composer-input")
        scroll.documentView = view
        return scroll
    }
    func updateNSView(_ scroll: NSScrollView, context: Context) {
        guard let view = scroll.documentView as? ImagePasteTextView else { return }
        context.coordinator.parent = self
        if view.string != text { view.string = text }
        if view.isEditable != enabled { view.isEditable = enabled }
        view.allowsImagePaste = enabled && allowsImagePaste
        view.pasteImages = pasteImages
        view.focusChanged = { value in if focused != value { focused = value } }
        if focused && enabled && view.window?.firstResponder !== view { view.window?.makeFirstResponder(view) }
        else if (!focused || !enabled) && view.window?.firstResponder === view { view.window?.makeFirstResponder(nil) }
    }
    func sizeThatFits(_ proposal: ProposedViewSize, nsView: NSScrollView, context: Context) -> CGSize? {
        guard let width = proposal.width, width > 0 else { return nil }
        let font = NSFont.preferredFont(forTextStyle: .body)
        let line = ceil(font.ascender - font.descender + font.leading)
        let size = (text.isEmpty ? " " : text + " ").boundingRect(with: NSSize(width: width, height: .greatestFiniteMagnitude), options: [.usesLineFragmentOrigin, .usesFontLeading], attributes: [.font: font]).size
        return CGSize(width: width, height: min(max(line, ceil(size.height)), line * 6) + 2)
    }
    final class Coordinator: NSObject, NSTextViewDelegate {
        var parent: NativeMessageTextInput
        init(_ parent: NativeMessageTextInput) { self.parent = parent }
        func textDidChange(_ notification: Notification) {
            if let view = notification.object as? NSTextView { parent.text = view.string }
        }
    }
}
#endif
