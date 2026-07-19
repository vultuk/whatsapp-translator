import CryptoKit
import Foundation

actor MediaCacheStore {
    static let shared = MediaCacheStore()

    private let directoryURL: URL
    private let maximumBytes: Int

    init(directoryURL: URL? = nil, maximumBytes: Int = 512 * 1_024 * 1_024) {
        self.directoryURL = directoryURL
            ?? FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0]
                .appending(path: "WhatsAppTranslator/MediaCache", directoryHint: .isDirectory)
        self.maximumBytes = maximumBytes
    }

    func cachedURL(for messageID: String) throws -> URL? {
        guard FileManager.default.fileExists(atPath: directoryURL.path) else { return nil }
        let prefix = "\(cacheKey(for: messageID))."
        let files = try FileManager.default.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: [.isRegularFileKey],
            options: [.skipsHiddenFiles]
        )
        guard let url = files.first(where: { $0.lastPathComponent.hasPrefix(prefix) }) else {
            return nil
        }
        try? FileManager.default.setAttributes(
            [.modificationDate: Date()],
            ofItemAtPath: url.path
        )
        return url
    }

    @discardableResult
    func store(_ data: Data, messageID: String, fileExtension: String) throws -> URL {
        try FileManager.default.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        try remove(messageID: messageID)

        let safeExtension = fileExtension
            .lowercased()
            .filter { $0.isLetter || $0.isNumber }
        let url = directoryURL.appending(
            path: "\(cacheKey(for: messageID)).\(safeExtension.isEmpty ? "bin" : safeExtension)",
            directoryHint: .notDirectory
        )
        try data.write(
            to: url,
            options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication]
        )
        var resourceValues = URLResourceValues()
        resourceValues.isExcludedFromBackup = true
        var mutableURL = url
        try? mutableURL.setResourceValues(resourceValues)
        try trimIfNeeded()
        return url
    }

    func remove(messageID: String) throws {
        guard FileManager.default.fileExists(atPath: directoryURL.path) else { return }
        let prefix = "\(cacheKey(for: messageID))."
        let files = try FileManager.default.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: nil,
            options: [.skipsHiddenFiles]
        )
        for url in files where url.lastPathComponent.hasPrefix(prefix) {
            try FileManager.default.removeItem(at: url)
        }
    }

    func clear() {
        try? FileManager.default.removeItem(at: directoryURL)
    }

    private func cacheKey(for messageID: String) -> String {
        SHA256.hash(data: Data(messageID.utf8)).map { String(format: "%02x", $0) }.joined()
    }

    private func trimIfNeeded() throws {
        let keys: Set<URLResourceKey> = [.fileSizeKey, .contentModificationDateKey, .isRegularFileKey]
        let files = try FileManager.default.contentsOfDirectory(
            at: directoryURL,
            includingPropertiesForKeys: Array(keys),
            options: [.skipsHiddenFiles]
        )
        let entries = try files.compactMap { url -> (url: URL, size: Int, date: Date)? in
            let values = try url.resourceValues(forKeys: keys)
            guard values.isRegularFile == true else { return nil }
            return (url, values.fileSize ?? 0, values.contentModificationDate ?? .distantPast)
        }
        var totalBytes = entries.reduce(0) { $0 + $1.size }
        let removableEntries = entries
            .sorted(by: { ($0.date, $0.url.path) < ($1.date, $1.url.path) })
            .dropLast()
        for entry in removableEntries where totalBytes > maximumBytes {
            try FileManager.default.removeItem(at: entry.url)
            totalBytes -= entry.size
        }
    }
}
