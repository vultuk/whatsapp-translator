import Foundation

enum MessageTone: String, Codable, CaseIterable, Identifiable, Sendable {
    case `default`, silent, aurora, bamboo, bloom, droplet, glass, orbit

    var id: String { rawValue }
    var filename: String? {
        switch self {
        case .default, .silent: nil
        default: "bb-\(rawValue).wav"
        }
    }

    var title: String {
        switch self {
        case .default: "System default"
        case .silent: "Silent"
        default: descriptor?.title ?? rawValue.capitalized
        }
    }

    var detail: String {
        switch self {
        case .default: "This device’s standard notification sound"
        case .silent: "Show notifications without a sound"
        default: descriptor?.detail ?? "Custom message ringtone"
        }
    }

    var symbol: String {
        switch self {
        case .default: "speaker.wave.2"
        case .silent: "bell.slash"
        default: descriptor?.symbol ?? "music.note"
        }
    }

    private var descriptor: MessageToneDescriptor? { Self.catalog.first { $0.id == self } }
    static let customTones = allCases.filter { $0.filename != nil }
    static let catalog: [MessageToneDescriptor] = {
        guard let url = Bundle.main.url(forResource: "message-tones", withExtension: "json"),
              let data = try? Data(contentsOf: url),
              let catalog = try? JSONDecoder().decode([MessageToneDescriptor].self, from: data) else { return [] }
        return catalog
    }()

    func resourceURL(in bundle: Bundle = .main) -> URL? {
        guard let filename else { return nil }
        return bundle.url(forResource: filename, withExtension: nil)
    }
}

struct MessageToneDescriptor: Decodable, Sendable {
    let id: MessageTone
    let title: String
    let detail: String
    let symbol: String
    let filename: String
}

struct MessageToneSettings: Codable, Equatable, Sendable {
    var tone: MessageTone?
    var globalTone: MessageTone
    var effectiveTone: MessageTone

    static let initial = Self(tone: .default, globalTone: .default, effectiveTone: .default)
}

struct MessageToneUpdate: Encodable, Sendable {
    let tone: MessageTone?
    private enum CodingKeys: String, CodingKey { case tone }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        // Explicit null restores inheritance; omitting the field is not a valid update.
        try container.encode(tone, forKey: .tone)
    }
}
