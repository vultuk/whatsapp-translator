import SwiftUI

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
            .scrollDismissesKeyboard(.interactively)
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
                Text("Your translator,\nnative on iPhone.")
                    .font(.system(size: 38, weight: .bold, design: .rounded))
                    .tracking(-1.1)
                Text("Connect to your existing WhatsApp Translator and keep every conversation in its language.")
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
                    .textInputAutocapitalization(.never)
                    .keyboardType(.URL)
                    .textContentType(.URL)
                    .submitLabel(.next)
                    .focused($focusedField, equals: .address)
                    .onSubmit { focusedField = .password }
                    .padding(14)
                    .background(.background.opacity(0.52), in: RoundedRectangle(cornerRadius: 15, style: .continuous))
            }

            VStack(alignment: .leading, spacing: 8) {
                Label("Password", systemImage: "lock.fill")
                    .font(.subheadline.weight(.semibold))
                SecureField("Backend password", text: $password)
                    .textContentType(.password)
                    .submitLabel(.go)
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
            Text("Your server address and password are kept in the iOS Keychain. Messages stay on your translator backend.")
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
