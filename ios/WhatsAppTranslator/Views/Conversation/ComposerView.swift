import AVKit
import CoreTransferable
import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ComposerView: View {
    @Environment(\.translatorPalette) private var palette
    let contactID: String
    @Binding var text: String
    let reply: MessageReplyTarget?
    var showReplyPreview = true
    let isSending: Bool
    let cancelReply: () -> Void
    let sendImages: ([OutgoingImage], String?) -> Bool
    let send: () -> Void
    @State private var showVoiceComposer = false
    @State private var selectedPhotos: [PhotosPickerItem] = []
    @State private var pendingPhotos: PendingPhotoSelection?
    @State private var pickerError: String?
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            if let reply, showReplyPreview {
                HStack(spacing: 10) {
                    RoundedRectangle(cornerRadius: 2)
                        .fill(palette.accent)
                        .frame(width: 3, height: 34)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(reply.senderName)
                            .font(.caption.weight(.semibold))
                            .foregroundStyle(palette.deepAccent)
                        Text(reply.text)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Spacer()
                    Button("Cancel reply", systemImage: "xmark.circle.fill", action: cancelReply)
                        .labelStyle(.iconOnly)
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, 15)
                .padding(.vertical, 8)
                .translatorGlass(in: RoundedRectangle(cornerRadius: 20))
                .padding(.horizontal, 12)
            }

            ComposerGlassGroup {
                HStack(alignment: .center, spacing: 7) {
                    PhotosPicker(selection: $selectedPhotos, maxSelectionCount: 30, matching: .images) {
                        addImageLabel
                    }
                    .buttonStyle(.plain)
                    .disabled(isSending)
                    .accessibilityLabel("Send photos")
                    .translatorGlassControl(in: Circle())

                    composerInput

                    if text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSending {
                        Button { showVoiceComposer = true } label: {
                            Image(systemName: "mic.fill")
                                .font(.system(size: 23, weight: .regular))
                                .foregroundStyle(Color.primary)
                                .frame(width: 44, height: 44)
                                .translatorGlassControl(in: Circle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Record voice note")
                        .help("Record voice note")
                    } else {
                        sendButton
                    }
                }
                .padding(.horizontal, 12)
                .padding(.top, reply == nil || !showReplyPreview ? 8 : 2)
                .padding(.bottom, 9)
            }
        }
        .onChange(of: reply?.messageID) { _, value in focused = value != nil }
        .onChange(of: selectedPhotos) { _, items in
            guard !items.isEmpty else { return }
            Task {
                defer { selectedPhotos = [] }
                var prepared: [PendingPhoto] = []
                for item in items {
                    guard let photo = await loadPhoto(item) else {
                        pickerError = "One of the selected photos couldn’t be opened. Please choose the photos again."
                        return
                    }
                    prepared.append(photo)
                }
                pendingPhotos = PendingPhotoSelection(photos: prepared)
            }
        }
        .sheet(isPresented: $showVoiceComposer) {
            VoiceComposerView(contactID: contactID, reply: reply, onSent: cancelReply)
        }
        .sheet(item: $pendingPhotos) { selection in
            ImageComposerSheet(photos: selection.photos, reply: reply, send: sendImages)
        }
        .alert("Couldn’t prepare image", isPresented: pickerErrorPresented) {
            Button("OK") { pickerError = nil }
        } message: {
            Text(pickerError ?? "Please choose another image.")
        }
        .task {
            let arguments = ProcessInfo.processInfo.arguments
            guard arguments.contains("-demoImageComposer")
                    || arguments.contains("-demoImageAlbumComposer")
                    || arguments.contains("-demoOptimizedPhotoComposer"),
                  pendingPhotos == nil else { return }
            let count = arguments.contains("-demoImageAlbumComposer") ? 4 : 1
            let photos = (0..<count).compactMap { index -> PendingPhoto? in
                let image = DemoImageFactory.landscape(size: CGSize(width: 800 - index * 80, height: 600 + index * 40))
                guard let data = image.platformJPEGData(compressionQuality: 0.9) else { return nil }
                return PendingPhoto(data: data, mimeType: "image/jpeg", image: image)
            }
            pendingPhotos = PendingPhotoSelection(photos: photos)
        }
    }

    @ViewBuilder
    private var composerInput: some View {
        #if os(macOS)
        ZStack(alignment: .topLeading) {
            TextEditor(text: $text)
                .font(.body)
                .scrollContentBackground(.hidden)
                .frame(height: macEditorHeight)
                .focused($focused)

            if text.isEmpty {
                Text("Message")
                    .font(.body)
                    .foregroundStyle(.tertiary)
                    .padding(.leading, 5)
                    .padding(.top, 5)
                    .allowsHitTesting(false)
            }
        }
        .modifier(ComposerInputStyle())
        #else
        TextField("Message", text: $text, axis: .vertical)
            .lineLimit(1...6)
            .focused($focused)
            .modifier(ComposerInputStyle())
        #endif
    }

    private var macEditorHeight: CGFloat {
        let explicitLineCount = text.components(separatedBy: .newlines).count
        let visibleLineCount = min(max(explicitLineCount, 1), 6)
        return CGFloat(visibleLineCount * 19 + 5)
    }

    nonisolated private var addImageLabel: some View {
        Image(systemName: "plus")
            .font(.system(size: 23, weight: .regular))
            .foregroundStyle(Color.primary)
            .frame(width: 44, height: 44)
    }

    @ViewBuilder
    private var sendButton: some View {
        #if os(macOS)
        Button {
            guard !isSending else { return }
            send()
        } label: {
            sendButtonLabel
                .frame(width: 44, height: 44)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(.white)
        .frame(width: 44, height: 44)
        .translatorGlassControl(in: Circle(), tint: palette.accent)
        .opacity(isSendDisabled ? 0.42 : 1)
        .disabled(isSendDisabled)
        .allowsHitTesting(!isSending)
        .keyboardShortcut(.return, modifiers: .command)
        .accessibilityLabel(isSending ? "Sending message" : "Send message")
        .accessibilityValue(isSending ? "In progress" : "")
        .animation(.snappy, value: isSending)
        #else
        Button {
            guard !isSending else { return }
            send()
        } label: {
            sendButtonLabel
                .frame(width: 46, height: 46)
                .contentShape(Circle())
        }
        .buttonStyle(.plain)
        .foregroundStyle(isSendDisabled ? Color.platformSecondaryLabel : .white)
        .translatorGlassControl(in: Circle(), tint: isSendDisabled ? nil : palette.accent)
        .disabled(isSendDisabled)
        .allowsHitTesting(!isSending)
        .accessibilityLabel(isSending ? "Sending message" : "Send message")
        .accessibilityValue(isSending ? "In progress" : "")
        .animation(.snappy, value: isSending)
        #endif
    }

    @ViewBuilder
    private var sendButtonLabel: some View {
        if isSending {
            ProgressView()
                .controlSize(.small)
                .tint(.white)
                .transition(.scale.combined(with: .opacity))
        } else {
            Image(systemName: "paperplane.fill")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(isSendDisabled ? Color.platformSecondaryLabel : .white)
                .transition(.scale.combined(with: .opacity))
        }
    }

    private var isSendDisabled: Bool {
        text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSending
    }

    private var pickerErrorPresented: Binding<Bool> {
        Binding(get: { pickerError != nil }, set: { if !$0 { pickerError = nil } })
    }

    private func loadPhoto(_ item: PhotosPickerItem) async -> PendingPhoto? {
        guard let data = try? await item.loadTransferable(type: Data.self),
              let image = PlatformImage(data: data) else {
            return nil
        }
        let mimeType = item.supportedContentTypes.compactMap(\.preferredMIMEType).first ?? "application/octet-stream"
        return PendingPhoto(data: data, mimeType: mimeType, image: image)
    }
}

private struct ComposerInputStyle: ViewModifier {
    func body(content: Content) -> some View {
        content
            .textFieldStyle(.plain)
            .padding(.horizontal, 16)
            .padding(.vertical, 10)
            .frame(minHeight: 44)
            .translatorGlassControl(in: RoundedRectangle(cornerRadius: 24, style: .continuous))
    }
}

private struct PendingPhoto: Identifiable {
    let id = UUID()
    let data: Data
    let mimeType: String
    let image: PlatformImage
}

private struct PendingPhotoSelection: Identifiable {
    let id = UUID()
    let photos: [PendingPhoto]
}

private struct ImageComposerSheet: View {
    @Environment(\.dismiss) private var dismiss
    let photos: [PendingPhoto]
    let reply: MessageReplyTarget?
    var destination: String? = nil
    let send: ([OutgoingImage], String?) -> Bool
    @State private var caption = ""

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                photoPreview

                if photos.count > 1 {
                    Label("\(photos.count) photos selected", systemImage: "photo.stack.fill")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
                }

                if photos.count > 0 {
                    Label("Photos are optimized when you send", systemImage: "wand.and.sparkles")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }

                if let destination {
                    VStack(spacing: 5) {
                        Text("To: \(destination)").font(.headline)
                        if let reply { Text(reply.text).font(.caption).lineLimit(2).foregroundStyle(.secondary) }
                    }.padding(.horizontal)
                }

                if let reply {
                    Label("Replying to \(reply.senderName)", systemImage: "arrowshape.turn.up.left")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }

                TextField("Add a caption", text: $caption, axis: .vertical)
                    .lineLimit(1...4)
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)
                    .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
                    .padding(.horizontal)

                Spacer(minLength: 0)
            }
            .padding(.top)
            .navigationTitle(photos.count == 1 ? "Send photo" : "Send \(photos.count) photos")
            .platformInlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        let cleanCaption = caption.trimmingCharacters(in: .whitespacesAndNewlines)
                        let images = photos.map { OutgoingImage(data: $0.data, mimeType: $0.mimeType) }
                        if send(images, cleanCaption.isEmpty ? nil : cleanCaption) {
                            dismiss()
                        }
                    } label: {
                        Label("Send", systemImage: "paperplane.fill")
                    }
                    .fontWeight(.semibold)
                }
            }
        }
        #if os(macOS)
        .platformSheetSize(
            minWidth: MacChatLayoutMetrics.mediaSheetMinimumWidth,
            minHeight: MacChatLayoutMetrics.mediaSheetMinimumHeight
        )
        #endif
    }

    @ViewBuilder
    private var photoPreview: some View {
        if photos.count == 1, let photo = photos.first {
            Image(platformImage: photo.image)
                .resizable()
                .scaledToFit()
                .frame(maxWidth: .infinity, maxHeight: 440)
                .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                .padding(.horizontal)
        } else {
            ScrollView(.horizontal, showsIndicators: false) {
                LazyHStack(spacing: 10) {
                    ForEach(Array(photos.enumerated()), id: \.element.id) { index, photo in
                        ZStack(alignment: .topTrailing) {
                            Image(platformImage: photo.image)
                                .resizable()
                                .scaledToFill()
                                .frame(width: 150, height: 210)
                                .clipShape(RoundedRectangle(cornerRadius: 16, style: .continuous))
                            Text("\(index + 1)")
                                .font(.caption.bold())
                                .foregroundStyle(.white)
                                .frame(width: 26, height: 26)
                                .background(.black.opacity(0.62), in: Circle())
                                .padding(8)
                        }
                    }
                }
                .padding(.horizontal)
            }
            .frame(height: 210)
        }
    }
}

