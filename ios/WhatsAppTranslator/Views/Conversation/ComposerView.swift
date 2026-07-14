import PhotosUI
import SwiftUI
import UniformTypeIdentifiers

struct ComposerView: View {
    @Environment(\.translatorPalette) private var palette
    @Binding var text: String
    let reply: MessageReplyTarget?
    let isSending: Bool
    let cancelReply: () -> Void
    let sendImage: (Data, String, String?) async -> Bool
    let send: () -> Void
    @State private var selectedPhoto: PhotosPickerItem?
    @State private var pendingPhoto: PendingPhoto?
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
                PhotosPicker(selection: $selectedPhoto, matching: .images) {
                    addImageLabel
                }
                .disabled(isSending)
                .accessibilityLabel("Send image")

                TextField("Message", text: $text, axis: .vertical)
                    .lineLimit(1...6)
                    .focused($focused)
                    .modifier(ComposerInputStyle())

                sendButton
            }
            .padding(.horizontal, 12)
            .padding(.top, reply == nil ? 8 : 2)
            .padding(.bottom, 9)
        }
        .background(.ultraThinMaterial)
        .onChange(of: selectedPhoto) { _, item in
            guard let item else { return }
            Task {
                defer { selectedPhoto = nil }
                guard let data = try? await item.loadTransferable(type: Data.self) else {
                    pickerError = "The selected image couldn’t be read. Please choose another image."
                    return
                }
                let allowed = ["image/jpeg", "image/png", "image/gif", "image/webp"]
                let selectedMimeType = item.supportedContentTypes
                    .first(where: { type in type.preferredMIMEType.map(allowed.contains) ?? false })?
                    .preferredMIMEType
                if let selectedMimeType {
                    guard let image = PlatformImage(data: data) else {
                        pickerError = "The selected file isn’t a supported image."
                        return
                    }
                    pendingPhoto = PendingPhoto(data: data, mimeType: selectedMimeType, image: image)
                } else if let image = PlatformImage(data: data),
                          let jpeg = image.platformJPEGData(compressionQuality: 0.9) {
                    pendingPhoto = PendingPhoto(data: jpeg, mimeType: "image/jpeg", image: image)
                } else {
                    pickerError = "The selected file isn’t a supported image."
                }
            }
        }
        .sheet(item: $pendingPhoto) { photo in
            ImageComposerSheet(photo: photo, reply: reply, send: sendImage)
        }
        .alert("Couldn’t prepare image", isPresented: pickerErrorPresented) {
            Button("OK") { pickerError = nil }
        } message: {
            Text(pickerError ?? "Please choose another image.")
        }
        .task {
            guard ProcessInfo.processInfo.arguments.contains("-demoImageComposer"), pendingPhoto == nil else { return }
            let image = DemoImageFactory.landscape(size: CGSize(width: 800, height: 600))
            if let data = image.platformJPEGData(compressionQuality: 0.9) {
                pendingPhoto = PendingPhoto(data: data, mimeType: "image/jpeg", image: image)
            }
        }
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

private struct ImageComposerSheet: View {
    @Environment(\.dismiss) private var dismiss
    let photo: PendingPhoto
    let reply: MessageReplyTarget?
    let send: (Data, String, String?) async -> Bool
    @State private var caption = ""
    @State private var isSending = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 16) {
                Image(platformImage: photo.image)
                    .resizable()
                    .scaledToFit()
                    .frame(maxWidth: .infinity, maxHeight: 440)
                    .clipShape(RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .padding(.horizontal)

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
            .navigationTitle("Send image")
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
                            if await send(photo.data, photo.mimeType, cleanCaption.isEmpty ? nil : cleanCaption) {
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
}
