import XCTest
#if os(macOS)
import AppKit
@testable import BabelBridgeMac
#else
import UIKit
@testable import WhatsAppTranslator
#endif

@MainActor
final class AppIconTests: XCTestCase {
    private final class System: AppIconSystem {
        var currentIcon: AppIconChoice?
        var isSupported = true
        var failure = false
        var applied: [AppIconChoice] = []
        func apply(_ icon: AppIconChoice) async throws {
            if failure { throw CocoaError(.fileReadUnknown) }
            applied.append(icon)
        }
    }

    private func defaults() -> UserDefaults {
        let name = "AppIconTests-\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: name)!
        addTeardownBlock { defaults.removePersistentDomain(forName: name) }
        return defaults
    }

    func testChoicePersistsAcrossControllerRecreationAndOriginalCanBeRestored() async {
        let defaults = defaults()
        let system = System()
        let icons = AppIconController(defaults: defaults, system: system)
        await icons.select(.mosaic)
        XCTAssertEqual(system.applied, [.mosaic])
        let reopened = AppIconController(defaults: defaults, system: system)
        XCTAssertEqual(reopened.selected, .mosaic)
        await reopened.select(.original)
        XCTAssertEqual(AppIconController(defaults: defaults, system: system).selected, .original)
    }

    func testFailureKeepsPreviousSelectionAndStoredPreference() async {
        let defaults = defaults()
        let system = System()
        let icons = AppIconController(defaults: defaults, system: system)
        await icons.select(.ribbon)
        system.failure = true
        await icons.select(.porcelain)
        XCTAssertEqual(icons.selected, .ribbon)
        XCTAssertEqual(defaults.string(forKey: AppIconController.storageKey), "ribbon")
        XCTAssertNotNil(icons.error)
        XCTAssertFalse(icons.isChanging)
    }

    func testSystemSelectionTakesPrecedenceOverStalePreference() {
        let defaults = defaults()
        defaults.set("mosaic", forKey: AppIconController.storageKey)
        let system = System()
        system.currentIcon = .neonOrbit
        let icons = AppIconController(defaults: defaults, system: system)
        XCTAssertEqual(icons.selected, .neonOrbit)
        system.currentIcon = .original
        icons.refresh()
        XCTAssertEqual(icons.selected, .original)
    }

    func testUnknownSavedIconFallsBackWithoutChangingOtherPreferences() {
        let defaults = defaults()
        defaults.set("retired-icon", forKey: AppIconController.storageKey)
        defaults.set("kept", forKey: "other-preference")
        XCTAssertEqual(AppIconController(defaults: defaults, system: System()).selected, .original)
        XCTAssertEqual(defaults.string(forKey: "other-preference"), "kept")
    }

    func testUnsupportedDeviceDoesNotApplyOrPersistChoice() async {
        let defaults = defaults()
        let system = System()
        system.isSupported = false
        let icons = AppIconController(defaults: defaults, system: system)
        await icons.select(.pixel)
        XCTAssertTrue(system.applied.isEmpty)
        XCTAssertNil(defaults.string(forKey: AppIconController.storageKey))
        XCTAssertEqual(icons.selected, .original)
        XCTAssertNotNil(icons.error)
    }

    func testAllElevenChoicesHavePackagedArtworkAndIOSAlternateDeclarations() throws {
        XCTAssertEqual(AppIconChoice.allCases.count, 11)
        for icon in AppIconChoice.allCases {
            #if os(macOS)
            XCTAssertNotNil(NSImage(named: icon.assetName), icon.title)
            #else
            XCTAssertNotNil(UIImage(named: icon.assetName), icon.title)
            #endif
        }
        #if os(iOS)
        // Bundle.infoDictionary resolves platform-specific keys for the current device.
        // Inspect the packaged plist to verify both iPhone and iPad declarations.
        let data = try Data(contentsOf: Bundle.main.bundleURL.appendingPathComponent("Info.plist"))
        let packagedInfo = try XCTUnwrap(PropertyListSerialization.propertyList(from: data, format: nil) as? [String: Any])
        for key in ["CFBundleIcons", "CFBundleIcons~ipad"] {
            let icons = try XCTUnwrap(packagedInfo[key] as? [String: Any], key)
            let alternates = try XCTUnwrap(icons["CFBundleAlternateIcons"] as? [String: Any], key)
            for icon in AppIconChoice.allCases where icon != .original {
                XCTAssertNotNil(alternates[icon.alternateName!], "\(key): \(icon.title)")
            }
        }
        #endif
    }

    #if os(macOS)
    func testDockRestorationAppliesSavedChoiceOnlyOnce() async {
        let defaults = defaults()
        defaults.set("cosmic", forKey: AppIconController.storageKey)
        let system = System()
        let icons = AppIconController(defaults: defaults, system: system)
        await icons.restoreDockIcon()
        await icons.restoreDockIcon()
        XCTAssertEqual(system.applied, [.cosmic])
    }
    #endif
}
