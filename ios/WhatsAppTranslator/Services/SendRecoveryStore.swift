import CryptoKit
import Foundation

/// Only fingerprints and retry identifiers are stored here; message bodies and passwords are never saved.
actor SendRecoveryStore {
    static let shared = SendRecoveryStore()
    struct Identity: Sendable { let fingerprint: String; let key: String }
    private let file: URL
    private var entries: [String: String]?

    init(directory: URL = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]) {
        file = directory.appendingPathComponent("BabelBridgeSendRecovery.json")
    }

    static func isSend(path: String, method: String) -> Bool {
        method == "POST" && (["/api/send", "/api/send-image", "/api/send-images", "/api/react", "/api/voice/send"].contains(path)
            || (path.hasPrefix("/api/photo-albums/") && path.hasSuffix("/send")))
    }

    func identity(server: URL, path: String, method: String, body: Data?) throws -> Identity? {
        guard Self.isSend(path: path, method: method) else { return nil }
        if entries == nil {
            if FileManager.default.fileExists(atPath: file.path) {
                entries = try JSONDecoder().decode([String: String].self, from: Data(contentsOf: file))
            } else { entries = [:] }
        }
        var bytes = Data("\(server.absoluteString)\n\(path)\n".utf8)
        if let body, let json = try? JSONSerialization.jsonObject(with: body) {
            bytes.append(try JSONSerialization.data(withJSONObject: json, options: [.sortedKeys]))
        } else { bytes.append(body ?? Data()) }
        let fingerprint = SHA256.hash(data: bytes).map { String(format: "%02x", $0) }.joined()
        if let key = entries?[fingerprint] { return Identity(fingerprint: fingerprint, key: key) }
        let identity = Identity(fingerprint: fingerprint, key: UUID().uuidString)
        entries?[fingerprint] = identity.key
        do { try save() } catch { entries?.removeValue(forKey: fingerprint); throw error }
        return identity
    }

    func settle(_ identity: Identity?, response: HTTPURLResponse) throws {
        guard let identity else { return }
        guard entries?[identity.fingerprint] == identity.key else { return }
        let state = response.value(forHTTPHeaderField: "X-Delivery-State")
        guard state == "confirmed" || state == "failed" || (state == nil && (200..<300).contains(response.statusCode)) else { return }
        entries?.removeValue(forKey: identity.fingerprint)
        do { try save() } catch { entries?[identity.fingerprint] = identity.key; throw error }
    }

    private func save() throws {
        try FileManager.default.createDirectory(at: file.deletingLastPathComponent(), withIntermediateDirectories: true)
        try JSONEncoder().encode(entries ?? [:]).write(to: file, options: .atomic)
    }
}
