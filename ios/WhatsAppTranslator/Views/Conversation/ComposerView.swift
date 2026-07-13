import PhotosUI
import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct ComposerView: View {
    @Environment(\.translatorPalette) private var palette
    @Binding var text: String
    let reply: MessageReplyTarget?
    let isSending: Bool
    let cancelReply: () -> Void
    let sendImage: (Data, String) -> Void
    let send: () -> Void
    @State private var selectedPhoto: PhotosPickerItem?
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
                    Image(systemName: "plus")
                        .font(.system(size: 18, weight: .semibold))
                        .frame(width: 36, height: 36)
                        .background(.ultraThinMaterial, in: Circle())
                        .overlay(Circle().stroke(.white.opacity(0.22), lineWidth: 0.5))
                }
                .disabled(isSending)
                .accessibilityLabel("Send image")

                TextField("Message", text: $text, axis: .vertical)
                    .lineLimit(1...6)
                    .focused($focused)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
                    .translatorGlass(in: RoundedRectangle(cornerRadius: 22, style: .continuous))

                Button {
                    guard !isSending else { return }
                    send()
                } label: {
                    Group {
                        if isSending {
                            ProgressView()
                                .controlSize(.small)
                                .tint(.white)
                                .transition(.scale.combined(with: .opacity))
                        } else {
                            Image(systemName: "paperplane.fill")
                                .transition(.scale.combined(with: .opacity))
                        }
                    }
                    .font(.system(size: 16, weight: .semibold))
                    .frame(width: 36, height: 36)
                }
                .buttonStyle(.borderedProminent)
                .buttonBorderShape(.circle)
                .controlSize(.small)
                .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty && !isSending)
                .allowsHitTesting(!isSending)
                .accessibilityLabel(isSending ? "Sending message" : "Send message")
                .accessibilityValue(isSending ? "In progress" : "")
                .animation(.snappy, value: isSending)
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
                guard let data = try? await item.loadTransferable(type: Data.self) else { return }
                let allowed = ["image/jpeg", "image/png", "image/gif", "image/webp"]
                let selectedMimeType = item.supportedContentTypes
                    .first(where: { type in type.preferredMIMEType.map(allowed.contains) ?? false })?
                    .preferredMIMEType
                if let selectedMimeType {
                    sendImage(data, selectedMimeType)
                } else if let image = UIImage(data: data), let jpeg = image.jpegData(compressionQuality: 0.9) {
                    sendImage(jpeg, "image/jpeg")
                }
            }
        }
    }
}