/// Captured before presenting any picker or recorder, never inferred again at send time.
struct UnifiedMediaContext {
    let message: ChatMessage
    let reply: MessageReplyTarget
    let destination: String
}

struct UnifiedMediaControls: View {
    @Environment(AppSession.self) private var session
    let disabled: Bool
    let begin: () -> UnifiedMediaContext?
    let onSent: (ChatMessage) -> Void
    @State private var context: UnifiedMediaContext?
    @State private var choices = false
    @State private var photosPresented = false
    @State private var videoPresented = false
    @State private var filesPresented = false
    @State private var voicePresented = false
    @State private var selectedPhotos: [PhotosPickerItem] = []
    @State private var selectedVideo: PhotosPickerItem?
    @State private var pendingPhotos: PendingPhotoSelection?
    @State private var pendingAttachment: PendingAttachment?
    @State private var loading = false
    @State private var error: String?

    private var unavailable: Bool {
        disabled || loading
    }

    var body: some View {
        HStack(spacing: 2) {
            Button {
                guard let captured = begin() else { return }
                context = captured
                choices = true
            } label: {
                Group {
                    if loading { ProgressView().controlSize(.small) }
                    else { Image(systemName: "plus").font(.system(size: 23)) }
                }.frame(width: 44, height: 46)
                    .translatorGlassControl(in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Add attachment")
            .help("Send photos, videos, files, or a voice note")
            .disabled(unavailable)

            Button {
                guard let captured = begin() else { return }
                context = captured
                voicePresented = true
            } label: {
                Image(systemName: "mic.fill").font(.system(size: 21))
                    .frame(width: 44, height: 46)
                    .translatorGlassControl(in: Circle())
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Record voice note")
            .disabled(unavailable)
        }
        #if os(macOS)
        .popover(isPresented: $choices, arrowEdge: .bottom) {
            VStack(alignment: .leading, spacing: 12) {
                Text(context.map { "Send to \($0.destination)" } ?? "Add attachment").font(.headline)
                Button("Photos", systemImage: "photo.on.rectangle") { choices = false; photosPresented = true }
                Button("Video", systemImage: "video") { choices = false; videoPresented = true }
                Button("File", systemImage: "doc") { choices = false; filesPresented = true }
                Button("Voice note", systemImage: "mic") { choices = false; voicePresented = true }
            }
            .buttonStyle(.borderless)
            .padding(18)
            .frame(minWidth: 210, alignment: .leading)
        }
        #else
        .confirmationDialog(context.map { "Send to \($0.destination)" } ?? "Add attachment", isPresented: $choices, titleVisibility: .visible) {
            Button("Photos") { photosPresented = true }
            Button("Video") { videoPresented = true }
            Button("File") { filesPresented = true }
            Button("Voice note") { voicePresented = true }
            Button("Cancel", role: .cancel) {}
        }
        #endif
        .photosPicker(isPresented: $photosPresented, selection: $selectedPhotos, maxSelectionCount: 30, matching: .images)
        .photosPicker(isPresented: $videoPresented, selection: $selectedVideo, matching: .videos)
        .fileImporter(isPresented: $filesPresented, allowedContentTypes: [.item]) { result in
            switch result {
            case .success(let url):
                prepareFile(url)
            case .failure(let failure): error = failure.localizedDescription
            }
        }
        .onChange(of: selectedPhotos) { _, items in
            guard !items.isEmpty, context != nil else { return }
            loading = true
            Task {
                defer { loading = false; selectedPhotos = [] }
                var photos: [PendingPhoto] = []
                for item in items {
                    guard let data = try? await item.loadTransferable(type: Data.self),
                          let image = PlatformImage(data: data) else {
                        error = "A photo couldn’t be opened. Please choose it again."
                        return
                    }
                    photos.append(PendingPhoto(data: data, mimeType: item.supportedContentTypes.compactMap(\.preferredMIMEType).first ?? "image/jpeg", image: image))
                }
                pendingPhotos = PendingPhotoSelection(photos: photos)
            }
        }
        .onChange(of: selectedVideo) { _, item in
            guard let item, context != nil else { return }
            loading = true
            Task {
                defer { loading = false; selectedVideo = nil }
                do {
                    guard let movie = try await item.loadTransferable(type: ImportedMovie.self) else {
                        throw APIError.server("The video couldn’t be opened.")
                    }
                    defer { try? FileManager.default.removeItem(at: movie.url) }
                    pendingAttachment = try await PendingAttachment.video(from: movie.url)
                } catch { self.error = error.localizedDescription }
            }
        }
        .sheet(item: $pendingPhotos) { selection in
            if let context {
                ImageComposerSheet(photos: selection.photos, reply: context.reply, destination: context.destination) { images, caption in
                    if session.startPhotoSend(images, caption: caption, to: context.message.contactId, reply: context.reply, replyOnlyIfNotLatest: true) {
                        onSent(context.message)
                        return true
                    }
                    return false
                }
            }
        }
        .sheet(item: $pendingAttachment, onDismiss: { pendingAttachment = nil }) { attachment in
            if let context {
                AttachmentComposerSheet(attachment: attachment, context: context) { onSent(context.message) }
            }
        }
        .sheet(isPresented: $voicePresented) {
            if let context {
                VoiceComposerView(contactID: context.message.contactId, reply: context.reply, replyOnlyIfNotLatest: true, destination: context.destination) { onSent(context.message) }
            }
        }
        .alert("Couldn’t prepare attachment", isPresented: Binding(get: { error != nil }, set: { if !$0 { error = nil } })) {
            Button("OK") { error = nil }
        } message: { Text(error ?? "Please choose another file.") }
    }

    private func prepareFile(_ url: URL) {
        guard context != nil else { return }
        loading = true
        Task {
            defer { loading = false }
            let accessible = url.startAccessingSecurityScopedResource()
            defer { if accessible { url.stopAccessingSecurityScopedResource() } }
            do {
                let type = try url.resourceValues(forKeys: [.contentTypeKey]).contentType
                    ?? UTType(filenameExtension: url.pathExtension) ?? .data
                if type.conforms(to: .movie) {
                    pendingAttachment = try await PendingAttachment.video(from: url)
                } else {
                    let outgoing = try await Task.detached(priority: .userInitiated) {
                        let values = try url.resourceValues(forKeys: [.fileSizeKey, .isRegularFileKey])
                        guard values.isRegularFile == true, let size = values.fileSize, size > 0, size <= OutgoingAttachment.maximumBytes else {
                            throw APIError.server("Choose a file smaller than 64 MB.")
                        }
                        return OutgoingAttachment(data: try Data(contentsOf: url), mimeType: type.preferredMIMEType ?? "application/octet-stream", fileName: url.lastPathComponent, kind: "document")
                    }.value
                    pendingAttachment = PendingAttachment(outgoing: outgoing, previewURL: nil)
                }
            } catch { self.error = error.localizedDescription }
        }
    }
}

private struct ImportedMovie: Transferable {
    let url: URL
    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(importedContentType: .movie) { received in
            let copy = FileManager.default.temporaryDirectory.appendingPathComponent("import-\(UUID().uuidString).\(received.file.pathExtension)")
            try FileManager.default.copyItem(at: received.file, to: copy)
            return ImportedMovie(url: copy)
        }
    }
}

private final class PendingAttachment: Identifiable {
    let id = UUID()
    let outgoing: OutgoingAttachment
    let previewURL: URL?
    init(outgoing: OutgoingAttachment, previewURL: URL?) {
        self.outgoing = outgoing
        self.previewURL = previewURL
    }
    deinit { if let previewURL { try? FileManager.default.removeItem(at: previewURL) } }

    static func video(from url: URL) async throws -> PendingAttachment {
        let asset = AVURLAsset(url: url)
        guard let exporter = AVAssetExportSession(asset: asset, presetName: AVAssetExportPreset1280x720) else {
            throw APIError.server("This video couldn’t be prepared for WhatsApp.")
        }
        let output = FileManager.default.temporaryDirectory.appendingPathComponent("video-\(UUID().uuidString).mp4")
        do {
            exporter.shouldOptimizeForNetworkUse = true
            try await exporter.export(to: output, as: .mp4)
            let size = try output.resourceValues(forKeys: [.fileSizeKey]).fileSize ?? 0
            guard size > 0, size <= OutgoingAttachment.maximumBytes else {
                throw APIError.server("This video is too large. Choose a shorter clip (up to 64 MB after preparation).")
            }
            let data = try Data(contentsOf: output)
            return PendingAttachment(outgoing: OutgoingAttachment(data: data, mimeType: "video/mp4", fileName: "Video.mp4", kind: "video"), previewURL: output)
        } catch {
            try? FileManager.default.removeItem(at: output)
            throw error
        }
    }
}

private struct AttachmentComposerSheet: View {
    @Environment(AppSession.self) private var session
    @Environment(\.dismiss) private var dismiss
    let attachment: PendingAttachment
    let context: UnifiedMediaContext
    let onSent: () -> Void
    @State private var caption = ""
    @State private var sending = false
    @State private var attemptedCaption: String?
    @State private var hasAttemptedSend = false
    @State private var failed = false
    @State private var player: AVPlayer?

    var body: some View {
        NavigationStack {
            VStack(spacing: 18) {
                if let player { VideoPlayer(player: player).frame(minHeight: 180, maxHeight: 360) }
                else {
                    Image(systemName: "doc.fill").font(.system(size: 60)).foregroundStyle(.secondary)
                    Text(attachment.outgoing.fileName).font(.headline).lineLimit(3)
                }
                VStack(alignment: .leading, spacing: 5) {
                    Text("To: \(context.destination)").font(.headline)
                    Text(context.reply.text).font(.caption).lineLimit(2).foregroundStyle(.secondary)
                }.frame(maxWidth: .infinity, alignment: .leading)
                TextField("Add a caption", text: $caption, axis: .vertical)
                    .lineLimit(1...4).textFieldStyle(.roundedBorder).disabled(sending || hasAttemptedSend)
                if sending { ProgressView("Sending attachment…") }
                if failed { Text("The attachment wasn’t confirmed. Try again to check its delivery safely.").font(.caption).foregroundStyle(.red) }
                Spacer(minLength: 0)
            }
            .padding()
            .navigationTitle(attachment.outgoing.kind == "video" ? "Send video" : "Send file")
            .platformInlineNavigationTitle()
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() }.disabled(sending) }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Send", systemImage: "paperplane.fill") {
                        guard !sending else { return }
                        sending = true; failed = false; player?.pause()
                        if !hasAttemptedSend {
                            let clean = caption.trimmingCharacters(in: .whitespacesAndNewlines)
                            attemptedCaption = clean.isEmpty ? nil : clean
                            hasAttemptedSend = true
                        }
                        Task {
                            let sent = await session.sendAttachment(attachment.outgoing, caption: attemptedCaption, to: context.message.contactId, reply: context.reply, replyOnlyIfNotLatest: true)
                            sending = false
                            if sent { onSent(); dismiss() } else { failed = true }
                        }
                    }.disabled(sending)
                }
            }
        }
        .interactiveDismissDisabled(sending)
        .onAppear { if let url = attachment.previewURL { player = AVPlayer(url: url) } }
        .onDisappear { player?.pause(); player = nil }
        #if os(macOS)
        .frame(minWidth: 480, minHeight: 480)
        #endif
    }
}
