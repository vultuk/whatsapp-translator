import SwiftUI

struct ComposerView: View {
    @Binding var text: String
    let isSending: Bool
    let send: () -> Void
    @FocusState private var focused: Bool

    var body: some View {
        HStack(alignment: .bottom, spacing: 10) {
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
        .padding(.top, 8)
        .padding(.bottom, 9)
        .background(.ultraThinMaterial)
    }
}
