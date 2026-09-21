import SwiftUI
import Observation
#if os(iOS)
import UIKit
#else
import AppKit
#endif

enum AppIconChoice: String, CaseIterable, Identifiable, Sendable {
    case original, paperBridge, neonOrbit, sunrise, porcelain, botanical, pixel, ribbon, mono, mosaic, cosmic

    var id: String { rawValue }
    var assetName: String { "IconPreview-\(rawValue)" }
    var alternateName: String? { self == .original ? nil : "AppIcon-\(rawValue)" }
    var title: String {
        switch self {
        case .original: "Original"
        case .paperBridge: "Paper Bridge"
        case .neonOrbit: "Neon Orbit"
        case .sunrise: "Sunrise"
        case .porcelain: "Porcelain"
        case .botanical: "Botanical"
        case .pixel: "Pixel Passport"
        case .ribbon: "Ribbon"
        case .mono: "Monochrome"
        case .mosaic: "Mosaic"
        case .cosmic: "Cosmic Whale"
        }
    }

    static func from(alternateName: String?) -> Self {
        allCases.first { $0.alternateName == alternateName } ?? .original
    }
}

@MainActor
protocol AppIconSystem {
    /// iOS owns the persisted Home Screen choice. macOS restores our local preference.
    var currentIcon: AppIconChoice? { get }
    var isSupported: Bool { get }
    func apply(_ icon: AppIconChoice) async throws
}

@MainActor
private struct NativeAppIconSystem: AppIconSystem {
    var currentIcon: AppIconChoice? {
        #if os(iOS)
        AppIconChoice.from(alternateName: UIApplication.shared.alternateIconName)
        #else
        nil
        #endif
    }

    var isSupported: Bool {
        #if os(iOS)
        UIApplication.shared.supportsAlternateIcons
        #else
        true
        #endif
    }

    func apply(_ icon: AppIconChoice) async throws {
        #if os(iOS)
        try await UIApplication.shared.setAlternateIconName(icon.alternateName)
        #else
        guard icon != .original else {
            NSApplication.shared.applicationIconImage = nil
            return
        }
        guard let artwork = NSImage(named: icon.assetName) else {
            throw CocoaError(.fileReadNoSuchFile)
        }
        // Match the inset, rounded shape of native Dock icons without changing the signed bundle.
        let dockIcon = NSImage(size: NSSize(width: 512, height: 512))
        dockIcon.lockFocus()
        let rect = NSRect(x: 40, y: 40, width: 432, height: 432)
        NSBezierPath(roundedRect: rect, xRadius: 96, yRadius: 96).addClip()
        artwork.draw(in: rect)
        dockIcon.unlockFocus()
        NSApplication.shared.applicationIconImage = dockIcon
        #endif
    }
}

@MainActor
@Observable
final class AppIconController {
    static let storageKey = "babel-bridge-app-icon-v1"
    private let defaults: UserDefaults
    private let system: any AppIconSystem
    private var restored = false
    private(set) var selected: AppIconChoice
    private(set) var isChanging = false
    var error: String?
    var isSupported: Bool { system.isSupported }

    init(defaults: UserDefaults = .standard, system: (any AppIconSystem)? = nil) {
        self.defaults = defaults
        let system = system ?? NativeAppIconSystem()
        self.system = system
        selected = system.currentIcon
            ?? defaults.string(forKey: Self.storageKey).flatMap(AppIconChoice.init(rawValue:))
            ?? .original
    }

    func refresh() {
        if let current = system.currentIcon { selected = current }
    }

    func select(_ icon: AppIconChoice) async {
        guard !isChanging, icon != selected else { return }
        guard isSupported else {
            error = "Changing the app icon isn’t available on this device."
            return
        }
        isChanging = true
        error = nil
        defer { isChanging = false }
        do {
            try await system.apply(icon)
            selected = system.currentIcon ?? icon
            defaults.set(selected.rawValue, forKey: Self.storageKey)
        } catch {
            self.error = error.localizedDescription
        }
    }

    func restoreDockIcon() async {
        guard !restored else { return }
        restored = true
        #if os(macOS)
        do { try await system.apply(selected) }
        catch { self.error = error.localizedDescription }
        #endif
    }
}

struct AppIconPickerView: View {
    @Environment(AppSession.self) private var session
    @Environment(\.scenePhase) private var scenePhase

    var body: some View {
        @Bindable var icons = session.appIcons
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                Text("Make it yours")
                    .font(.title2.bold())
                Text(explanation)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                if !icons.isSupported {
                    Text("Changing the app icon isn’t available on this device.")
                        .foregroundStyle(.secondary)
                }
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 100), spacing: 16)], spacing: 20) {
                    ForEach(AppIconChoice.allCases) { icon in
                        Button {
                            Task { await icons.select(icon) }
                        } label: {
                            VStack(spacing: 10) {
                                Image(icon.assetName)
                                    .resizable().scaledToFit()
                                    .frame(width: 88, height: 88)
                                    .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
                                    .overlay(alignment: .bottomTrailing) {
                                        if icons.selected == icon {
                                            Image(systemName: "checkmark.circle.fill")
                                                .symbolRenderingMode(.palette)
                                                .foregroundStyle(.white, Color.accentColor)
                                                .font(.title3)
                                                .background(.background, in: Circle())
                                                .offset(x: 5, y: 5)
                                        }
                                    }
                                Text(icon.title)
                                    .font(.caption.weight(.medium))
                                    .foregroundStyle(.primary)
                                    .multilineTextAlignment(.center)
                                    .frame(minHeight: 30, alignment: .top)
                            }
                            .frame(maxWidth: .infinity)
                            .padding(.top, 10)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityIdentifier("app-icon-\(icon.rawValue)")
                        .accessibilityLabel(icon.title)
                        .accessibilityValue(icons.selected == icon ? "Selected" : "")
                        .accessibilityAddTraits(icons.selected == icon ? .isSelected : [])
                        .disabled(icons.isChanging || !icons.isSupported)
                    }
                }
                if icons.isChanging { ProgressView("Changing icon…") }
            }
            .padding(20)
            .frame(maxWidth: 650)
            .frame(maxWidth: .infinity)
        }
        .navigationTitle("App icon")
        .platformInlineNavigationTitle()
        .onAppear { icons.refresh() }
        .onChange(of: scenePhase) { _, phase in
            if phase == .active { icons.refresh() }
        }
        .alert("Couldn’t change icon", isPresented: Binding(
            get: { icons.error != nil },
            set: { if !$0 { icons.error = nil } }
        )) {
            Button("OK") { icons.error = nil }
        } message: {
            Text(icons.error ?? "Please try again.")
        }
    }

    private var explanation: String {
        #if os(macOS)
        "Choose the icon shown in the Dock while Babel Bridge is running. Your choice is remembered on this Mac. The Finder icon stays the original."
        #else
        "Choose a new look for Babel Bridge on your Home Screen. You can return to the original at any time."
        #endif
    }
}
