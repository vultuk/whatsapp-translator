import SwiftUI
import CoreImage.CIFilterBuiltins

struct WhatsAppLinkView: View {
    @Environment(AppSession.self) private var session

    var body: some View {
        ZStack {
            TranslatorBackdrop()
            ScrollView {
                VStack(spacing: 22) {
                    TranslatorMark(size: 64)
                    Text("Link WhatsApp")
                        .font(.largeTitle.bold())
                    Text("Your WhatsApp session needs linking. Your chats and server settings are saved.")
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    Group {
                        if let code = session.whatsAppQRCode, let image = qrImage(code) {
                            Image(image, scale: 1, label: Text("WhatsApp linking QR code"))
                                .interpolation(.none)
                                .resizable()
                                .scaledToFit()
                                .padding(20)
                                .background(.white, in: RoundedRectangle(cornerRadius: 20))
                                .accessibilityLabel("WhatsApp linking QR code")
                                .accessibilityIdentifier("whatsapp-link-qr")
                        } else {
                            VStack(spacing: 14) {
                                ProgressView()
                                Text("Preparing a fresh linking code…")
                                    .foregroundStyle(.secondary)
                            }
                            .frame(maxWidth: .infinity, minHeight: 260)
                        }
                    }
                    .frame(maxWidth: 300)
                    Text("On your main phone, open WhatsApp → Settings → Linked Devices → Link a Device, then scan this code.")
                        .multilineTextAlignment(.center)
                    Text("Using your main phone now? Open Babel Bridge on a computer or tablet to scan its code.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                    if let error = session.whatsAppLinkError {
                        Text(error).font(.footnote).foregroundStyle(.secondary)
                    }
                    Button("Check connection") { Task { await session.refreshWhatsAppConnection() } }
                        .buttonStyle(.bordered)
                    Text("This screen closes automatically once WhatsApp is linked.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(28)
                .frame(maxWidth: 510)
                .frame(maxWidth: .infinity)
            }
        }
    }

    private func qrImage(_ code: String) -> CGImage? {
        let filter = CIFilter.qrCodeGenerator()
        filter.message = Data(code.utf8)
        filter.correctionLevel = "M"
        guard let output = filter.outputImage else { return nil }
        return CIContext().createCGImage(output, from: output.extent)
    }
}

struct ConnectionSetupView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.translatorPalette) private var palette
    @State private var address = ""
    @State private var password = ""
    @FocusState private var focusedField: Field?

    private enum Field { case address, password }

    var body: some View {
        ZStack {
            TranslatorBackdrop()
            ScrollView {
                VStack(alignment: .leading, spacing: 28) {
                    Spacer(minLength: 42)
                    hero
                    connectionCard
                    securityNote
                    Spacer(minLength: 24)
                }
                .frame(maxWidth: 520)
                .padding(.horizontal, 24)
                .frame(maxWidth: .infinity)
            }
            .platformDismissesKeyboard()
        }
        .onAppear {
            address = session.configuration?.baseURL.absoluteString ?? ""
            password = session.configuration?.password ?? ""
        }
    }

    private var hero: some View {
        VStack(alignment: .leading, spacing: 18) {
            TranslatorMark(size: 74)
            VStack(alignment: .leading, spacing: 8) {
                Text("Your translator,\nnative on Apple devices.")
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .tracking(-1.1)
                Text("Connect to your existing Babel Bridge server and keep every conversation in its language.")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
    }

    private var connectionCard: some View {
        VStack(spacing: 18) {
            VStack(alignment: .leading, spacing: 8) {
                Label("Translator web address", systemImage: "network")
                    .font(.subheadline.weight(.semibold))
                TextField("https://your-translator.example.com", text: $address)
                    .platformURLInput()
                    .focused($focusedField, equals: .address)
                    .onSubmit { focusedField = .password }
                    .padding(14)
                    .background(.background.opacity(0.52), in: RoundedRectangle(cornerRadius: 15, style: .continuous))
            }

            VStack(alignment: .leading, spacing: 8) {
                Label("Password", systemImage: "lock.fill")
                    .font(.subheadline.weight(.semibold))
                SecureField("Backend password", text: $password)
                    .platformPasswordInput()
                    .focused($focusedField, equals: .password)
                    .onSubmit { connect() }
                    .padding(14)
                    .background(.background.opacity(0.52), in: RoundedRectangle(cornerRadius: 15, style: .continuous))
            }

            Button(action: connect) {
                HStack {
                    Spacer()
                    Text("Connect translator")
                    Image(systemName: "arrow.right")
                    Spacer()
                }
                .font(.headline)
                .padding(.vertical, 7)
            }
            .buttonStyle(.borderedProminent)
            .buttonBorderShape(.roundedRectangle(radius: 16))
            .disabled(address.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(20)
        .translatorGlass(in: RoundedRectangle(cornerRadius: 28, style: .continuous))
    }

    private var securityNote: some View {
        Label {
            Text("Your server address and password are kept in the system Keychain. Messages stay on your translator backend.")
        } icon: {
            Image(systemName: "checkmark.shield.fill")
                .foregroundStyle(palette.accent)
        }
        .font(.footnote)
        .foregroundStyle(.secondary)
        .padding(.horizontal, 4)
    }

    private func connect() {
        focusedField = nil
        Task { await session.connect(address: address, password: password) }
    }
}
