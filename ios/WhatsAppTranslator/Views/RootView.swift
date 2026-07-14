import SwiftUI

struct RootView: View {
    @Environment(AppSession.self) private var session

    var body: some View {
        Group {
            switch session.phase {
            case .restoring:
                LaunchView(label: "Restoring your translator…")
            case .connecting:
                LaunchView(label: "Connecting securely…")
            case .needsConfiguration:
                ConnectionSetupView()
            case .ready:
                ChatListView()
            }
        }
        .animation(.snappy, value: session.phase)
        .alert(session.errorTitle, isPresented: errorPresented) {
            Button("OK") { session.errorMessage = nil }
        } message: {
            Text(session.errorMessage ?? "Something went wrong.")
        }
    }

    private var errorPresented: Binding<Bool> {
        Binding(
            get: { session.errorMessage != nil },
            set: { if !$0 { session.errorMessage = nil } }
        )
    }
}

private struct LaunchView: View {
    let label: String

    var body: some View {
        ZStack {
            TranslatorBackdrop()
            VStack(spacing: 18) {
                TranslatorMark(size: 70)
                ProgressView()
                    .controlSize(.large)
                Text(label)
                    .font(.headline)
                    .foregroundStyle(.secondary)
            }
        }
    }
}
