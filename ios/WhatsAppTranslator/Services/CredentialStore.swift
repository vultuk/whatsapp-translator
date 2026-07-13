import Foundation
import Security

struct CredentialStore: Sendable {
    private let service = "com.vultuk.whatsapptranslator.backend"
    private var sharedAccessGroup: String? {
        guard let rawValue = Bundle.main.object(forInfoDictionaryKey: "KeychainAccessGroup") as? String else {
            return nil
        }
        let value = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        return value.isEmpty ? nil : value
    }

    func load() -> ServerConfiguration? {
        if let address = read(account: "server", accessGroup: sharedAccessGroup),
           let password = read(account: "password", accessGroup: sharedAccessGroup) {
            makeAvailableAfterFirstUnlock(account: "server", accessGroup: sharedAccessGroup)
            makeAvailableAfterFirstUnlock(account: "password", accessGroup: sharedAccessGroup)
            return try? ServerConfiguration.make(address: address, password: password)
        }

        guard let address = read(account: "server", accessGroup: nil),
              let password = read(account: "password", accessGroup: nil),
              let configuration = try? ServerConfiguration.make(address: address, password: password)
        else { return nil }
        try? save(configuration)
        return configuration
    }

    func save(_ configuration: ServerConfiguration) throws {
        try write(configuration.baseURL.absoluteString, account: "server")
        try write(configuration.password, account: "password")
    }

    func clear() {
        delete(account: "server", accessGroup: sharedAccessGroup)
        delete(account: "password", accessGroup: sharedAccessGroup)
        delete(account: "server", accessGroup: nil)
        delete(account: "password", accessGroup: nil)
    }

    private func write(_ value: String, account: String) throws {
        let data = Data(value.utf8)
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ].adding(accessGroup: sharedAccessGroup)
        let attributes: [String: Any] = [
            kSecValueData as String: data,
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        let status = SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
        if status == errSecItemNotFound {
            var insert = query
            insert[kSecValueData as String] = data
            insert[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
            let insertStatus = SecItemAdd(insert as CFDictionary, nil)
            guard insertStatus == errSecSuccess else { throw KeychainError.status(insertStatus) }
        } else if status != errSecSuccess {
            throw KeychainError.status(status)
        }
    }

    private func read(account: String, accessGroup: String?) -> String? {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
            kSecReturnData as String: true,
            kSecMatchLimit as String: kSecMatchLimitOne,
        ].adding(accessGroup: accessGroup)
        var item: CFTypeRef?
        guard SecItemCopyMatching(query as CFDictionary, &item) == errSecSuccess,
              let data = item as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    private func delete(account: String, accessGroup: String?) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ].adding(accessGroup: accessGroup)
        SecItemDelete(query as CFDictionary)
    }

    private func makeAvailableAfterFirstUnlock(account: String, accessGroup: String?) {
        let query: [String: Any] = [
            kSecClass as String: kSecClassGenericPassword,
            kSecAttrService as String: service,
            kSecAttrAccount as String: account,
        ].adding(accessGroup: accessGroup)
        let attributes: [String: Any] = [
            kSecAttrAccessible as String: kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly,
        ]
        SecItemUpdate(query as CFDictionary, attributes as CFDictionary)
    }

    enum KeychainError: Error { case status(OSStatus) }
}

private extension Dictionary where Key == String, Value == Any {
    func adding(accessGroup: String?) -> Self {
        guard let accessGroup else { return self }
        var copy = self
        copy[kSecAttrAccessGroup as String] = accessGroup
        return copy
    }
}
