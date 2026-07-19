import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ComposerView: View {
    @Environment(\.translatorPalette) private var palette
    @Binding var text: String
    let reply: MessageReplyTarget?
    let isSending: Bool
    let cancelReply: () -> Void
    let sendImages: ([OutgoingImage], String?) async -> Bool
    let send: () -> Void
    @State private var selectedPhotos: [PhotosPickerItem] = []
    @State private var pendingPhotos: PendingPhotoSelection?
    @State private var pickerError: String?
    @FocusState private var focused: Bool

    var body: some View {
        VStack(spacing: 0) {
            if let reply {
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
            }

            HStack(alignment: .bottom, spacing: 9) {
                PhotosPicker(selection: $selectedPhotos, maxSelectionCount: 30, matching: .images) {
                    addImageLabel
                }
                .disabled(isSending)
                .accessibilityLabel("Send photos")

                composerInput

                sendButton
            }
            .padding(.horizontal, 12)
            .padding(.top, reply == nil ? 8 : 2)
            .padding(.bottom, 9)
        }
        .background(.ultraThinMaterial)
        .onChange(of: selectedPhotos) { _, items in
            guard !items.isEmpty else { return }
            Task {
                defer { selectedPhotos = [] }
                var prepared: [PendingPhoto] = []
                for item in items {
                    guard let photo = await preparePhoto(item) else {
                        pickerError = "One of the selected photos couldn’t be read. Please choose the photos again."
                        return
                    }
                    prepared.append(photo)
                }
                pendingPhotos = PendingPhotoSelection(photos: prepared)
            }
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
            guard arguments.contains("-demoImageComposer") || arguments.contains("-demoImageAlbumComposer"), pendingPhotos == nil else { return }
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

    @ViewBuilder
    private var addImageLabel: some View {
        #if os(macOS)
        Image(systemName: "plus")
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(.secondary)
            .frame(width: 34, height: 34)
            .background(.primary.opacity(0.06), in: RoundedRectangle(cornerRadius: 9))
            .contentShape(RoundedRectangle(cornerRadius: 9))
        #else
        Image(systemName: "plus")
            .font(.system(size: 18, weight: .semibold))
            .frame(width: 36, height: 36)
            .background(.ultraThinMaterial, in: Circle())
            .overlay(Circle().stroke(.white.opacity(0.22), lineWidth: 0.5))
        #endif
    }

    @ViewBuilder
    private var sendButton: some View {
        #if os(macOS)
        Button {
            guard !isSending else { return }
            send()
        } label: {
            sendButtonLabel
        }
        .buttonStyle(.plain)
        .foregroundStyle(.white)
        .frame(width: 34, height: 34)
        .background(palette.accent, in: RoundedRectangle(cornerRadius: 9))
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
        }
        .buttonStyle(.plain)
        .foregroundStyle(isSendDisabled ? Color.platformSecondaryLabel : .white)
        .background(
            isSendDisabled ? Color.primary.opacity(0.08) : palette.accent,
            in: Circle()
        )
        .overlay(Circle().stroke(.primary.opacity(0.08), lineWidth: 0.5))
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

    private func preparePhoto(_ item: PhotosPickerItem) async -> PendingPhoto? {
        guard let data = try? await item.loadTransferable(type: Data.self),
              let image = PlatformImage(data: data) else {
            return nil
        }
        let allowed = ["image/jpeg", "image/png", "image/gif", "image/webp"]
        if let mimeType = item.supportedContentTypes
            .first(where: { type in type.preferredMIMEType.map(allowed.contains) ?? false })?
            .preferredMIMEType {
            return PendingPhoto(data: data, mimeType: mimeType, image: image)
        }
        guard let jpeg = image.platformJPEGData(compressionQuality: 0.9) else { return nil }
        return PendingPhoto(data: jpeg, mimeType: "image/jpeg", image: image)
    }
}

private struct ComposerInputStyle: ViewModifier {
    @ViewBuilder
    func body(content: Content) -> some View {
        #if os(macOS)
        content
            .textFieldStyle(.plain)
            .padding(.horizontal, 13)
            .padding(.vertical, 8)
            .background(.regularMaterial, in: RoundedRectangle(cornerRadius: 10))
            .overlay {
                RoundedRectangle(cornerRadius: 10)
                    .stroke(.primary.opacity(0.09), lineWidth: 0.5)
            }
        #else
        content
            .padding(.horizontal, 16)
            .padding(.vertical, 12)
            .translatorGlass(in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        #endif
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
    let send: ([OutgoingImage], String?) async -> Bool
    @State private var caption = ""
    @State private var isSending = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                photoPreview

                if photos.count > 1 {
                    Label("\(photos.count) photos selected", systemImage: "photo.stack.fill")
                        .font(.subheadline.weight(.semibold))
                        .foregroundStyle(.secondary)
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
            .platformInteractiveDismissDisabled(isSending)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") { dismiss() }
                        .disabled(isSending)
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button {
                        isSending = true
                        Task {
                            let cleanCaption = caption.trimmingCharacters(in: .whitespacesAndNewlines)
                            let images = photos.map { OutgoingImage(data: $0.data, mimeType: $0.mimeType) }
                            if await send(images, cleanCaption.isEmpty ? nil : cleanCaption) {
                                dismiss()
                            } else {
                                isSending = false
                            }
                        }
                    } label: {
                        if isSending { ProgressView() } else { Label("Send", systemImage: "paperplane.fill") }
                    }
                    .fontWeight(.semibold)
                    .disabled(isSending)
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
